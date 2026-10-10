//! The inspecting reverse proxy: one loopback listener per inspected ingress rule, forwarding
//! to exactly one upstream and recording each exchange.

pub mod capture;
mod tls;

use std::convert::Infallible;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bytes::Bytes;
use http_body_util::{combinators::BoxBody, BodyExt, Empty, Full};
use hyper::body::Incoming;
use hyper::header::{self, HeaderMap, HeaderName, HeaderValue};
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode, Uri};
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::Client;
use hyper_util::rt::{TokioExecutor, TokioIo};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

use crate::derived::Upstream;
use capture::{BodyCapture, CapturedExchange, ExchangeKind, Pending, SharedOptions, Side, Sink, TeeBody};

type ProxyBody = BoxBody<Bytes, hyper::Error>;
type UpstreamClient = Client<TimeoutConnector<hyper_rustls::HttpsConnector<HttpConnector>>, ProxyBody>;

/// Bounds TCP connect plus TLS handshake (`connectTimeout` + `tlsTimeout`).
#[derive(Clone)]
struct TimeoutConnector<C> {
    inner: C,
    timeout: Duration,
}

type BoxError = Box<dyn std::error::Error + Send + Sync>;

impl<C> tower_service::Service<Uri> for TimeoutConnector<C>
where
    C: tower_service::Service<Uri> + Send,
    C::Future: Send + 'static,
    C::Error: Into<BoxError>,
{
    type Response = C::Response;
    type Error = BoxError;
    type Future = Pin<Box<dyn Future<Output = Result<C::Response, BoxError>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx).map_err(Into::into)
    }

    fn call(&mut self, uri: Uri) -> Self::Future {
        let fut = self.inner.call(uri);
        let timeout = self.timeout;
        Box::pin(async move {
            match tokio::time::timeout(timeout, fut).await {
                Ok(r) => r.map_err(Into::into),
                Err(_) => Err(format!("timed out after {}s connecting to the origin", timeout.as_secs()).into()),
            }
        })
    }
}

/// Everything a listener needs besides its upstream.
#[derive(Clone)]
pub struct ProxyContext {
    pub tunnel_id: String,
    pub options: SharedOptions,
    pub sink: Sink,
}

/// A running listener. Dropping it stops accepting connections.
pub struct ProxyHandle {
    pub addr: SocketAddr,
    pub upstream: Upstream,
    task: JoinHandle<()>,
}

impl ProxyHandle {
    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    pub fn is_running(&self) -> bool {
        !self.task.is_finished()
    }
}

impl Drop for ProxyHandle {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Bind `127.0.0.1:port` (0 = any free port) and start serving.
pub async fn start(port: u16, upstream: Upstream, ctx: ProxyContext) -> anyhow::Result<ProxyHandle> {
    let listener = TcpListener::bind(("127.0.0.1", port)).await?;
    let addr = listener.local_addr()?;
    let client = build_client(&upstream)?;
    let shared = Arc::new(Shared { upstream: upstream.clone(), ctx, client });

    let task = tokio::spawn(async move {
        loop {
            let (stream, _) = match listener.accept().await {
                Ok(s) => s,
                Err(e) => {
                    // EMFILE and friends: back off instead of spinning.
                    tracing::warn!("proxy accept failed: {e}");
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }
            };
            let _ = stream.set_nodelay(true);
            let shared = shared.clone();
            tokio::spawn(async move {
                let svc = service_fn(move |req| {
                    let shared = shared.clone();
                    async move { Ok::<_, Infallible>(shared.handle(req).await) }
                });
                if let Err(e) = hyper::server::conn::http1::Builder::new()
                    .serve_connection(TokioIo::new(stream), svc)
                    .with_upgrades()
                    .await
                {
                    tracing::debug!("proxy connection ended: {e}");
                }
            });
        }
    });
    Ok(ProxyHandle { addr, upstream, task })
}

fn build_client(up: &Upstream) -> anyhow::Result<UpstreamClient> {
    let mut http = HttpConnector::new();
    http.enforce_http(false);
    http.set_connect_timeout(Some(up.connect_timeout));
    http.set_nodelay(true);
    let builder = hyper_rustls::HttpsConnectorBuilder::new().with_tls_config(tls::client_config(up)?);
    let builder = if up.scheme == "https" { builder.https_only() } else { builder.https_or_http() };
    let builder = match &up.origin_server_name {
        Some(name) => builder.with_server_name_resolver(hyper_rustls::FixedServerNameResolver::new(
            rustls::pki_types::ServerName::try_from(name.clone())?,
        )),
        None => builder,
    };
    // Always HTTP/1.1 upstream, like cloudflared without http2Origin (rules with it aren't inspected).
    let connector = TimeoutConnector {
        inner: builder.enable_http1().wrap_connector(http),
        timeout: up.connect_timeout + up.tls_timeout,
    };
    Ok(Client::builder(TokioExecutor::new())
        .pool_idle_timeout(Duration::from_secs(90))
        .build(connector))
}

struct Shared {
    upstream: Upstream,
    ctx: ProxyContext,
    client: UpstreamClient,
}

/// Hop-by-hop headers (RFC 9110 §7.6.1) are connection-specific and never forwarded.
const HOP_BY_HOP: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-connection",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
];

fn strip_hop_by_hop(headers: &mut HeaderMap) {
    // Headers named in `Connection` are hop-by-hop too.
    let named: Vec<HeaderName> = headers
        .get_all(header::CONNECTION)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .filter_map(|n| HeaderName::from_bytes(n.trim().as_bytes()).ok())
        .collect();
    for n in named {
        headers.remove(n);
    }
    for n in HOP_BY_HOP {
        headers.remove(*n);
    }
}

fn is_upgrade<B>(req: &Request<B>) -> bool {
    req.headers().contains_key(header::UPGRADE)
        && req
            .headers()
            .get_all(header::CONNECTION)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .any(|v| v.split(',').any(|t| t.trim().eq_ignore_ascii_case("upgrade")))
}

pub fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

fn text_response(status: StatusCode, msg: String) -> Response<ProxyBody> {
    let mut r = Response::new(Full::new(Bytes::from(msg)).map_err(|n: Infallible| match n {}).boxed());
    *r.status_mut() = status;
    r.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("text/plain; charset=utf-8"));
    r
}

impl Shared {
    async fn handle(self: Arc<Self>, mut req: Request<Incoming>) -> Response<ProxyBody> {
        let (cap, redact) = {
            let o = self.ctx.options.read().unwrap();
            (o.max_body_bytes, o.redactor())
        };
        let path = req.uri().path_and_query().map(|p| p.as_str().to_string()).unwrap_or_else(|| "/".into());
        let host = req
            .headers()
            .get(header::HOST)
            .and_then(|h| h.to_str().ok())
            .map(String::from)
            .or_else(|| self.upstream.hostname.clone())
            .unwrap_or_default();
        let upgrade = is_upgrade(&req);

        let record = CapturedExchange {
            id: uuid::Uuid::new_v4().to_string(),
            tunnel_id: self.ctx.tunnel_id.clone(),
            rule_index: self.upstream.rule_index,
            started_at_ms: now_ms(),
            duration_ms: None,
            kind: if upgrade { ExchangeKind::Websocket } else { ExchangeKind::Http },
            method: req.method().to_string(),
            host,
            path: capture::redact_query(&path, &redact.query),
            status: None,
            req_headers: capture::redact_headers(req.headers(), &redact.headers),
            resp_headers: Vec::new(),
            req_body: BodyCapture::default(),
            resp_body: BodyCapture::default(),
            error: None,
        };
        let pending = Pending::new(record, cap, self.ctx.sink.clone());

        let uri: Uri = match format!("{}://{}{}", self.upstream.scheme, self.upstream.authority, path).parse() {
            Ok(u) => u,
            Err(e) => {
                pending.lock().unwrap().commit(Some(format!("bad upstream URI: {e}")));
                return text_response(StatusCode::BAD_GATEWAY, format!("Burrow inspector: bad upstream URI: {e}\n"));
            }
        };

        let client_upgrade = if upgrade { Some(hyper::upgrade::on(&mut req)) } else { None };
        let (mut parts, body) = req.into_parts();
        let upgrade_value = parts.headers.get(header::UPGRADE).cloned();
        strip_hop_by_hop(&mut parts.headers);
        if let Some(u) = upgrade_value.filter(|_| upgrade) {
            parts.headers.insert(header::CONNECTION, HeaderValue::from_static("upgrade"));
            parts.headers.insert(header::UPGRADE, u);
        }
        parts.uri = uri;
        parts.version = hyper::Version::HTTP_11;

        let out_body: ProxyBody = if upgrade {
            Empty::<Bytes>::new().map_err(|n: Infallible| match n {}).boxed()
        } else {
            TeeBody::new(body, pending.clone(), Side::Request).boxed()
        };
        let upstream_req = Request::from_parts(parts, out_body);

        let mut resp = match self.client.request(upstream_req).await {
            Ok(r) => r,
            Err(e) => {
                let msg = describe_client_error(&e, &self.upstream);
                let mut p = pending.lock().unwrap();
                p.record.status = Some(502);
                p.commit(Some(msg.clone()));
                return text_response(StatusCode::BAD_GATEWAY, format!("Burrow inspector: {msg}\n"));
            }
        };

        let status = resp.status();
        let is_sse = resp
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.trim_start().to_ascii_lowercase().starts_with("text/event-stream"))
            .unwrap_or(false);
        {
            let mut p = pending.lock().unwrap();
            p.record.status = Some(status.as_u16());
            p.record.resp_headers = capture::redact_headers(resp.headers(), &redact.headers);
            if is_sse {
                p.record.kind = ExchangeKind::Sse;
                p.capture_response_body = false;
                p.emit_open();
            }
        }

        if status == StatusCode::SWITCHING_PROTOCOLS {
            if let Some(client_upgrade) = client_upgrade {
                let upstream_upgrade = hyper::upgrade::on(&mut resp);
                pending.lock().unwrap().emit_open();
                let pending = pending.clone();
                tokio::spawn(async move {
                    let result = async {
                        let (client_io, upstream_io) = tokio::try_join!(client_upgrade, upstream_upgrade)?;
                        let mut client_io = TokioIo::new(client_io);
                        let mut upstream_io = TokioIo::new(upstream_io);
                        let (up, down) = tokio::io::copy_bidirectional(&mut client_io, &mut upstream_io).await?;
                        anyhow::Ok((up, down))
                    }
                    .await;
                    let mut p = pending.lock().unwrap();
                    match result {
                        Ok((up, down)) => {
                            p.record.req_body.count(up);
                            p.record.resp_body.count(down);
                            p.commit(None);
                        }
                        Err(e) => p.commit(Some(format!("websocket: {e}"))),
                    }
                });
                let (mut parts, _) = resp.into_parts();
                parts.headers.remove(header::TRANSFER_ENCODING);
                return Response::from_parts(parts, Empty::<Bytes>::new().map_err(|n: Infallible| match n {}).boxed());
            }
        }

        let (mut parts, body) = resp.into_parts();
        strip_hop_by_hop(&mut parts.headers);
        Response::from_parts(parts, TeeBody::new(body, pending, Side::Response).boxed())
    }
}

fn describe_client_error(e: &hyper_util::client::legacy::Error, up: &Upstream) -> String {
    use std::error::Error;
    let mut chain = vec![e.to_string()];
    let mut src = e.source();
    while let Some(s) = src {
        chain.push(s.to_string());
        src = s.source();
    }
    let joined = chain.join(": ");
    if e.is_connect() {
        if joined.contains("refused") {
            return format!("nothing is listening on {}://{} (connection refused)", up.scheme, up.authority);
        }
        return format!("could not connect to {}://{}: {joined}", up.scheme, up.authority);
    }
    format!("upstream {}://{} failed: {joined}", up.scheme, up.authority)
}
