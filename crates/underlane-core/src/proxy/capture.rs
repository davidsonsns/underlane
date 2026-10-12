//! What the proxy records about each exchange, and the streaming tee that records bodies
//! without delaying them.

use std::collections::HashSet;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::task::{Context, Poll};
use std::time::Instant;

use bytes::Bytes;
use hyper::body::{Body, Frame, SizeHint};
use hyper::HeaderMap;
use serde::{Deserialize, Serialize};

pub const REDACTED: &str = "[redacted]";

/// Headers that are always redacted, whatever the user configures.
pub const ALWAYS_REDACT: &[&str] = &[
    "authorization",
    "proxy-authorization",
    "cookie",
    "set-cookie",
    "cf-access-jwt-assertion",
    "cf-access-client-secret",
    "x-api-key",
    "x-auth-token",
    "sec-websocket-protocol",
];

/// Headers whose values are URLs; their query strings get the query redaction.
const URL_HEADERS: &[&str] = &["location", "referer", "content-location", "refresh"];

/// Query parameters whose values are always redacted in captured paths and log messages.
pub const ALWAYS_REDACT_QUERY: &[&str] =
    &["token", "access_token", "api_key", "apikey", "key", "code", "sig", "signature", "password", "secret"];

/// Upper bound on body bytes held in memory across all open exchanges.
pub const INFLIGHT_BUDGET: usize = 64 * 1024 * 1024;
static INFLIGHT: AtomicUsize = AtomicUsize::new(0);

/// Bytes currently held by open exchanges (for tests and diagnostics).
pub fn inflight_bytes() -> usize {
    INFLIGHT.load(Ordering::Relaxed)
}

/// Reserve up to `want` bytes from the global budget; returns how many were granted.
fn reserve(want: usize) -> usize {
    let mut cur = INFLIGHT.load(Ordering::Relaxed);
    loop {
        let grant = want.min(INFLIGHT_BUDGET.saturating_sub(cur));
        if grant == 0 {
            return 0;
        }
        match INFLIGHT.compare_exchange_weak(cur, cur + grant, Ordering::AcqRel, Ordering::Relaxed) {
            Ok(_) => return grant,
            Err(actual) => cur = actual,
        }
    }
}

fn release(n: usize) {
    if n > 0 {
        INFLIGHT.fetch_sub(n, Ordering::AcqRel);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CaptureOptions {
    /// Maximum stored bytes per body. 0 records metadata only.
    pub max_body_bytes: usize,
    /// Extra header names to redact (case-insensitive), on top of [`ALWAYS_REDACT`].
    pub redact_headers: Vec<String>,
    /// Extra query parameter names to redact, on top of [`ALWAYS_REDACT_QUERY`].
    #[serde(default)]
    pub redact_query: Vec<String>,
}

impl Default for CaptureOptions {
    fn default() -> Self {
        CaptureOptions { max_body_bytes: 1024 * 1024, redact_headers: Vec::new(), redact_query: Vec::new() }
    }
}

impl CaptureOptions {
    pub fn redaction_set(&self) -> HashSet<String> {
        ALWAYS_REDACT
            .iter()
            .map(|s| s.to_string())
            .chain(self.redact_headers.iter().map(|s| s.trim().to_ascii_lowercase()))
            .filter(|s| !s.is_empty())
            .collect()
    }

    pub fn query_redaction_set(&self) -> HashSet<String> {
        ALWAYS_REDACT_QUERY
            .iter()
            .map(|s| s.to_string())
            .chain(self.redact_query.iter().map(|s| s.trim().to_ascii_lowercase()))
            .filter(|s| !s.is_empty())
            .collect()
    }

    pub fn redactor(&self) -> Redactor {
        Redactor { headers: self.redaction_set(), query: self.query_redaction_set() }
    }
}

/// Precomputed redaction lists.
#[derive(Debug, Clone, Default)]
pub struct Redactor {
    pub headers: HashSet<String>,
    pub query: HashSet<String>,
}

/// Replace the values of sensitive query parameters in a path or URL. Everything else,
/// including parameter order and encoding, is left exactly as it was.
pub fn redact_query(url: &str, names: &HashSet<String>) -> String {
    let Some((base, rest)) = url.split_once('?') else {
        return url.to_string();
    };
    let (query, fragment) = match rest.split_once('#') {
        Some((q, f)) => (q, Some(f)),
        None => (rest, None),
    };
    let redacted: Vec<String> = query
        .split('&')
        .map(|pair| match pair.split_once('=') {
            Some((k, _)) if names.contains(&percent_decode_lower(k)) => format!("{k}={REDACTED}"),
            _ => pair.to_string(),
        })
        .collect();
    let mut out = format!("{base}?{}", redacted.join("&"));
    if let Some(f) = fragment {
        out.push('#');
        out.push_str(f);
    }
    out
}

fn percent_decode_lower(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                match u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("zz"), 16) {
                    Ok(b) => {
                        out.push(b);
                        i += 3;
                    }
                    Err(_) => {
                        out.push(b'%');
                        i += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).to_ascii_lowercase()
}

/// Redact query parameters in every URL-looking token of free text (cloudflared debug
/// messages contain full request URLs).
pub fn redact_query_in_text(text: &str, names: &HashSet<String>) -> String {
    if !text.contains('?') {
        return text.to_string();
    }
    text.split(' ')
        .map(|tok| if tok.contains('?') && tok.contains('=') { redact_query(tok, names) } else { tok.to_string() })
        .collect::<Vec<_>>()
        .join(" ")
}

pub type SharedOptions = Arc<RwLock<CaptureOptions>>;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ExchangeKind {
    Http,
    Websocket,
    Sse,
}

impl ExchangeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ExchangeKind::Http => "http",
            ExchangeKind::Websocket => "websocket",
            ExchangeKind::Sse => "sse",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct BodyCapture {
    /// Stored bytes (at most `max_body_bytes`).
    pub bytes: Vec<u8>,
    /// Total bytes that went through, stored or not.
    pub size: u64,
    pub truncated: bool,
}

impl BodyCapture {
    /// Append a chunk, keeping at most `cap` bytes in total and at most `allowance` new ones
    /// (the global budget grant). Returns the bytes newly stored.
    pub fn push_limited(&mut self, chunk: &[u8], cap: usize, allowance: usize) -> usize {
        self.size += chunk.len() as u64;
        // Once anything was skipped, stop storing: the capture must stay a faithful prefix.
        if self.truncated {
            return 0;
        }
        let room = cap.saturating_sub(self.bytes.len()).min(allowance);
        if chunk.len() > room {
            self.truncated = true;
        }
        let take = room.min(chunk.len());
        self.bytes.extend_from_slice(&chunk[..take]);
        take
    }

    /// Count bytes without storing them (SSE, WebSocket).
    pub fn count(&mut self, n: u64) {
        self.size += n;
    }
}

/// One request/response pair as stored.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CapturedExchange {
    pub id: String,
    pub tunnel_id: String,
    pub rule_index: usize,
    pub started_at_ms: i64,
    /// None while a long-lived exchange (WebSocket, SSE) is still open.
    pub duration_ms: Option<i64>,
    pub kind: ExchangeKind,
    pub method: String,
    pub host: String,
    /// Path and query.
    pub path: String,
    pub status: Option<u16>,
    pub req_headers: Vec<(String, String)>,
    pub resp_headers: Vec<(String, String)>,
    pub req_body: BodyCapture,
    pub resp_body: BodyCapture,
    pub error: Option<String>,
}

/// Header list with sensitive values replaced. Names keep their wire spelling. URL-valued
/// headers (Location, Referer, ...) get their query parameters redacted.
pub fn redact_headers(headers: &HeaderMap, r: &Redactor) -> Vec<(String, String)> {
    headers
        .iter()
        .map(|(name, value)| {
            let n = name.as_str().to_string();
            let lower = n.to_ascii_lowercase();
            let v = if r.headers.contains(&lower) {
                REDACTED.to_string()
            } else {
                let raw = String::from_utf8_lossy(value.as_bytes()).into_owned();
                if URL_HEADERS.contains(&lower.as_str()) {
                    redact_query_in_text(&raw, &r.query)
                } else {
                    raw
                }
            };
            (n, v)
        })
        .collect()
}

/// Redact a cloudflared debug-log `headers` object (`{"Name": ["v", ...]}`) in place.
pub fn redact_json_headers(headers: &mut serde_json::Value, redact: &HashSet<String>) {
    if let Some(map) = headers.as_object_mut() {
        for (name, value) in map.iter_mut() {
            if redact.contains(&name.to_ascii_lowercase()) {
                *value = serde_json::Value::Array(vec![serde_json::Value::String(REDACTED.into())]);
            }
        }
    }
}

pub type Sink = Arc<dyn Fn(CapturedExchange) + Send + Sync>;

/// State shared by the request tee, the response tee and the handler for one exchange.
pub struct Pending {
    pub record: CapturedExchange,
    pub started: Instant,
    pub cap: usize,
    pub capture_response_body: bool,
    committed: bool,
    /// Bytes reserved from the global in-flight budget.
    reserved: usize,
    sink: Sink,
}

impl Pending {
    pub fn new(record: CapturedExchange, cap: usize, sink: Sink) -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Pending {
            record,
            started: Instant::now(),
            cap,
            capture_response_body: true,
            committed: false,
            reserved: 0,
            sink,
        }))
    }

    /// Store a body chunk within the per-body cap and the global budget.
    pub fn store(&mut self, side: Side, chunk: &[u8]) {
        if self.committed {
            return;
        }
        let cap = self.cap;
        let body = match side {
            Side::Request => &mut self.record.req_body,
            Side::Response => &mut self.record.resp_body,
        };
        let want = cap.saturating_sub(body.bytes.len()).min(chunk.len());
        let grant = if want > 0 { reserve(want) } else { 0 };
        let stored = body.push_limited(chunk, cap, grant);
        release(grant - stored);
        self.reserved += stored;
    }

    /// Emit an intermediate snapshot (used for long-lived exchanges so they show up while open).
    pub fn emit_open(&self) {
        let mut snapshot = self.record.clone();
        snapshot.duration_ms = None;
        (self.sink)(snapshot);
    }

    /// Emit the final record once.
    pub fn commit(&mut self, error: Option<String>) {
        if self.committed {
            return;
        }
        self.committed = true;
        if error.is_some() && self.record.error.is_none() {
            self.record.error = error;
        }
        self.record.duration_ms = Some(self.started.elapsed().as_millis() as i64);
        let record = std::mem::replace(&mut self.record, empty_record());
        release(std::mem::take(&mut self.reserved));
        (self.sink)(record);
    }
}

impl Drop for Pending {
    fn drop(&mut self) {
        release(std::mem::take(&mut self.reserved));
    }
}

fn empty_record() -> CapturedExchange {
    CapturedExchange {
        id: String::new(),
        tunnel_id: String::new(),
        rule_index: 0,
        started_at_ms: 0,
        duration_ms: None,
        kind: ExchangeKind::Http,
        method: String::new(),
        host: String::new(),
        path: String::new(),
        status: None,
        req_headers: Vec::new(),
        resp_headers: Vec::new(),
        req_body: BodyCapture::default(),
        resp_body: BodyCapture::default(),
        error: None,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Request,
    Response,
}

/// A body wrapper that forwards every frame untouched and copies data into the capture.
/// The response-side tee commits the exchange when the stream ends or is dropped.
pub struct TeeBody<B: Body> {
    inner: B,
    pending: Arc<Mutex<Pending>>,
    side: Side,
    done: bool,
}

impl<B: Body> TeeBody<B> {
    pub fn new(inner: B, pending: Arc<Mutex<Pending>>, side: Side) -> Self {
        TeeBody { inner, pending, side, done: false }
    }
}

impl<B> Body for TeeBody<B>
where
    B: Body<Data = Bytes> + Unpin,
    B::Error: std::fmt::Display,
{
    type Data = Bytes;
    type Error = B::Error;

    fn poll_frame(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Frame<Bytes>, B::Error>>> {
        let this = &mut *self;
        let polled = Pin::new(&mut this.inner).poll_frame(cx);
        match &polled {
            Poll::Ready(Some(Ok(frame))) => {
                if let Some(data) = frame.data_ref() {
                    let mut p = this.pending.lock().unwrap();
                    match this.side {
                        Side::Response if !p.capture_response_body => p.record.resp_body.count(data.len() as u64),
                        side => p.store(side, data),
                    }
                }
            }
            Poll::Ready(Some(Err(e))) => {
                this.done = true;
                let msg = format!("{} body error: {e}", if this.side == Side::Request { "request" } else { "response" });
                let mut p = this.pending.lock().unwrap();
                if this.side == Side::Response {
                    p.commit(Some(msg));
                } else if p.record.error.is_none() {
                    p.record.error = Some(msg);
                }
            }
            Poll::Ready(None) => {
                this.done = true;
                if this.side == Side::Response {
                    this.pending.lock().unwrap().commit(None);
                }
            }
            Poll::Pending => {}
        }
        polled
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        self.inner.size_hint()
    }
}

impl<B: Body> Drop for TeeBody<B> {
    fn drop(&mut self) {
        if self.side == Side::Response && !self.done {
            // Either an empty body hyper never polled, or the client went away mid-stream.
            let complete = self.inner.is_end_stream();
            let mut p = self.pending.lock().unwrap();
            p.commit(if complete { None } else { Some("client closed the connection before the response finished".into()) });
        }
    }
}
