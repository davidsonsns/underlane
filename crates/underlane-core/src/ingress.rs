//! Offline matching against the imported source on disk, never the derived running config.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;
use serde_norway::Value;

use crate::cli::{self, Cancellation, ConfigPolicy, Invocation};
use crate::config::TunnelConfig;
use crate::secret::{redact_text, Secret};
use crate::supervisor::RouteMapping;

#[derive(Debug, Clone, Serialize)]
pub struct IngressMatch {
    pub source_path: String,
    pub rule_index: usize,
    pub rules: Vec<RouteMapping>,
    pub raw: String,
}

const RETRY: &str = "Config changed during the test. Test the URL again.";

/// Validate before spawning; credentials, query and fragment do not participate in matching.
pub fn match_url(input: &str) -> Result<String, String> {
    let input = input.trim();
    let invalid = || "Enter a complete http:// or https:// URL with a hostname.".to_string();
    if input.chars().any(|c| c.is_whitespace() || c == '\\') { return Err(invalid()); }
    let bytes = input.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'%' && (!bytes.get(index + 1).is_some_and(u8::is_ascii_hexdigit) || !bytes.get(index + 2).is_some_and(u8::is_ascii_hexdigit)) {
            return Err(invalid());
        }
    }
    let (scheme, rest) = input.split_once("://").ok_or_else(invalid)?;
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") { return Err(invalid()); }
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = rest[..end].rsplit('@').next().unwrap_or_default();
    let path = rest[end..].split(['?', '#']).next().unwrap_or_default();
    let safe = format!("{}://{authority}{path}", scheme.to_ascii_lowercase());
    let uri: hyper::Uri = safe.parse().map_err(|_| invalid())?;
    let host = uri.host().ok_or_else(invalid)?;
    let port = if authority.starts_with('[') { authority.split_once(']').map(|(_, tail)| tail) } else { authority.split_once(':').map(|(_, port)| port) };
    let bad_port = port.is_some_and(|port| !port.is_empty() && if authority.starts_with('[') { port.strip_prefix(':').is_none_or(|port| port.parse::<u16>().is_err()) } else { port.parse::<u16>().is_err() });
    if host.is_empty() || host.contains('%') || authority.ends_with(':') || bad_port {
        return Err(invalid());
    }
    Ok(safe)
}

/// URL values are opaque diagnostics; removing all query/fragment values avoids custom-key leaks.
fn safe_urls(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    loop {
        let lower = rest.to_ascii_lowercase();
        let next = ["https://", "http://", "tcp://", "ssh://", "rdp://", "smb://", "socks5://"].iter().filter_map(|scheme| lower.find(scheme)).min();
        let Some(start) = next else { out.push_str(rest); break };
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        let end = rest.find(|c: char| c.is_whitespace() || matches!(c, '\'' | '"' | '<' | '>')).unwrap_or(rest.len());
        let url = &rest[..end];
        let (scheme, body) = url.split_once("://").unwrap();
        let authority_end = body.find(['/', '?', '#']).unwrap_or(body.len());
        let authority = &body[..authority_end];
        out.push_str(scheme);
        out.push_str("://");
        if let Some((_, host)) = authority.rsplit_once('@') {
            out.push_str("[REDACTED]@"); out.push_str(host);
        } else { out.push_str(authority); }
        let suffix = &body[authority_end..];
        let clean_end = suffix.find(['?', '#']).unwrap_or(suffix.len());
        out.push_str(&suffix[..clean_end]);
        if clean_end < suffix.len() { out.push_str("?[REDACTED]"); }
        rest = &rest[end..];
    }
    out
}

fn config_secrets(value: &Value, secrets: &mut Vec<Secret<String>>) {
    fn collect(value: &Value, protected: bool, secrets: &mut Vec<Secret<String>>) {
        match value {
            Value::Mapping(map) => for (key, value) in map {
                let key = key.as_str().unwrap_or_default().to_ascii_lowercase().replace(['-', '_'], "");
                let protected = protected || matches!(key.as_str(), "token" | "apitoken" | "apikey" | "tunnelsecret" | "credentialscontents" | "password" | "authorization" | "cookie" | "headers" | "httphostheader");
                collect(value, protected, secrets);
            },
            Value::Sequence(values) => for value in values { collect(value, protected, secrets); },
            Value::String(value) if protected && !value.is_empty() => secrets.push(Secret::new(value.clone())),
            _ => {},
        }
    }
    collect(value, false, secrets);
}

fn diagnostic(text: &str, secrets: &[Secret<String>]) -> String {
    let mut safe = text.to_string();
    for secret in secrets { safe = safe.replace(secret.expose(), "[REDACTED]"); }
    safe_urls(&redact_text(&safe))
}

fn matched_index(text: &str, count: usize) -> Result<usize, String> {
    let mut indices = text.lines().filter_map(|line| line.trim().strip_prefix("Matched rule #"));
    let index = indices.next().and_then(|s| s.parse::<usize>().ok());
    match index {
        Some(index) if index < count && indices.next().is_none() => Ok(index),
        _ => Err("cloudflared returned an invalid rule match. Test the URL again.".into()),
    }
}

struct Snapshot { dir: PathBuf, path: PathBuf }

impl Snapshot {
    fn create(bytes: &[u8]) -> std::io::Result<Self> {
        let dir = std::env::temp_dir().join(format!("underlane-ingress-{}", uuid::Uuid::new_v4()));
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        { use std::os::unix::fs::DirBuilderExt; builder.mode(0o700); }
        builder.create(&dir)?;
        let snapshot = Self { path: dir.join("config.yml"), dir };
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        { use std::os::unix::fs::OpenOptionsExt; options.mode(0o600); }
        use std::io::Write;
        options.open(&snapshot.path)?.write_all(bytes)?;
        Ok(snapshot)
    }
}

impl Drop for Snapshot {
    fn drop(&mut self) { let _ = std::fs::remove_file(&self.path); let _ = std::fs::remove_dir(&self.dir); }
}

pub async fn test_rule(bin: &Path, path: &Path, url: &str, cancel: Cancellation) -> Result<IngressMatch, String> {
    let url = match_url(url)?;
    let source: PathBuf = if path.is_absolute() { path.into() } else {
        std::env::current_dir().map_err(|_| "Could not resolve the config path.")?.join(path)
    };
    let read_path = source.clone();
    let bytes = tokio::task::spawn_blocking(move || std::fs::read(read_path)).await
        .map_err(|_| "Could not read the config on disk.")?.map_err(|_| "Could not read the config on disk.")?;
    let text = std::str::from_utf8(&bytes).map_err(|_| "Config on disk is not valid UTF-8 YAML.")?;
    // YAML parser errors can quote source secrets; expose a concise generic error.
    let config = TunnelConfig::parse(&source, text).map_err(|_| "Config on disk is not valid YAML or ingress configuration.")?;
    if !config.raw.as_mapping().and_then(|m| m.get("ingress")).is_some_and(|v| v.as_sequence().is_some_and(|rules| !rules.is_empty())) {
        return Err("This config has no local ingress list. URL testing requires locally managed hostname rules.".into());
    }
    let mut secrets = Vec::new();
    config_secrets(&config.raw, &mut secrets);
    secrets.sort_by_key(|s| std::cmp::Reverse(s.expose().len()));
    // A private immutable copy closes A -> B -> A races while cloudflared opens its config.
    let captured = bytes.clone();
    let snapshot = tokio::task::spawn_blocking(move || Snapshot::create(&captured)).await
        .map_err(|_| "Could not prepare the offline config snapshot.")?.map_err(|_| "Could not prepare the offline config snapshot.")?;
    let mut inv = Invocation::new(["ingress", "rule", url.as_str()], ConfigPolicy::Target(snapshot.path.clone()));
    inv.cwd = source.parent().map(Path::to_path_buf);
    inv.timeout = Duration::from_secs(10);
    inv.cancel = cancel.clone();
    // The offline operation needs no account or token environment, even when the app has one.
    inv.clear_env = true;
    inv.env.push(("PATH".into(), Secret::new("/usr/bin:/bin".into())));
    let outcome = cli::exec(bin, inv).await;
    let read_path = source.clone();
    let current = tokio::task::spawn_blocking(move || std::fs::read(read_path)).await
        .map_err(|_| RETRY)?.map_err(|_| RETRY)?;
    if current != bytes { return Err(RETRY.into()); }
    if cancel.is_cancelled() { return Err("Ingress test cancelled.".into()); }
    let remap = |text: &str| diagnostic(&text.replace(snapshot.path.to_string_lossy().as_ref(), source.to_string_lossy().as_ref()), &secrets);
    let output = outcome.map_err(|e| remap(&e.message))?;
    let raw = remap(&format!("{}\n{}", output.stdout.text().unwrap_or_default(), output.stderr));
    let rule_index = matched_index(output.stdout.text().unwrap_or_default(), config.ingress.len())?;
    let rules = config.ingress.iter().map(|r| RouteMapping {
        hostname: r.hostname.as_ref().map(|s| safe_urls(&redact_text(s))),
        path: r.path.as_ref().map(|s| safe_urls(&redact_text(s))),
        service: safe_urls(&redact_text(&r.service)), inspected: false, note: None,
    }).collect();
    Ok(IngressMatch { source_path: source.to_string_lossy().into_owned(), rule_index, rules, raw: raw.trim().into() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_full_urls_and_excludes_credentials_query_fragment() {
        assert_eq!(match_url("https://user:invented-pass@example.invalid/a?token=invented-query#invented-fragment").unwrap(), "https://example.invalid/a");
        for invalid in ["example.invalid", "ftp://example.invalid/a", "https:///a", "http://", "http://example.invalid:70000/a", "http://example.invalid:abc/a", "http://example.invalid\\evil/a", "https://example.invalid/%XX", "http://example.invalid:/a", "https://example.invalid/a b"] {
            assert!(match_url(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn parses_only_one_zero_based_bounded_index() {
        assert_eq!(matched_index("Using rules\nMatched rule #0\nservice: http_status:404", 2).unwrap(), 0);
        for malformed in ["OK", "Matched rule #-1", "Matched rule #2", "Matched rule #999999999999999999999999", "Matched rule #0 extra", "Matched rule #0\nMatched rule #1"] {
            assert!(matched_index(malformed, 2).is_err(), "{malformed}");
        }
    }

    #[test]
    fn redacts_echoed_urls_and_nonpublic_config_fields() {
        let config = TunnelConfig::parse(Path::new("synthetic.yml"), "token: invented-token\ningress:\n - service: https://user:invented-pass@localhost:1/a?custom=invented-query\n   originRequest:\n     headers:\n       X-Secret: invented-header\n").unwrap();
        let mut secrets = Vec::new();
        config_secrets(&config.raw, &mut secrets);
        let safe = diagnostic("token: invented-token\nX-Secret: invented-header\nHTTPS://user:invented-pass@example.invalid/a?custom=invented-query", &secrets);
        assert!(!safe.contains("invented-"), "{safe}");
        assert!(safe.contains("example.invalid/a"));
    }
}
