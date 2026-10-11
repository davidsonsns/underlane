//! Test origin server and helpers shared by the integration tests.

use std::convert::Infallible;
use std::io::Write;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use bytes::Bytes;
use http_body_util::{BodyExt, Full, StreamBody};
use hyper::body::{Frame, Incoming};
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;

use burrow_core::derived::Upstream;
use burrow_core::proxy::capture::{CaptureOptions, CapturedExchange};
use burrow_core::proxy::{self, ProxyContext, ProxyHandle};

type Body = http_body_util::combinators::BoxBody<Bytes, Infallible>;

fn full(b: impl Into<Bytes>) -> Body {
    Full::new(b.into()).boxed()
}

/// Origin with a few behaviours selected by path.
async fn origin(mut req: Request<Incoming>) -> Result<Response<Body>, Infallible> {
    let path = req.uri().path().to_string();
    let query = req.uri().query().unwrap_or("").to_string();
    match path.as_str() {
        "/echo" => {
            let ct = req.headers().get("content-type").cloned();
            let body = req.into_body().collect().await.unwrap().to_bytes();
            let mut r = Response::new(full(body));
            if let Some(ct) = ct {
                r.headers_mut().insert("content-type", ct);
            }
            r.headers_mut().insert("set-cookie", "session=abc123; HttpOnly".parse().unwrap());
            r.headers_mut().insert("x-origin", "test".parse().unwrap());
            Ok(r)
        }
        "/json" => {
            let body = req.into_body().collect().await.unwrap().to_bytes();
            let json = format!("{{\"received\":{},\"ok\":true}}", String::from_utf8_lossy(&body));
            let mut r = Response::new(full(json));
            r.headers_mut().insert("content-type", "application/json".parse().unwrap());
            Ok(r)
        }
        "/stream" => {
            // n bytes in 8 KiB chunks with chunked transfer encoding.
            let n: usize = query.strip_prefix("n=").and_then(|v| v.parse().ok()).unwrap_or(1024);
            let chunks: Vec<Result<Frame<Bytes>, Infallible>> = (0..n)
                .step_by(8192)
                .map(|start| Ok(Frame::data(Bytes::from(vec![b'x'; (n - start).min(8192)]))))
                .collect();
            Ok(Response::new(StreamBody::new(futures_iter(chunks)).boxed()))
        }
        "/gzip" => {
            let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            enc.write_all(br#"{"compressed":true,"items":[1,2,3]}"#).unwrap();
            let mut r = Response::new(full(enc.finish().unwrap()));
            r.headers_mut().insert("content-encoding", "gzip".parse().unwrap());
            r.headers_mut().insert("content-type", "application/json".parse().unwrap());
            Ok(r)
        }
        "/sse" => {
            let frames: Vec<Result<Frame<Bytes>, Infallible>> =
                (0..3).map(|i| Ok(Frame::data(Bytes::from(format!("data: event {i}\n\n"))))).collect();
            let mut r = Response::new(StreamBody::new(futures_iter(frames)).boxed());
            r.headers_mut().insert("content-type", "text/event-stream".parse().unwrap());
            Ok(r)
        }
        "/ws" => {
            let on_upgrade = hyper::upgrade::on(&mut req);
            tokio::spawn(async move {
                if let Ok(up) = on_upgrade.await {
                    let mut io = TokioIo::new(up);
                    let (mut r, mut w) = tokio::io::split(&mut io);
                    let _ = tokio::io::copy(&mut r, &mut w).await;
                }
            });
            let mut r = Response::new(full(Bytes::new()));
            *r.status_mut() = hyper::StatusCode::SWITCHING_PROTOCOLS;
            r.headers_mut().insert("connection", "upgrade".parse().unwrap());
            r.headers_mut().insert("upgrade", "websocket".parse().unwrap());
            Ok(r)
        }
        "/headers" => {
            let host = req.headers().get("host").map(|h| h.to_str().unwrap().to_string()).unwrap_or_default();
            Ok(Response::new(full(format!("host={host}"))))
        }
        _ => {
            let mut r = Response::new(full("not found"));
            *r.status_mut() = hyper::StatusCode::NOT_FOUND;
            Ok(r)
        }
    }
}

fn futures_iter(items: Vec<Result<Frame<Bytes>, Infallible>>) -> impl futures_util::Stream<Item = Result<Frame<Bytes>, Infallible>> + Send + Sync + 'static {
    futures_util::stream::iter(items)
}

pub async fn start_origin() -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            tokio::spawn(async move {
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(TokioIo::new(stream), hyper::service::service_fn(origin))
                    .with_upgrades()
                    .await;
            });
        }
    });
    addr
}

pub struct Harness {
    pub proxy: ProxyHandle,
    pub captured: Arc<Mutex<Vec<CapturedExchange>>>,
    pub origin: SocketAddr,
}

impl Harness {
    pub async fn new(opts: CaptureOptions) -> Self {
        let origin = start_origin().await;
        let captured = Arc::new(Mutex::new(Vec::new()));
        let sink_store = captured.clone();
        let ctx = ProxyContext {
            tunnel_id: "t1".into(),
            options: Arc::new(RwLock::new(opts)),
            sink: Arc::new(move |x| sink_store.lock().unwrap().push(x)),
        };
        let up = Upstream::from_service(0, Some("app.example.com".into()), &format!("http://{origin}")).unwrap();
        let proxy = proxy::start(0, up, ctx).await.unwrap();
        Harness { proxy, captured, origin }
    }

    pub fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{}", self.proxy.port(), path)
    }

    /// Wait until a completed record (duration set) for `path_prefix` exists.
    pub async fn wait_for(&self, path_prefix: &str) -> CapturedExchange {
        for _ in 0..200 {
            if let Some(x) = self
                .captured
                .lock()
                .unwrap()
                .iter()
                .rev()
                .find(|x| x.path.starts_with(path_prefix) && x.duration_ms.is_some())
            {
                return x.clone();
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("no capture for {path_prefix}: {:?}", self.captured.lock().unwrap());
    }
}

/// Raw HTTP/1.1 request over TCP; returns (status line + headers, body bytes).
pub async fn raw_request(port: u16, request: &str) -> (String, Vec<u8>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut s = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    s.write_all(request.as_bytes()).await.unwrap();
    let mut buf = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(5), s.read_to_end(&mut buf)).await;
    let split = buf.windows(4).position(|w| w == b"\r\n\r\n").map(|p| p + 4).unwrap_or(buf.len());
    (String::from_utf8_lossy(&buf[..split]).to_string(), buf[split..].to_vec())
}
