mod common;

use std::convert::Infallible;
use std::time::{Duration, Instant};

use bytes::Bytes;
use common::{raw_request, Harness};
use http_body_util::{BodyExt, Full, StreamBody};
use hyper::body::Frame;
use hyper::Request;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;

use burrow_core::decode::decode_body;
use burrow_core::proxy::capture::{self, CaptureOptions, ExchangeKind, REDACTED};

type AnyBody = http_body_util::combinators::BoxBody<Bytes, Infallible>;

fn client() -> Client<hyper_util::client::legacy::connect::HttpConnector, AnyBody> {
    Client::builder(TokioExecutor::new()).build_http()
}

fn header<'a>(h: &'a [(String, String)], name: &str) -> Option<&'a str> {
    h.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
}

#[tokio::test]
async fn captures_json_request_and_response_bodies() {
    let h = Harness::new(CaptureOptions::default()).await;
    let req = Request::post(h.url("/json?page=2"))
        .header("content-type", "application/json")
        .header("host", "app.example.com")
        .body(Full::new(Bytes::from_static(br#"{"hello":"world"}"#)).boxed())
        .unwrap();
    let resp = client().request(req).await.unwrap();
    assert_eq!(resp.status(), 200);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], br#"{"received":{"hello":"world"},"ok":true}"#);

    let x = h.wait_for("/json").await;
    assert_eq!(x.method, "POST");
    assert_eq!(x.path, "/json?page=2");
    assert_eq!(x.host, "app.example.com");
    assert_eq!(x.status, Some(200));
    assert_eq!(x.kind, ExchangeKind::Http);
    assert_eq!(x.req_body.bytes, br#"{"hello":"world"}"#);
    assert_eq!(x.resp_body.bytes, &body[..]);
    assert_eq!(x.resp_body.size, body.len() as u64);
    assert!(!x.req_body.truncated && !x.resp_body.truncated);
    assert_eq!(header(&x.resp_headers, "content-type"), Some("application/json"));
    assert!(x.duration_ms.is_some());
    assert_eq!(x.tunnel_id, "t1");
}

#[tokio::test]
async fn preserves_host_header_for_the_origin() {
    let h = Harness::new(CaptureOptions::default()).await;
    let req = Request::get(h.url("/headers")).header("host", "public.example.com").body(Full::new(Bytes::new()).boxed()).unwrap();
    let body = client().request(req).await.unwrap().into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], b"host=public.example.com");
}

#[tokio::test]
async fn redacts_sensitive_headers_and_query_params() {
    let h = Harness::new(CaptureOptions {
        redact_headers: vec!["X-Internal-Key".into()],
        redact_query: vec!["session_id".into()],
        ..Default::default()
    })
    .await;
    let req = Request::post(h.url("/echo?token=s3cret&page=1&Session_Id=abc&api%5Fkey=zzz"))
        .header("authorization", "Bearer s3cret")
        .header("cookie", "sid=123")
        .header("x-internal-key", "k")
        .header("x-api-key", "k2")
        .header("x-visible", "yes")
        .body(Full::new(Bytes::from_static(b"payload")).boxed())
        .unwrap();
    let resp = client().request(req).await.unwrap();
    // The origin still receives and returns everything; only the record is redacted.
    assert!(resp.headers().get("set-cookie").is_some());
    resp.into_body().collect().await.unwrap();

    let x = h.wait_for("/echo").await;
    assert_eq!(header(&x.req_headers, "authorization"), Some(REDACTED));
    assert_eq!(header(&x.req_headers, "cookie"), Some(REDACTED));
    assert_eq!(header(&x.req_headers, "x-internal-key"), Some(REDACTED));
    assert_eq!(header(&x.req_headers, "x-api-key"), Some(REDACTED));
    assert_eq!(header(&x.req_headers, "x-visible"), Some("yes"));
    assert_eq!(header(&x.resp_headers, "set-cookie"), Some(REDACTED));
    assert_eq!(header(&x.resp_headers, "x-origin"), Some("test"));
    assert_eq!(x.path, format!("/echo?token={REDACTED}&page=1&Session_Id={REDACTED}&api%5Fkey={REDACTED}"));
    let serialized = serde_json::to_string(&x).unwrap();
    assert!(!serialized.contains("s3cret") && !serialized.contains("sid=123") && !serialized.contains("abc123"));
}

#[tokio::test]
async fn truncates_large_streaming_bodies_but_forwards_everything() {
    let h = Harness::new(CaptureOptions { max_body_bytes: 10_000, ..Default::default() }).await;
    // Chunked request body of 50 KB.
    let chunks: Vec<Result<Frame<Bytes>, Infallible>> = (0..50).map(|_| Ok(Frame::data(Bytes::from(vec![b'a'; 1000])))).collect();
    let req = Request::post(h.url("/echo")).body(StreamBody::new(futures_util::stream::iter(chunks)).boxed()).unwrap();
    let body = client().request(req).await.unwrap().into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body.len(), 50_000, "origin received and echoed the full body");

    let x = h.wait_for("/echo").await;
    assert_eq!(x.req_body.size, 50_000);
    assert_eq!(x.req_body.bytes.len(), 10_000);
    assert!(x.req_body.truncated);
    assert_eq!(x.resp_body.size, 50_000);
    assert_eq!(x.resp_body.bytes.len(), 10_000);
    assert!(x.resp_body.truncated);

    // A chunked streaming response larger than the cap.
    let body = client().get(h.url("/stream?n=200000").parse().unwrap()).await.unwrap().into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body.len(), 200_000);
    let x = h.wait_for("/stream").await;
    assert_eq!(x.resp_body.size, 200_000);
    assert_eq!(x.resp_body.bytes.len(), 10_000);
    assert!(x.resp_body.truncated);
    assert_eq!(capture::inflight_bytes() < capture::INFLIGHT_BUDGET, true);
}

#[tokio::test]
async fn zero_cap_records_metadata_only() {
    let h = Harness::new(CaptureOptions { max_body_bytes: 0, ..Default::default() }).await;
    let req = Request::post(h.url("/echo")).body(Full::new(Bytes::from_static(b"hello")).boxed()).unwrap();
    client().request(req).await.unwrap().into_body().collect().await.unwrap();
    let x = h.wait_for("/echo").await;
    assert!(x.req_body.bytes.is_empty() && x.resp_body.bytes.is_empty());
    assert_eq!((x.req_body.size, x.resp_body.size), (5, 5));
    assert!(x.req_body.truncated);
}

#[tokio::test]
async fn stores_compressed_bodies_raw_and_decodes_for_display() {
    let h = Harness::new(CaptureOptions::default()).await;
    let resp = client().get(h.url("/gzip").parse().unwrap()).await.unwrap();
    assert_eq!(resp.headers().get("content-encoding").unwrap(), "gzip");
    resp.into_body().collect().await.unwrap();
    let x = h.wait_for("/gzip").await;
    assert_eq!(&x.resp_body.bytes[..2], &[0x1f, 0x8b], "raw gzip stored");
    let d = decode_body(&x.resp_body.bytes, header(&x.resp_headers, "content-encoding"));
    assert_eq!(d.bytes, br#"{"compressed":true,"items":[1,2,3]}"#);
    assert_eq!(d.decoded_from.as_deref(), Some("gzip"));
    assert!(!d.partial);
    // A truncated gzip stream still yields its decodable prefix.
    let big: Vec<u8> = (0..200_000u32).flat_map(|i| i.to_le_bytes()).collect();
    let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    std::io::Write::write_all(&mut enc, &big).unwrap();
    let gz = enc.finish().unwrap();
    let cut = &gz[..gz.len() / 2];
    let d = decode_body(cut, Some("gzip"));
    assert!(d.partial);
    assert!(!d.bytes.is_empty() && big.starts_with(&d.bytes));
}

#[tokio::test]
async fn sse_is_passed_through_with_metadata_only() {
    let h = Harness::new(CaptureOptions::default()).await;
    let body = client().get(h.url("/sse").parse().unwrap()).await.unwrap().into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], b"data: event 0\n\ndata: event 1\n\ndata: event 2\n\n");
    let x = h.wait_for("/sse").await;
    assert_eq!(x.kind, ExchangeKind::Sse);
    assert!(x.resp_body.bytes.is_empty(), "SSE bodies are not stored");
    assert_eq!(x.resp_body.size, body.len() as u64);
    // An "open" snapshot was emitted before the final one, so it shows up while streaming.
    let snapshots = h.captured.lock().unwrap().iter().filter(|c| c.path == "/sse").count();
    assert_eq!(snapshots, 2);
}

#[tokio::test]
async fn websocket_upgrade_is_tunnelled_and_recorded() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let h = Harness::new(CaptureOptions::default()).await;
    let mut s = tokio::net::TcpStream::connect(("127.0.0.1", h.proxy.port())).await.unwrap();
    s.write_all(
        b"GET /ws HTTP/1.1\r\nHost: app.example.com\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nAuthorization: Bearer x\r\n\r\n",
    )
    .await
    .unwrap();
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        s.read_exact(&mut byte).await.unwrap();
        head.push(byte[0]);
    }
    let head = String::from_utf8(head).unwrap();
    assert!(head.starts_with("HTTP/1.1 101"), "{head}");
    // Bytes flow both ways through the tunnel (the origin echoes).
    s.write_all(b"ping-frame").await.unwrap();
    let mut echoed = [0u8; 10];
    s.read_exact(&mut echoed).await.unwrap();
    assert_eq!(&echoed, b"ping-frame");
    drop(s);

    let x = h.wait_for("/ws").await;
    assert_eq!(x.kind, ExchangeKind::Websocket);
    assert_eq!(x.status, Some(101));
    assert_eq!(header(&x.req_headers, "authorization"), Some(REDACTED));
    assert_eq!(x.req_body.size, 10);
    assert_eq!(x.resp_body.size, 10);
    assert!(x.req_body.bytes.is_empty(), "frames are not stored");
}

#[tokio::test]
async fn upstream_down_returns_502_and_is_recorded() {
    let h = Harness::new(CaptureOptions::default()).await;
    // Point a second proxy at a port nothing listens on.
    let dead = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap();
    let up = burrow_core::derived::Upstream::from_service(0, None, &format!("http://{dead}")).unwrap();
    let ctx = burrow_core::proxy::ProxyContext {
        tunnel_id: "t2".into(),
        options: std::sync::Arc::new(std::sync::RwLock::new(CaptureOptions::default())),
        sink: {
            let c = h.captured.clone();
            std::sync::Arc::new(move |x| c.lock().unwrap().push(x))
        },
    };
    let p = burrow_core::proxy::start(0, up, ctx).await.unwrap();
    let (head, body) = raw_request(p.port(), "GET /down HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n").await;
    assert!(head.starts_with("HTTP/1.1 502"), "{head}");
    assert!(String::from_utf8_lossy(&body).contains("connection refused"));
    let x = h.wait_for("/down").await;
    assert_eq!(x.status, Some(502));
    assert!(x.error.unwrap().contains("connection refused"));
}

/// Measures the latency the proxy adds on loopback. Run with
/// `cargo test --release -p burrow-core --test proxy_capture -- --ignored --nocapture`.
#[tokio::test]
#[ignore]
async fn bench_added_latency() {
    let h = Harness::new(CaptureOptions::default()).await;
    let c = client();
    let direct = format!("http://{}/json", h.origin);
    let proxied = h.url("/json");
    async fn sample(c: &Client<hyper_util::client::legacy::connect::HttpConnector, AnyBody>, url: &str) -> Vec<f64> {
        let mut v = Vec::new();
        for i in 0..1200 {
            let t = Instant::now();
            let req = Request::post(url).body(Full::new(Bytes::from_static(br#"{"a":1}"#)).boxed()).unwrap();
            c.request(req).await.unwrap().into_body().collect().await.unwrap();
            if i >= 200 {
                v.push(t.elapsed().as_secs_f64() * 1000.0);
            }
        }
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v
    }
    let d = sample(&c, &direct).await;
    let p = sample(&c, &proxied).await;
    let pct = |v: &[f64], q: f64| v[((v.len() as f64) * q) as usize];
    println!(
        "direct p50={:.3}ms p99={:.3}ms | proxied p50={:.3}ms p99={:.3}ms | added p50={:.3}ms",
        pct(&d, 0.5),
        pct(&d, 0.99),
        pct(&p, 0.5),
        pct(&p, 0.99),
        pct(&p, 0.5) - pct(&d, 0.5)
    );
    tokio::time::sleep(Duration::from_millis(50)).await;
}

#[test]
fn capture_stays_a_faithful_prefix_after_a_gap() {
    let mut b = capture::BodyCapture::default();
    assert_eq!(b.push_limited(b"AAA", 100, 3), 3);
    // Budget exhausted for this chunk: nothing stored, marked truncated.
    assert_eq!(b.push_limited(b"BBB", 100, 0), 0);
    // Budget available again, but storing CCC would produce AAACCC; it must not.
    assert_eq!(b.push_limited(b"CCC", 100, 100), 0);
    assert_eq!(b.bytes, b"AAA");
    assert_eq!(b.size, 9);
    assert!(b.truncated);
}

#[tokio::test]
async fn redacts_query_strings_inside_url_headers() {
    let h = Harness::new(CaptureOptions::default()).await;
    let req = Request::get(h.url("/echo"))
        .header("referer", "https://app.example.com/callback?code=abc123&state=ok")
        .body(Full::new(Bytes::new()).boxed())
        .unwrap();
    client().request(req).await.unwrap().into_body().collect().await.unwrap();
    let x = h.wait_for("/echo").await;
    assert_eq!(header(&x.req_headers, "referer"), Some("https://app.example.com/callback?code=[redacted]&state=ok"));
}
