//! Derived configs: a copy of the user's config that Burrow runs instead of the original.
//!
//! The copy always lives in Burrow's data directory (so every process Burrow starts can be
//! identified as its own) and has relative paths made absolute. With inspection on, eligible
//! ingress services are pointed at Burrow's loopback proxy. The user's file is never written.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_norway::{Mapping, Value};

use crate::config::{resolve_path, TunnelConfig};

/// Where the proxy for one inspected rule forwards to, with the `originRequest` options the
/// proxy (not cloudflared) now has to honour.
#[derive(Debug, Clone, PartialEq)]
pub struct Upstream {
    pub rule_index: usize,
    pub hostname: Option<String>,
    /// `http` or `https` (`ws`/`wss` are mapped to these).
    pub scheme: String,
    /// `host:port` (port always explicit).
    pub authority: String,
    pub no_tls_verify: bool,
    pub origin_server_name: Option<String>,
    pub ca_pool: Option<PathBuf>,
    pub connect_timeout: Duration,
    pub tls_timeout: Duration,
}

impl Upstream {
    /// Build an upstream from a plain service URL (quick tunnels).
    pub fn from_service(rule_index: usize, hostname: Option<String>, service: &str) -> Option<Self> {
        let (scheme, authority) = split_service(service)?;
        Some(Upstream {
            rule_index,
            hostname,
            scheme,
            authority,
            no_tls_verify: false,
            origin_server_name: None,
            ca_pool: None,
            connect_timeout: Duration::from_secs(30),
            tls_timeout: Duration::from_secs(10),
        })
    }
}

/// Split `http://localhost:3000/` into (`http`, `localhost:3000`). `ws`/`wss` map to
/// `http`/`https`. Returns None for anything that isn't HTTP.
pub fn split_service(service: &str) -> Option<(String, String)> {
    let service = service.trim();
    let (scheme, rest) = service.split_once("://")?;
    let scheme = match scheme.to_ascii_lowercase().as_str() {
        "http" | "ws" => "http",
        "https" | "wss" => "https",
        _ => return None,
    }
    .to_string();
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let authority = authority.rsplit_once('@').map(|(_, a)| a).unwrap_or(authority);
    if authority.is_empty() {
        return None;
    }
    let has_port = match authority.find(']') {
        Some(end) => authority[end..].contains(':'),
        None => authority.contains(':'),
    };
    let authority = if has_port {
        authority.to_string()
    } else {
        format!("{authority}:{}", if scheme == "https" { 443 } else { 80 })
    };
    Some((scheme, authority))
}

/// The outcome of deciding which rules get inspected.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InspectionPlan {
    pub upstreams: Vec<Upstream>,
    /// Rules that point at an HTTP service but can't be inspected, with the reason.
    pub skipped: Vec<(usize, String)>,
}

/// Decide which rules are inspected, merging top-level and rule-level `originRequest`.
pub fn plan(config: &TunnelConfig) -> InspectionPlan {
    let base_dir = config.source.parent().unwrap_or(Path::new("."));
    let root = config.raw.as_mapping();
    let top_or = root.and_then(|m| m.get("originRequest")).and_then(Value::as_mapping);
    let rules = root.and_then(|m| m.get("ingress")).and_then(Value::as_sequence);
    let mut plan = InspectionPlan::default();

    for rule in &config.ingress {
        let lower = rule.service.trim().to_ascii_lowercase();
        if lower.starts_with("unix:") || lower.starts_with("unix+tls:") {
            plan.skipped.push((rule.index, "Unix-socket services aren't inspected in this version.".into()));
            continue;
        }
        let Some(mut up) = Upstream::from_service(rule.index, rule.hostname.clone(), &rule.service) else {
            continue;
        };
        let rule_or = rules
            .and_then(|seq| seq.get(rule.index))
            .and_then(Value::as_mapping)
            .and_then(|m| m.get("originRequest"))
            .and_then(Value::as_mapping);
        let get = |key: &str| rule_or.and_then(|m| m.get(key)).or_else(|| top_or.and_then(|m| m.get(key)));

        if get("http2Origin").and_then(Value::as_bool) == Some(true) {
            plan.skipped.push((rule.index, "Uses http2Origin (HTTP/2 or gRPC origin); traffic goes straight to the origin.".into()));
            continue;
        }
        if get("matchSNItoHost").and_then(Value::as_bool) == Some(true) {
            plan.skipped.push((rule.index, "Uses matchSNItoHost; traffic goes straight to the origin.".into()));
            continue;
        }
        up.no_tls_verify = get("noTLSVerify").and_then(Value::as_bool).unwrap_or(false);
        up.origin_server_name = get("originServerName").and_then(Value::as_str).filter(|s| !s.is_empty()).map(String::from);
        up.ca_pool = get("caPool").and_then(Value::as_str).filter(|s| !s.is_empty()).map(|p| resolve_path(base_dir, p));
        if let Some(d) = get("connectTimeout").and_then(parse_duration) {
            up.connect_timeout = d;
        }
        if let Some(d) = get("tlsTimeout").and_then(parse_duration) {
            up.tls_timeout = d;
        }
        plan.upstreams.push(up);
    }
    plan
}

/// cloudflared durations are Go-style strings (`30s`, `1m30s`) or bare seconds.
pub fn parse_duration(v: &Value) -> Option<Duration> {
    match v {
        Value::Number(n) => n.as_u64().map(Duration::from_secs),
        Value::String(s) => parse_duration_str(s),
        _ => None,
    }
}

pub fn parse_duration_str(s: &str) -> Option<Duration> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if let Ok(n) = s.parse::<u64>() {
        return Some(Duration::from_secs(n));
    }
    let mut total = 0f64;
    let mut num = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_ascii_digit() || c == '.' {
            num.push(c);
            continue;
        }
        let mut unit = c.to_string();
        if c == 'm' && chars.peek() == Some(&'s') {
            unit.push(chars.next().unwrap());
        }
        let n: f64 = num.parse().ok()?;
        num.clear();
        total += match unit.as_str() {
            "h" => n * 3600.0,
            "m" => n * 60.0,
            "s" => n,
            "ms" => n / 1000.0,
            _ => return None,
        };
    }
    if !num.is_empty() {
        return None;
    }
    Some(Duration::from_secs_f64(total))
}

/// Produce the derived document.
///
/// `proxy_ports` maps an ingress rule index to the loopback port of its inspecting proxy; rules
/// without an entry keep their service. Relative paths are made absolute against the original
/// file's directory, which is also the working directory cloudflared is started in.
pub fn derive(config: &TunnelConfig, proxy_ports: &HashMap<usize, u16>) -> Value {
    let base_dir = config.source.parent().unwrap_or(Path::new("."));
    let mut doc = config.raw.clone();
    let Some(root) = doc.as_mapping_mut() else {
        return doc;
    };

    for key in ["credentials-file", "origincert", "logfile", "log-directory", "pidfile"] {
        absolutize(root, key, base_dir);
    }
    if let Some(Value::Mapping(or)) = root.get_mut("originRequest") {
        absolutize(or, "caPool", base_dir);
    }

    if config.is_single_origin() {
        if let Some(port) = proxy_ports.get(&0) {
            root.insert("url".into(), Value::String(loopback(*port)));
        }
        return doc;
    }

    if let Some(Value::Sequence(rules)) = root.get_mut("ingress") {
        for (index, rule) in rules.iter_mut().enumerate() {
            let Some(rule) = rule.as_mapping_mut() else { continue };
            if let Some(Value::Mapping(or)) = rule.get_mut("originRequest") {
                absolutize(or, "caPool", base_dir);
            }
            if let Some(port) = proxy_ports.get(&index) {
                rule.insert("service".into(), Value::String(loopback(*port)));
            }
        }
    }
    doc
}

fn loopback(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
}

fn absolutize(map: &mut Mapping, key: &str, base_dir: &Path) {
    if let Some(Value::String(p)) = map.get(key) {
        let abs = resolve_path(base_dir, p);
        map.insert(key.into(), Value::String(abs.to_string_lossy().into_owned()));
    }
}

/// Serialize and write the derived document. The header makes it obvious the file is generated.
pub fn write_derived(doc: &Value, original: &Path, out: &Path) -> std::io::Result<()> {
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let body = serde_norway::to_string(doc).map_err(std::io::Error::other)?;
    let text = format!(
        "# Generated by Burrow from {}.\n# Edits here are overwritten; change the original file instead.\n{body}",
        original.display()
    );
    std::fs::write(out, text)
}

/// Check that a named tunnel's credentials file exists and matches its `tunnel:` UUID.
/// `ingress validate` checks neither, and cloudflared's own error is cryptic.
pub fn preflight_credentials(config: &TunnelConfig) -> Result<(), String> {
    let Some(cred) = &config.credentials_file else {
        return Ok(());
    };
    let text = std::fs::read_to_string(cred).map_err(|_| {
        format!("The credentials file {} doesn't exist. Run `cloudflared tunnel create` again or fix `credentials-file`.", cred.display())
    })?;
    let id = serde_json::from_str::<serde_json::Value>(&text)
        .ok()
        .and_then(|v| v.get("TunnelID").and_then(|t| t.as_str()).map(String::from));
    if let (Some(id), Some(tunnel)) = (id, &config.tunnel) {
        if looks_like_uuid(tunnel) && !id.eq_ignore_ascii_case(tunnel) {
            return Err(format!(
                "The credentials file belongs to tunnel {id}, but the config says `tunnel: {tunnel}`."
            ));
        }
    }
    Ok(())
}

pub fn looks_like_uuid(s: &str) -> bool {
    let s = s.trim();
    s.len() == 36 && s.chars().enumerate().all(|(i, c)| if [8, 13, 18, 23].contains(&i) { c == '-' } else { c.is_ascii_hexdigit() })
}
