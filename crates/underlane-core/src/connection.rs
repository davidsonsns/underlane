//! Bounded, unauthenticated synthetic checks. TCP reachability and public HTTP responses
//! are separate observations; neither a cache hit nor an authentication gate proves the origin.

use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use http_body_util::{BodyExt, Empty};
use hyper::{HeaderMap, Request, Uri};
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use serde::{Deserialize, Serialize};

use crate::supervisor::RouteMapping;

const PUBLIC_TIMEOUT: Duration = Duration::from_secs(5);
const BODY_LIMIT: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PublicStatus {
    NotChecked,
    Verified,
    Responded,
    AuthRequired,
    CloudflareError,
    OriginError,
    HttpError,
    TransportError,
    Unsupported,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PublicCheck {
    pub status: PublicStatus,
    pub message: String,
    pub checked_at: i64,
    pub http_status: Option<u16>,
    pub cloudflare_error: Option<u16>,
    pub test_url: Option<String>,
}

impl PublicCheck {
    fn new(status: PublicStatus, message: impl Into<String>, test_url: Option<String>) -> Self {
        Self {
            status,
            message: message.into(),
            checked_at: crate::proxy::now_ms(),
            http_status: None,
            cloudflare_error: None,
            test_url,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ServiceStatus {
    Reachable,
    Unreachable,
    Unsupported,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServiceCheck {
    pub status: ServiceStatus,
    pub message: String,
    pub checked_at: i64,
}

/// Checks only whether the service accepts a TCP connection, not application health.
pub async fn probe_service(service: &str) -> ServiceCheck {
    let authority = crate::derived::split_service(service)
        .map(|(_, authority)| authority)
        .or_else(|| {
            let (scheme, rest) = service.trim().split_once("://")?;
            ["tcp", "ssh", "rdp", "smb"]
                .contains(&scheme.to_ascii_lowercase().as_str())
                .then(|| rest.trim_end_matches('/').to_string())
        });
    let (status, message) = match authority {
        None => (
            ServiceStatus::Unsupported,
            "This service has no TCP address to check.",
        ),
        Some(authority) => {
            let connected = matches!(
                tokio::time::timeout(
                    Duration::from_millis(400),
                    tokio::net::TcpStream::connect(authority)
                )
                .await,
                Ok(Ok(_))
            );
            if connected {
                (
                    ServiceStatus::Reachable,
                    "The service accepts TCP connections. Application health was not checked.",
                )
            } else {
                (
                    ServiceStatus::Unreachable,
                    "The service did not accept a TCP connection within 400 ms.",
                )
            }
        }
    };
    ServiceCheck {
        status,
        message: message.into(),
        checked_at: crate::proxy::now_ms(),
    }
}

fn validate_hostname(hostname: &str) -> Result<(), String> {
    if hostname.is_empty()
        || hostname.len() > 253
        || !hostname.is_ascii()
        || hostname.split('.').any(|label| {
            label.is_empty()
                || label.len() > 63
                || label.starts_with('-')
                || label.ends_with('-')
                || !label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
    {
        return Err(
            "Public checks need a concrete hostname without a wildcard, scheme, port, or path."
                .into(),
        );
    }
    Ok(())
}

fn validate_path(path: &str) -> Result<(), String> {
    // Encoded or unnormalized paths can match a different rule after edge/Go URL normalization.
    if path.len() > 2048
        || !path.starts_with('/')
        || path.contains("//")
        || !path.bytes().all(|b| b.is_ascii_graphic())
        || path.contains(['?', '#', '@', '\\', '%'])
        || path.split('/').any(|part| part == "." || part == "..")
    {
        return Err("Enter a path starting with /, without a query, fragment, credentials, encoding, dot segments, or spaces (maximum 2048 bytes).".into());
    }
    Ok(())
}

/// Validates a literal path against the first matching ingress rule. Never derives a URL
/// from a regex or assumes that the requested rule wins over an earlier rule.
pub fn test_path(
    mappings: &[RouteMapping],
    index: usize,
    requested: Option<&str>,
) -> Result<String, String> {
    let selected = mappings
        .get(index)
        .ok_or("This route no longer exists. Refresh and retry.")?;
    if crate::derived::split_service(&selected.service).is_none() {
        return Err("Public checks support HTTP services only.".into());
    }
    let hostname = selected
        .hostname
        .as_deref()
        .ok_or("This route needs a concrete hostname for a public check.")?;
    validate_hostname(hostname)?;
    let hostname = hostname.to_ascii_lowercase();
    let path = requested.unwrap_or("/");
    validate_path(path)?;
    for (candidate, mapping) in mappings.iter().enumerate().take(index + 1) {
        let host_matches = match mapping.hostname.as_deref() {
            None | Some("") | Some("*") => true,
            Some(host) if !host.is_ascii() => return Err("An earlier route uses an international hostname that cannot be matched safely for this check.".into()),
            Some(host) if host.eq_ignore_ascii_case(&hostname) => true,
            Some(host) => host.strip_prefix("*.").is_some_and(|suffix| {
                !suffix.contains('*') && hostname.ends_with(&format!(".{}", suffix.to_ascii_lowercase()))
            }),
        };
        if !host_matches {
            continue;
        }
        let path_matches = match mapping.path.as_deref() {
            None | Some("") => true,
            Some(pattern) => path_regex(pattern)?.is_match(path.as_bytes()),
        };
        if path_matches {
            return if candidate == index {
                Ok(path.into())
            } else {
                Err("This path matches an earlier ingress rule. Choose a path that reaches this route.".into())
            };
        }
    }
    Err("This path does not match this route's path filter. Enter a matching test path.".into())
}

fn path_regex(pattern: &str) -> Result<regex::bytes::Regex, String> {
    let invalid = || {
        "This route uses a path pattern that cannot be checked safely. Use cloudflared ingress rule to verify it.".to_string()
    };
    // shortcut: accept the ASCII Go/RE2 subset only; add a Go matcher if wider patterns are needed.
    if pattern.len() > 2048
        || !pattern.is_ascii()
        || ["&&", "--", "~~"].iter().any(|part| pattern.contains(part))
    {
        return Err(invalid());
    }
    let mut chars = pattern.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            let escaped = chars.next().ok_or_else(invalid)?;
            if escaped.is_ascii_alphanumeric() && !"AdDsSwWbzBafnrtv".contains(escaped) {
                return Err(invalid());
            }
        } else if ch == '(' && chars.peek() == Some(&'?') {
            chars.next();
            // Restrict extensions to Go flags and noncapturing groups.
            let flags: String = chars
                .clone()
                .take_while(|c| ![':', ')', '>'].contains(c))
                .collect();
            if !flags.chars().all(|c| "imsU-".contains(c)) {
                return Err(invalid());
            }
        } else if ch == '{' {
            let repetition: String = chars
                .clone()
                .take_while(|c| c.is_ascii_digit() || *c == ',')
                .collect();
            if repetition.is_empty()
                || repetition.starts_with(',')
                || repetition.matches(',').count() > 1
                || chars.clone().nth(repetition.len()) != Some('}')
                || repetition
                    .split(',')
                    .filter(|n| !n.is_empty())
                    .any(|n| n.parse::<u32>().map_or(true, |n| n > 1000))
            {
                return Err(invalid());
            }
        }
    }
    regex::bytes::RegexBuilder::new(pattern)
        .unicode(false)
        .size_limit(256 * 1024)
        .build()
        .map_err(|_| invalid())
}

/// Makes one normal, trusted HTTPS GET with SNI and Host set from the concrete hostname.
/// Hyper does not follow redirects or retain cookies. No credentials are supplied.
pub async fn probe_public(hostname: &str, path: &str) -> PublicCheck {
    if let Err(message) = validate_hostname(hostname).and_then(|_| validate_path(path)) {
        return PublicCheck::new(PublicStatus::Unsupported, message, None);
    }
    probe_url(
        format!("https://{}{path}", hostname.to_ascii_lowercase()),
        PUBLIC_TIMEOUT,
        false,
    )
    .await
}

async fn probe_url(url: String, deadline: Duration, allow_http: bool) -> PublicCheck {
    let result = tokio::time::timeout(deadline, async {
        let roots = tokio::task::spawn_blocking(|| {
            let mut roots = rustls::RootCertStore::empty();
            for cert in rustls_native_certs::load_native_certs().certs {
                let _ = roots.add(cert);
            }
            roots
        }).await.map_err(|_| "Trusted HTTPS certificates could not be loaded. Check this device's certificate store.")?;
        if roots.is_empty() {
            return Err("Trusted HTTPS certificates could not be loaded. Check this device's certificate store.");
        }
        let tls = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions().map_err(|_| "HTTPS could not be initialized.")?
            .with_root_certificates(roots).with_no_client_auth();
        let builder = hyper_rustls::HttpsConnectorBuilder::new().with_tls_config(tls);
        let https = if allow_http { builder.https_or_http() } else { builder.https_only() }.enable_http1().build();
        let client = Client::builder(TokioExecutor::new()).build::<_, Empty<Bytes>>(https);
        let request = Request::get(&url)
            .header("user-agent", "Underlane-Synthetic-Check/1.0")
            .header("cache-control", "no-cache, no-store")
            .header("pragma", "no-cache")
            .header("accept-encoding", "identity")
            .body(Empty::<Bytes>::new()).map_err(|_| "The public test URL is invalid.")?;
        let response = client.request(request).await.map_err(|error| transport_message(&error))?;
        let (parts, mut body) = response.into_parts();
        let mut bytes = Vec::new();
        while bytes.len() < BODY_LIMIT {
            let Some(frame) = body.frame().await else { break };
            let frame = frame.map_err(|_| "The public response ended before it could be read. Retry the check.")?;
            if let Some(data) = frame.data_ref() {
                bytes.extend_from_slice(&data[..data.len().min(BODY_LIMIT - bytes.len())]);
            }
        }
        Ok(classify(parts.status.as_u16(), &parts.headers, &bytes, url.clone()))
    }).await;
    match result {
        Ok(Ok(check)) => check,
        Ok(Err(message)) => PublicCheck::new(PublicStatus::TransportError, message, Some(url)),
        Err(_) => PublicCheck::new(PublicStatus::TransportError, "The public HTTPS check timed out after 5 seconds. Retry when the connection is stable.", Some(url)),
    }
}

fn transport_message(error: &(dyn Error + 'static)) -> &'static str {
    let mut current = Some(error);
    while let Some(error) = current {
        // Only inspect the cause; never return raw errors, which can contain URLs or secrets.
        let text = error.to_string().to_ascii_lowercase();
        if error.is::<rustls::Error>()
            || ["certificate", "tls", "invalid peer", "handshake"]
                .iter()
                .any(|word| text.contains(word))
        {
            return "The public HTTPS check failed TLS verification or negotiation. Check the hostname's certificate and this device's trust settings.";
        }
        // io::Error::source skips some wrappers; get_ref preserves nested TLS errors.
        current = error
            .downcast_ref::<std::io::Error>()
            .and_then(std::io::Error::get_ref)
            .map(|inner| inner as &(dyn Error + 'static))
            .or_else(|| error.source());
    }
    "Could not connect to the public hostname over HTTPS. Check DNS and your connection, then retry."
}

fn header<'a>(headers: &'a HeaderMap, key: &str) -> &'a str {
    headers
        .get(key)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
}

fn classify(status: u16, headers: &HeaderMap, body: &[u8], url: String) -> PublicCheck {
    let cloudflare = header(headers, "server").eq_ignore_ascii_case("cloudflare")
        && !header(headers, "cf-ray").is_empty();
    let body = String::from_utf8_lossy(body).to_ascii_lowercase();
    let code = if status == 530 && cloudflare {
        static CODES: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
        body.trim()
            .strip_prefix("error code:")
            .and_then(|value| value.trim().parse::<u16>().ok())
            .filter(|code| (1000..2000).contains(code))
            .or_else(|| {
                if !body.contains("cloudflare tunnel error") {
                    return None;
                }
                CODES
                    .get_or_init(|| {
                        regex::Regex::new(r"(?:error(?:\s+code)?\s*:\s*|error\s+)(1[0-9]{3})\b")
                            .unwrap()
                    })
                    .captures(&body)
                    .and_then(|captures| captures[1].parse::<u16>().ok())
                    .or_else(|| body.contains(">1033<").then_some(1033))
            })
    } else {
        None
    };
    let cached = headers.contains_key("age")
        || headers
            .get_all("cf-cache-status")
            .iter()
            .filter_map(|value| value.to_str().ok())
            .any(|value| {
                value.split(',').any(|value| {
                    ["hit", "stale", "updating", "revalidated"]
                        .contains(&value.trim().to_ascii_lowercase().as_str())
                })
            })
        || header(headers, "x-cache")
            .to_ascii_lowercase()
            .contains("hit")
        || header(headers, "warning")
            .split(',')
            .any(|value| value.trim().starts_with("110 ") || value.trim().starts_with("111 "));
    let (state, message) = if matches!(status, 401 | 403)
        || ((300..400).contains(&status) && access_redirect(header(headers, "location")))
    {
        (
            PublicStatus::AuthRequired,
            "The public hostname requires authentication. The origin has not been verified.".into(),
        )
    } else if let Some(code) = code {
        (
            PublicStatus::CloudflareError,
            if code == 1033 {
                "Cloudflare returned error 1033: it could not find a healthy tunnel connector for this hostname.".into()
            } else {
                format!("Cloudflare returned error {code}. Review this hostname's Cloudflare configuration.")
            },
        )
    } else if status == 502
        && cloudflare
        && body.contains("unable to reach the origin service")
        && body.contains("cloudflared")
    {
        (PublicStatus::OriginError, "Cloudflare reached the tunnel, but cloudflared reported that it could not reach the origin service.".into())
    } else if (200..300).contains(&status) && !cached {
        (
            PublicStatus::Verified,
            format!("Public HTTPS returned HTTP {status} without a cache-hit indication."),
        )
    } else if (200..400).contains(&status) || status == 404 {
        (
            PublicStatus::Responded,
            if cached {
                format!("Public HTTPS returned cached HTTP {status}. Origin reachability has not been verified.")
            } else {
                format!("Public HTTPS responded with HTTP {status}. Origin health has not been verified.")
            },
        )
    } else {
        (PublicStatus::HttpError, format!("Public HTTPS returned HTTP {status}. This response does not identify a tunnel or origin failure."))
    };
    let mut result = PublicCheck::new(state, message, Some(url));
    result.http_status = Some(status);
    result.cloudflare_error = code;
    result
}

fn access_redirect(location: &str) -> bool {
    let Ok(uri) = location.parse::<Uri>() else {
        return false;
    };
    let login =
        uri.path() == "/cdn-cgi/access/login" || uri.path().starts_with("/cdn-cgi/access/login/");
    login
        && match uri.host() {
            Some(host) => {
                uri.scheme_str() == Some("https")
                    && host.to_ascii_lowercase().ends_with(".cloudflareaccess.com")
            }
            None => location.starts_with('/') && !location.starts_with("//"),
        }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn route(hostname: Option<&str>, path: Option<&str>, service: &str) -> RouteMapping {
        RouteMapping {
            hostname: hostname.map(str::to_owned),
            path: path.map(str::to_owned),
            service: service.into(),
            inspected: false,
            note: None,
            route_key: None, control_supported: false, enabled: true,
        }
    }

    fn response(status: u16, headers: &[(&str, &str)], body: &str) -> PublicCheck {
        let mut map = HeaderMap::new();
        for (name, value) in headers {
            map.insert(
                hyper::header::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                value.parse().unwrap(),
            );
        }
        classify(
            status,
            &map,
            body.as_bytes(),
            "https://app.example.com/".into(),
        )
    }

    #[test]
    fn responses_keep_authentication_cache_and_app_errors_distinct() {
        assert_eq!(response(200, &[], "ok").status, PublicStatus::Verified);
        assert_eq!(response(204, &[], "").status, PublicStatus::Verified);
        assert_eq!(
            response(404, &[], "app route not found").status,
            PublicStatus::Responded
        );
        assert_eq!(
            response(302, &[("location", "/welcome")], "").status,
            PublicStatus::Responded
        );
        for location in [
            "https://team.cloudflareaccess.com/cdn-cgi/access/login/app.example.com?token=secret",
            "/cdn-cgi/access/login/app.example.com?token=secret",
        ] {
            let result = response(302, &[("location", location)], "");
            assert_eq!(result.status, PublicStatus::AuthRequired);
            assert!(!result.message.contains("secret"));
        }
        assert_eq!(
            response(
                302,
                &[(
                    "location",
                    "https://team.cloudflareaccess.com.evil.test/cdn-cgi/access/login/app"
                )],
                ""
            )
            .status,
            PublicStatus::Responded
        );
        for status in [401, 403] {
            assert_eq!(
                response(status, &[], "private").status,
                PublicStatus::AuthRequired
            );
        }
        for headers in [
            vec![("cf-cache-status", "HIT")],
            vec![("cf-cache-status", "STALE")],
            vec![("age", "0")],
            vec![("warning", "110 edge \"Response is stale\"")],
        ] {
            assert_eq!(
                response(200, &headers, "ok").status,
                PublicStatus::Responded
            );
        }
        assert_eq!(
            response(200, &[("cf-cache-status", "DYNAMIC")], "ok").status,
            PublicStatus::Verified
        );
    }

    #[test]
    fn cloudflare_errors_require_status_header_and_specific_body_evidence() {
        let cf = [("server", "cloudflare"), ("cf-ray", "abc-LAX")];
        for (status, headers, body) in [
            (530, &[][..], "error code: 1033"),
            (530, &cf[..], "an unrelated upstream error"),
            (530, &cf[..], "Our app database returned error 1033"),
            (500, &cf[..], "error code: 1033"),
            (530, &[("server", "cloudflare")][..], "error code: 1033"),
        ] {
            let result = response(status, headers, body);
            assert_eq!(result.status, PublicStatus::HttpError);
            assert_eq!(result.cloudflare_error, None);
        }
        for body in [
            "error code: 1033",
            "<h1><span>1033</span></h1><h2>Cloudflare Tunnel error</h2>",
        ] {
            let result = response(530, &cf, body);
            assert_eq!(result.status, PublicStatus::CloudflareError);
            assert_eq!(result.cloudflare_error, Some(1033));
        }
        let body = "Unable to reach the origin service. It may not be responding to traffic from cloudflared. secret-token";
        assert_eq!(response(502, &[], body).status, PublicStatus::HttpError);
        assert_eq!(
            response(502, &cf, "Bad gateway").status,
            PublicStatus::HttpError
        );
        let result = response(502, &cf, body);
        assert_eq!(result.status, PublicStatus::OriginError);
        assert!(!result.message.contains("secret-token"));
    }

    #[test]
    fn test_path_proves_first_matching_ingress_rule() {
        let routes = vec![
            route(
                Some("app.example.com"),
                Some("^/api/"),
                "http://localhost:3000",
            ),
            route(
                Some("*.example.com"),
                Some("^/admin"),
                "http://localhost:4000",
            ),
            route(Some("app.example.com"), None, "http://localhost:5000"),
            route(None, None, "http_status:404"),
        ];
        assert!(test_path(&routes, 0, None).is_err());
        assert_eq!(
            test_path(&routes, 0, Some("/api/health")).unwrap(),
            "/api/health"
        );
        assert!(test_path(&routes, 2, Some("/api/health")).is_err());
        assert!(test_path(&routes, 2, Some("/admin")).is_err());
        assert_eq!(test_path(&routes, 2, None).unwrap(), "/");
        assert!(
            test_path(&routes, 1, Some("/admin")).is_err(),
            "wildcard needs a concrete hostname"
        );
        assert!(
            test_path(&routes, 3, None).is_err(),
            "fallback is not a public HTTP origin"
        );
        assert!(test_path(&routes, 4, None).is_err());
        let shadowed = vec![route(None, None, "http_status:404"), routes[2].clone()];
        assert!(
            test_path(&shadowed, 1, None).is_err(),
            "unconditional earlier rule wins"
        );
        let shadowed = vec![
            route(Some("*"), Some("^/"), "http_status:404"),
            routes[2].clone(),
        ];
        assert!(
            test_path(&shadowed, 1, None).is_err(),
            "star host with path also wins"
        );
        let shadowed = vec![
            route(Some("APP.EXAMPLE.COM"), None, "http://localhost:3000"),
            routes[2].clone(),
        ];
        assert!(
            test_path(&shadowed, 1, None).is_err(),
            "hostname normalization cannot bypass an earlier rule"
        );
    }

    #[test]
    fn rejects_unsafe_paths_and_incompatible_regex_without_guessing() {
        for path in [
            "https://evil.test/",
            "//evil.test/",
            "/?token=secret",
            "/#fragment",
            "/user@host",
            "/a\nb",
            "/a b",
            "/a\\b",
            "/a/../b",
            "/%2e%2e/",
            "/a//b",
        ] {
            assert!(validate_path(path).is_err(), "{path:?}");
        }
        assert!(validate_path(&format!("/{}", "x".repeat(2048))).is_err());
        for pattern in [
            r"(?=admin)",
            r"\p{Greek}",
            r"(?x)/ a",
            r"[a&&b]",
            r"a{1001}",
            r"(?<name>a)",
            r"(?i:a)\1",
        ] {
            assert!(path_regex(pattern).is_err(), "{pattern}");
        }
        for pattern in [
            r"^/api/.*$",
            r"(?i)^/api/[a-z]+$",
            r"^/(?:api|health)/\d+$",
            r"^/[^/]+$",
            r"(?-i)^/api",
        ] {
            assert!(path_regex(pattern).is_ok(), "{pattern}");
        }
    }

    async fn server(reply: Vec<u8>, delay: Duration) -> (String, tokio::task::JoinHandle<String>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = vec![0; 4096];
            let read = socket.read(&mut bytes).await.unwrap();
            tokio::time::sleep(delay).await;
            let _ = socket.write_all(&reply).await;
            String::from_utf8_lossy(&bytes[..read]).into_owned()
        });
        (url, task)
    }

    #[tokio::test]
    async fn synthetic_request_is_get_unauthenticated_and_never_follows_redirects() {
        let (url, request) = server(b"HTTP/1.1 302 Found\r\nLocation: https://team.cloudflareaccess.com/cdn-cgi/access/login/app\r\nSet-Cookie: secret=value\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(), Duration::ZERO).await;
        let result = probe_url(url, PUBLIC_TIMEOUT, true).await;
        assert_eq!(result.status, PublicStatus::AuthRequired);
        let request = request.await.unwrap().to_ascii_lowercase();
        assert!(request.starts_with("get / http/1.1\r\n"));
        assert!(request.contains("user-agent: underlane-synthetic-check/1.0"));
        assert!(request.contains("cache-control: no-cache, no-store"));
        assert!(!request.contains("authorization:") && !request.contains("cookie:"));
        assert!(!result.message.contains("secret"));
    }

    #[tokio::test]
    async fn body_read_is_capped_and_diagnostics_do_not_keep_body() {
        let reply = format!("HTTP/1.1 530 Error\r\nServer: cloudflare\r\nCF-Ray: abc\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}error code: 1033 secret", BODY_LIMIT + 26, "x".repeat(BODY_LIMIT));
        let (url, task) = server(reply.into_bytes(), Duration::ZERO).await;
        let result = probe_url(url, PUBLIC_TIMEOUT, true).await;
        task.await.unwrap();
        assert_eq!(
            result.status,
            PublicStatus::HttpError,
            "code beyond 64 KiB is not inspected"
        );
        assert!(!result.message.contains("secret"));
    }

    #[tokio::test]
    async fn total_deadline_includes_waiting_for_response() {
        let (url, task) = server(
            b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n".to_vec(),
            Duration::from_secs(1),
        )
        .await;
        let result = probe_url(url, Duration::from_millis(100), true).await;
        task.abort();
        assert_eq!(result.status, PublicStatus::TransportError);
        assert!(result.message.contains("timed out"));
    }

    #[tokio::test]
    async fn total_deadline_also_bounds_a_stalled_body() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = vec![0; 4096];
            socket.read(&mut bytes).await.unwrap();
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\nx")
                .await
                .unwrap();
            tokio::time::sleep(Duration::from_secs(1)).await;
        });
        let result = probe_url(url, Duration::from_millis(100), true).await;
        task.abort();
        assert_eq!(result.status, PublicStatus::TransportError);
        assert!(result.message.contains("timed out"));
    }

    #[tokio::test]
    async fn tls_protocol_failure_is_reported_as_tls() {
        let (url, task) = server(
            b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n".to_vec(),
            Duration::ZERO,
        )
        .await;
        let result = probe_url(url.replacen("http:", "https:", 1), PUBLIC_TIMEOUT, false).await;
        task.await.unwrap();
        assert_eq!(result.status, PublicStatus::TransportError);
        assert!(result.message.contains("TLS"), "{}", result.message);
    }

    #[tokio::test]
    async fn public_https_rejects_an_untrusted_certificate() {
        // Public test fixture only: this self-signed localhost key is never trusted by the client.
        let cert = b"-----BEGIN CERTIFICATE-----
MIIBlTCCATugAwIBAgIUPR/ErG+BPiYd0jTi1fImiTbY+tcwCgYIKoZIzj0EAwIw
FDESMBAGA1UEAwwJbG9jYWxob3N0MCAXDTI2MTAxOTE5MDU1MFoYDzIxMjYwOTI1
MTkwNTUwWjAUMRIwEAYDVQQDDAlsb2NhbGhvc3QwWTATBgcqhkjOPQIBBggqhkjO
PQMBBwNCAAQFd7JvJ53v6irZL+ylKW7cFCSnOA5FeMJaJb43LqUByCSQXCowa+bf
Nrr6fENpOHALJqDdB2WS6bX+x8cG0A7fo2kwZzAdBgNVHQ4EFgQUK3a6gwxyoi+A
v2I50ytU4lHsVy8wHwYDVR0jBBgwFoAUK3a6gwxyoi+Av2I50ytU4lHsVy8wDwYD
VR0TAQH/BAUwAwEB/zAUBgNVHREEDTALgglsb2NhbGhvc3QwCgYIKoZIzj0EAwID
SAAwRQIhAPcAjUOPO86x/b+3/52wyoxflnxBchsdwoLuuHVzmRFjAiBYyCuhnNfw
QFK1SCg0Zeb0LM2HsbNt5GDLyusTt91F7g==
-----END CERTIFICATE-----";
        let key = b"-----BEGIN PRIVATE KEY-----
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgKBxJNNNJBRaVY/V6
PP8ucBRqwgx45rGfvalOz7V1nf2hRANCAAQFd7JvJ53v6irZL+ylKW7cFCSnOA5F
eMJaJb43LqUByCSQXCowa+bfNrr6fENpOHALJqDdB2WS6bX+x8cG0A7f
-----END PRIVATE KEY-----"; // gitleaks:allow -- public self-signed localhost fixture; never used outside this rejection test.
        let certs = rustls_pemfile::certs(&mut &cert[..])
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let key = rustls_pemfile::private_key(&mut &key[..]).unwrap().unwrap();
        let tls = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!(
            "https://localhost:{}/",
            listener.local_addr().unwrap().port()
        );
        let task = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            tokio_rustls::TlsAcceptor::from(Arc::new(tls))
                .accept(socket)
                .await
                .is_err()
        });
        let result = probe_url(url, PUBLIC_TIMEOUT, false).await;
        assert_eq!(result.status, PublicStatus::TransportError);
        assert!(result.message.contains("TLS"), "{}", result.message);
        assert!(
            task.await.unwrap(),
            "self-signed certificate must be rejected"
        );
    }

    #[tokio::test]
    async fn service_probe_distinguishes_tcp_reachability_from_unsupported() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        for scheme in ["http", "https", "tcp", "ssh", "rdp", "smb"] {
            assert_eq!(
                probe_service(&format!("{scheme}://{addr}")).await.status,
                ServiceStatus::Reachable
            );
        }
        drop(listener);
        assert_eq!(
            probe_service(&format!("http://{addr}")).await.status,
            ServiceStatus::Unreachable
        );
        for service in [
            "http_status:404",
            "hello_world",
            "unix:/tmp/app.sock",
            "bastion",
        ] {
            assert_eq!(
                probe_service(service).await.status,
                ServiceStatus::Unsupported
            );
        }
    }

    #[test]
    fn tls_error_messages_are_specific_and_never_copy_raw_errors() {
        let tls = rustls::Error::General("private-secret".into());
        assert!(transport_message(&tls).contains("TLS"));
        assert!(!transport_message(&tls).contains("private-secret"));
        let refused = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "private-secret");
        assert!(transport_message(&refused).contains("Could not connect"));
        assert!(!transport_message(&refused).contains("private-secret"));
    }

    #[tokio::test]
    async fn unsupported_hostnames_return_without_a_network_request() {
        for hostname in [
            "*.example.com",
            "app.example.com:443",
            "https://app.example.com",
            "app.example.com@evil.test",
        ] {
            let result = probe_public(hostname, "/").await;
            assert_eq!(result.status, PublicStatus::Unsupported);
            assert_eq!(result.test_url, None);
        }
    }
}
