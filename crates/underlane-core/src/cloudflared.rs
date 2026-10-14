//! Finding and talking to the `cloudflared` binary.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::Serialize;
use tokio::process::Command;

use crate::config::{expand_tilde, home_dir};
use crate::proxy::capture::redact_json_headers;
use crate::cli::{self, ConfigPolicy, Invocation};
use crate::secret::redact_text;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Binary {
    pub path: String,
    pub version: Option<String>,
    /// How it was found: "settings", "path", or "common location".
    pub source: String,
}

/// Locations checked after the configured path and `PATH`.
pub fn common_locations() -> Vec<PathBuf> {
    vec![
        PathBuf::from("/opt/homebrew/bin/cloudflared"),
        PathBuf::from("/usr/local/bin/cloudflared"),
        home_dir().join(".local/bin/cloudflared"),
        home_dir().join("bin/cloudflared"),
        PathBuf::from("/usr/bin/cloudflared"),
        PathBuf::from("/home/linuxbrew/.linuxbrew/bin/cloudflared"),
    ]
}

fn is_executable(p: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        p.is_file() && p.metadata().map(|m| m.permissions().mode() & 0o111 != 0).unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        p.is_file()
    }
}

/// `PATH` as a login shell sees it. Apps launched from Finder or at login get a minimal PATH
/// that doesn't include Homebrew, so ask the user's shell.
pub async fn login_shell_path() -> Option<String> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let out = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        Command::new(shell)
            .args(["-lc", "printf %s \"$PATH\""])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output(),
    )
    .await
    .ok()?
    .ok()?;
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}

fn search_path(path_var: &str) -> Vec<PathBuf> {
    std::env::split_paths(path_var).map(|d| d.join("cloudflared")).filter(|p| is_executable(p)).collect()
}

/// Locate cloudflared: explicit setting, then PATH (process and login shell), then common locations.
pub async fn discover(configured: Option<&str>) -> Option<Binary> {
    let mut candidates: Vec<(PathBuf, &str)> = Vec::new();
    if let Some(c) = configured.filter(|c| !c.trim().is_empty()) {
        candidates.push((expand_tilde(c.trim()), "settings"));
    }
    if let Ok(path) = std::env::var("PATH") {
        candidates.extend(search_path(&path).into_iter().map(|p| (p, "path")));
    }
    if let Some(path) = login_shell_path().await {
        candidates.extend(search_path(&path).into_iter().map(|p| (p, "path")));
    }
    for p in common_locations() {
        candidates.push((p, "common location"));
    }
    discover_candidates(candidates).await
}

async fn discover_candidates(candidates: Vec<(PathBuf, &str)>) -> Option<Binary> {
    let mut checked = HashSet::new();
    for (p, source) in candidates {
        if !checked.insert(p.clone()) { continue; }
        if is_executable(&p) {
            if let Some(version) = version(&p).await {
                return Some(Binary { path: p.to_string_lossy().into_owned(), version: Some(version), source: source.into() });
            }
        }
        if source == "settings" {
            // An explicit path that cannot run cloudflared must not silently fall back.
            return None;
        }
    }
    None
}

/// `cloudflared version 2026.9.1 (built ...)` -> `2026.9.1`.
pub async fn version(bin: &Path) -> Option<String> {
    let out = cli::exec(bin, Invocation::new(["--version"], ConfigPolicy::None)).await.ok()?;
    let text = out.stdout.text().unwrap_or_default().to_string() + &out.stderr;
    parse_version(&text)
}

pub fn parse_version(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let version = line.trim().strip_prefix("cloudflared version ")?.split_whitespace().next()?;
        let parts: Vec<_> = version.split('.').collect();
        (parts.len() == 3 && parts.iter().all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit())))
            .then(|| version.to_string())
    })
}

#[cfg(all(test, unix))]
mod discovery_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[tokio::test]
    async fn automatic_search_skips_broken_binaries_but_explicit_selection_does_not() {
        let directory = tempfile::tempdir().unwrap();
        let broken = directory.path().join("broken");
        let valid = directory.path().join("valid");
        std::fs::write(&broken, "#!/bin/sh\necho 'another tool version 2026.9.1'\n").unwrap();
        std::fs::write(&valid, "#!/bin/sh\necho 'cloudflared version 2026.9.1'\n").unwrap();
        for path in [&broken, &valid] {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let found = discover_candidates(vec![(broken.clone(), "path"), (valid.clone(), "common location")]).await.unwrap();
        assert_eq!(found.path, valid.to_string_lossy());
        assert!(discover_candidates(vec![(broken, "settings"), (valid, "path")]).await.is_none());
    }

    #[test]
    fn version_requires_cloudflared_identity_and_numeric_release() {
        assert_eq!(parse_version("cloudflared version 2026.9.1 (built test)"), Some("2026.9.1".into()));
        for output in ["other version 2026.9.1", "cloudflared version arbitrary", "cloudflared version 2026.9"] {
            assert_eq!(parse_version(output), None);
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Validation {
    pub ok: bool,
    /// cloudflared's own message.
    pub raw: String,
    /// Plain-language explanation and fix, when we recognise the message.
    pub hint: Option<String>,
    pub failure: Option<FailureExplanation>,
}

/// Safe, ephemeral explanation of an observed failure; never an account or DNS assertion.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FailureExplanation {
    pub cause: String,
    pub action: Option<String>,
    pub raw: String,
}

impl FailureExplanation {
    pub fn new(cause: &str, action: Option<&str>, raw: &str) -> Self {
        Self { cause: safe_diagnostic(cause), action: action.map(safe_diagnostic), raw: safe_diagnostic(raw) }
    }

    pub fn from_error(error: &cli::CfError) -> Self {
        use cli::ErrorKind::*;
        let cause = match error.kind {
            BinaryMissing => "cloudflared could not be found.",
            Io => "cloudflared could not start or read its output.",
            Timeout => "cloudflared did not finish before the timeout.",
            Cancelled => "The cloudflared command was cancelled.",
            Usage => "cloudflared rejected a command option.",
            Config => "The config could not be read or validated.",
            Credentials => "The tunnel credentials are missing or invalid.",
            NotLoggedIn => "cloudflared could not find the account certificate.",
            AlreadyLoggedIn => "cloudflared refused to overwrite an existing certificate.",
            Auth => "cloudflared reported rejected credentials.",
            TunnelNotFound => "cloudflared could not find the requested tunnel.",
            NameExists => "cloudflared reported an existing tunnel name.",
            ActiveConnections => "cloudflared reported active connectors.",
            DnsRecordExists => "cloudflared reported an existing DNS record.",
            RateLimited => "cloudflared reported rate limiting.",
            MetricsBind => "The metrics port is already in use.",
            IncompatibleOrigin => "The config combines incompatible origin options.",
            EdgeConnection => "cloudflared could not connect to the Cloudflare edge.",
            OriginConnection => "cloudflared reported an origin connection failure.",
            InvalidJson => "cloudflared returned an unexpected response.",
            Unknown => "cloudflared reported an unrecognized failure.",
        };
        let action = error.hint.as_deref().or(match error.kind {
            BinaryMissing => Some("Install cloudflared or set its executable path in Settings, then restart."),
            Io => Some("Check the executable path, permissions and diagnostic below, then restart."),
            Timeout => Some("Check the diagnostic below and retry when cloudflared is responding."),
            InvalidJson => Some("Check the installed cloudflared version and the diagnostic below."),
            Unknown => Some("Review the diagnostic and logs, then try again."),
            _ => None,
        });
        Self::new(cause, action, &error.message)
    }
}

fn redact_diagnostic_urls(text: &str, query: &HashSet<String>) -> String {
        let mut safe = text.to_string();
        let mut offset = 0;
        while let Some(pos) = safe[offset..].find("://") {
            let start = offset + pos + 3;
            let end = safe[start..].find(|c: char| matches!(c, '/' | '?' | '#' | '\"' | '\'' | '\\') || c.is_whitespace()).map(|p| start + p).unwrap_or(safe.len());
            if let Some(at) = safe[start..end].rfind('@') {
                safe.replace_range(start..start + at, "[redacted]");
                offset = start + "[redacted]@".len();
            } else { offset = end; }
        }
        safe.split_inclusive(char::is_whitespace).map(|part| {
            let token = part.trim_end_matches(char::is_whitespace);
            format!("{}{}", crate::proxy::capture::redact_query_in_text(token, query), &part[token.len()..])
        }).collect()
    }
/// Redact URLs and credentials in diagnostic text as well as the existing argv/JSON secrets.
pub fn safe_diagnostic(text: &str) -> String {
    let safe = redact_text(text);
    let query = crate::proxy::capture::CaptureOptions::default().query_redaction_set();
    fn value(v: &mut serde_json::Value, query: &HashSet<String>) {
        match v {
            serde_json::Value::String(s) => *s = redact_diagnostic_urls(s, query),
            serde_json::Value::Object(o) => o.values_mut().for_each(|v| value(v, query)),
            serde_json::Value::Array(a) => a.iter_mut().for_each(|v| value(v, query)),
            _ => {}
        }
    }
    if let Ok(mut json) = serde_json::from_str::<serde_json::Value>(&safe) {
        value(&mut json, &query);
        json.to_string()
    } else { redact_diagnostic_urls(&safe, &query) }
}

/// Only diagnostics participate in classification: payload names, IDs and paths do not.
pub(crate) fn diagnostic_text(text: &str) -> String {
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(text) {
        return match json {
            serde_json::Value::Object(map) if map.contains_key("level") => {
                let level = map.get("level").and_then(|v| v.as_str()).map(normalize_level).unwrap_or_default();
                if !matches!(level.as_str(), "error" | "fatal") { return String::new(); }
                ["message", "error"].into_iter().filter_map(|key| map.get(key).and_then(|v| v.as_str())).collect::<Vec<_>>().join("\n")
            }
            _ => String::new(),
        };
    }
    text.lines().filter_map(|line| {
        let line = line.trim();
        if line.to_ascii_lowercase().starts_with("warning:") || line.to_ascii_lowercase().starts_with("warn:") { return None; }
        if line.starts_with("Validating rules from ") || line.starts_with("Using rules from ") { return None; }
        if serde_json::from_str::<serde_json::Value>(line).is_ok() { return Some(diagnostic_text(line)); }
        // Incomplete JSON is useful raw evidence, but its payload values are not anchors.
        if line.starts_with('{') || line.starts_with('[') { return None; }
        if [" INF ", " WRN ", " DBG "].iter().any(|tag| line.contains(tag)) { return None; }
        Some(line.to_string())
    }).collect::<Vec<_>>().join("\n")
}

/// Run `cloudflared tunnel --config <file> ingress validate` from the config's directory
/// (cloudflared resolves relative paths against its working directory).
pub async fn validate(bin: &Path, config: &Path) -> Validation {
    validate_cancellable(bin, config, cli::Cancellation::new()).await
}

pub async fn validate_cancellable(bin: &Path, config: &Path, cancel: cli::Cancellation) -> Validation {
    let mut inv = Invocation::new(["ingress", "validate"], ConfigPolicy::Target(config.into()));
    inv.cancel = cancel;
    match cli::exec(bin, inv).await {
        Ok(o) => {
            let text = format!("{}{}", o.stdout.text().unwrap_or_default(), o.stderr);
            interpret_validation(true, &text)
        }
        Err(e) => {
            let mut validation = interpret_validation(false, &e.message);
            validation.hint = validation.hint.or(e.hint.clone());
            validation.failure = Some(FailureExplanation::from_error(&e));
            validation
        }
    }
}

pub fn interpret_validation(success: bool, output: &str) -> Validation {
    let safe = safe_diagnostic(output);
    let raw = safe
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("Validating rules from"))
        .collect::<Vec<_>>()
        .join("\n");
    let raw = raw.strip_prefix("Validation failed: ").unwrap_or(&raw).to_string();
    if success && cli::CfError::classify(Some(0), &raw, "").is_ok() {
        return Validation { ok: true, raw, hint: None, failure: None };
    }
    let hint = friendly_hint(&raw).or_else(|| cli::CfError::classify(Some(0), &raw, "").err().and_then(|e| e.hint));
    let mut error = cli::CfError::classify(Some(1), output, "").expect_err("failed validation");
    error.hint = hint.clone();
    Validation { ok: false, failure: Some(FailureExplanation::from_error(&error)), hint, raw }
}

/// Map known cloudflared validation errors to plain language.
pub fn friendly_hint(msg: &str) -> Option<String> {
    let safe = safe_diagnostic(msg);
    let diagnostic = diagnostic_text(&safe);
    let msg = diagnostic.as_str();
    let m = msg.to_ascii_lowercase();
    let hint = if m.contains("last ingress rule must match all urls") {
        "The last rule has to catch everything that didn't match earlier rules. Add a final rule without a hostname, for example `- service: http_status:404`.".to_string()
    } else if m.contains("is an invalid address") && m.contains("scheme") {
        "A service address is missing its scheme. Write it as a full URL like `http://localhost:3000`.".to_string()
    } else if m.contains("will match every hostname") {
        let rule = msg.split_whitespace().find(|w| w.starts_with('#')).unwrap_or("A rule");
        format!("Rule {rule} has no hostname, so it catches all traffic and the rules after it never run. Give it a hostname or move it to the end.")
    } else if m.contains("no ingress rules") || m.contains("no rules") {
        "The file has no `ingress:` rules. Add at least one rule that ends with a catch-all such as `- service: http_status:404`.".to_string()
    } else if m.contains("hostname") && m.contains("wildcard") {
        "Wildcards are only allowed as the left-most label, for example `*.example.com`.".to_string()
    } else if m.contains("not valid yaml") || m.contains("yaml:") || m.contains("unmarshal") {
        "The file isn't valid YAML. Check indentation and that each rule starts with `- `.".to_string()
    } else if m.contains("no such file") || m.contains("cannot find") {
        "The config file couldn't be read. It may have been moved or deleted.".to_string()
    } else if m.contains("path") && m.contains("regex") {
        "A rule's `path` isn't a valid regular expression.".to_string()
    } else {
        return None;
    };
    Some(hint)
}

/// A parsed stderr/stdout line from cloudflared.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedLog {
    pub ts_ms: Option<i64>,
    pub level: String,
    pub message: String,
    /// Remaining JSON fields as an object string.
    pub fields: String,
}

/// Parse one line of `--output json` (falls back to the raw text for anything else).
pub fn parse_log_line(line: &str, redact: &HashSet<String>) -> Option<ParsedLog> {
    parse_log_line_with(line, redact, None)
}

/// Redact sensitive query parameters in every string inside a JSON value (cloudflared puts
/// full URLs in fields such as `dest`, `url` and `error`).
fn redact_query_in_json(v: &mut serde_json::Value, q: &HashSet<String>) {
    match v {
        serde_json::Value::String(s) => {
            if s.contains('?') {
                *s = redact_diagnostic_urls(s, q);
            }
        }
        serde_json::Value::Array(a) => a.iter_mut().for_each(|x| redact_query_in_json(x, q)),
        serde_json::Value::Object(o) => o.values_mut().for_each(|x| redact_query_in_json(x, q)),
        _ => {}
    }
}

/// Like [`parse_log_line`], also redacting query parameters in the message and all fields.
pub fn parse_log_line_with(line: &str, redact: &HashSet<String>, query_redact: Option<&HashSet<String>>) -> Option<ParsedLog> {
    let safe = safe_diagnostic(line);
    let line = safe.as_str();
    let line = line.trim_end();
    if line.trim().is_empty() {
        return None;
    }
    if let Ok(serde_json::Value::Object(mut map)) = serde_json::from_str::<serde_json::Value>(line) {
        let level = map
            .remove("level")
            .and_then(|v| v.as_str().map(normalize_level))
            .unwrap_or_else(|| "info".into());
        let mut message = map.remove("message").and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
        if let Some(q) = query_redact {
            message = redact_diagnostic_urls(&message, q);
        }
        let ts_ms = map.remove("time").and_then(|v| v.as_str().and_then(parse_rfc3339_ms));
        if let Some(h) = map.get_mut("headers") {
            redact_json_headers(h, redact);
        }
        if let Some(q) = query_redact {
            for v in map.values_mut() {
                redact_query_in_json(v, q);
            }
        }
        let fields = serde_json::Value::Object(map).to_string();
        return Some(ParsedLog { ts_ms, level, message, fields });
    }
    // Plain text (e.g. "2026-10-10T06:01:19Z INF message").
    let mut level = "info".to_string();
    let mut tagged = false;
    for (tag, l) in [(" DBG ", "debug"), (" INF ", "info"), (" WRN ", "warn"), (" ERR ", "error"), (" FTL ", "fatal")] {
        if line.contains(tag) {
            level = l.into();
            tagged = true;
            break;
        }
    }
    let lower = line.trim().to_ascii_lowercase();
    if !tagged {
        if lower.starts_with("warning:") || lower.starts_with("warn:") {
            level = "warn".into();
        } else if lower.starts_with("error:") || lower.starts_with("fatal:")
            || cli::CfError::classify(Some(1), "", line).err().is_some_and(|e| e.kind != cli::ErrorKind::Unknown)
        {
            level = "error".into();
        }
    }
    let message = match query_redact {
        Some(q) => redact_diagnostic_urls(line, q),
        None => line.to_string(),
    };
    Some(ParsedLog { ts_ms: None, level, message, fields: "{}".into() })
}

fn normalize_level(l: &str) -> String {
    match l.to_ascii_lowercase().as_str() {
        "debug" | "dbg" | "trace" => "debug",
        "warn" | "warning" | "wrn" => "warn",
        "error" | "err" => "error",
        "fatal" | "ftl" | "panic" => "fatal",
        _ => "info",
    }
    .into()
}

/// Minimal RFC 3339 parser for cloudflared's `time` field (UTC `Z` or numeric offset).
pub fn parse_rfc3339_ms(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() < 20 {
        return None;
    }
    let num = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let (y, mo, d, h, mi, se) = (num(0..4)?, num(5..7)?, num(8..10)?, num(11..13)?, num(14..16)?, num(17..19)?);
    let mut idx = 19;
    let mut frac_ms = 0i64;
    if b.get(idx) == Some(&b'.') {
        let start = idx + 1;
        let mut end = start;
        while end < b.len() && b[end].is_ascii_digit() {
            end += 1;
        }
        let digits = &s[start..end];
        let padded = format!("{:0<3}", &digits[..digits.len().min(3)]);
        frac_ms = padded.parse().ok()?;
        idx = end;
    }
    let offset_min = match b.get(idx)? {
        b'Z' | b'z' => 0,
        b'+' | b'-' => {
            let sign = if b[idx] == b'-' { -1 } else { 1 };
            let oh = num(idx + 1..idx + 3)?;
            let om = num(idx + 4..idx + 6)?;
            sign * (oh * 60 + om)
        }
        _ => return None,
    };
    // Days from civil (Howard Hinnant's algorithm).
    let y2 = if mo <= 2 { y - 1 } else { y };
    let era = if y2 >= 0 { y2 } else { y2 - 399 } / 400;
    let yoe = y2 - era * 400;
    let mp = (mo + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    let secs = days * 86400 + h * 3600 + mi * 60 + se - offset_min * 60;
    Some(secs * 1000 + frac_ms)
}

/// `Starting metrics server on 127.0.0.1:20241/metrics` -> `127.0.0.1:20241`.
pub fn metrics_addr_from_message(msg: &str) -> Option<String> {
    let rest = msg.strip_prefix("Starting metrics server on ")?;
    let addr = rest.split('/').next()?.trim();
    (!addr.is_empty()).then(|| addr.to_string())
}

/// A cloudflared process that Underlane didn't start.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ExternalProcess {
    pub pid: u32,
    pub command: String,
}

/// What identifies a tunnel on another process's command line.
#[derive(Debug, Clone, Default)]
pub struct TunnelIdentity {
    /// Config file path (matched as a substring; paths are specific).
    pub config_path: Option<String>,
    /// `tunnel:` value. UUIDs match as substrings, names only as whole arguments.
    pub tunnel: Option<String>,
    /// The file is a default config (`~/.cloudflared/config.yml` etc.), so a bare
    /// `cloudflared tunnel run` without `--config` (e.g. `cloudflared service install`) uses it.
    pub is_default_config: bool,
}

impl TunnelIdentity {
    fn matches(&self, cmd: &str) -> bool {
        let tokens: Vec<&str> = cmd.split_whitespace().collect();
        if !tokens.contains(&"tunnel") || !tokens.contains(&"run") || tokens.get(1) == Some(&"access") {
            return false;
        }
        if let Some(p) = self.config_path.as_deref().filter(|p| !p.is_empty()) {
            if cmd.contains(p) {
                return true;
            }
        }
        if let Some(t) = self.tunnel.as_deref().filter(|t| !t.is_empty()) {
            if crate::derived::looks_like_uuid(t) {
                if cmd.contains(t) {
                    return true;
                }
            } else if tokens.iter().skip(1).any(|tok| *tok == t) {
                return true;
            }
        }
        if self.is_default_config {
            let has_config = tokens.iter().any(|t| *t == "--config" || t.starts_with("--config="));
            let runs = tokens.contains(&"run") && tokens.contains(&"tunnel");
            let has_other_source = tokens.iter().any(|t| *t == "--token" || *t == "--token-file" || *t == "--url" || t.starts_with("--token=") || t.starts_with("--token-file=") || t.starts_with("--url="));
            if runs && !has_config && !has_other_source && !cmd.contains("TUNNEL_TOKEN") {
                return true;
            }
        }
        false
    }
}

/// cloudflared processes not started by Underlane (their command line doesn't mention
/// `data_marker`) that run the given tunnel.
pub fn find_external(id: &TunnelIdentity, data_marker: &str) -> Vec<ExternalProcess> {
    let processes = match crate::platform::processes() {
        Ok(p) => p,
        Err(_) => return Vec::new(),
    };
    external_from_processes(processes, id, data_marker)
}

pub fn external_from_ps(ps: &str, id: &TunnelIdentity, data_marker: &str) -> Vec<ExternalProcess> {
    external_from_processes(crate::platform::process_rows(ps), id, data_marker)
}

fn external_from_processes(processes: Vec<crate::platform::Process>, id: &TunnelIdentity, data_marker: &str) -> Vec<ExternalProcess> {
    processes.into_iter()
        .filter_map(|p| {
            let cmd = p.command.as_str();
            let exe = cmd.split_whitespace().next()?;
            let is_cloudflared = Path::new(exe).file_name().and_then(|f| f.to_str()) == Some("cloudflared");
            if !is_cloudflared || cmd.contains(data_marker) {
                return None;
            }
            if !id.matches(cmd) {
                return None;
            }
            Some(ExternalProcess { pid: p.pid, command: redact_text(cmd) })
        })
        .collect()
}

/// Pull the public URL out of the quick tunnel banner, as a fallback for `/quicktunnel`.
pub fn quick_url_from_message(msg: &str) -> Option<String> {
    msg.split(|c: char| c.is_whitespace() || c == '|')
        .find(|w| w.starts_with("https://") && w.contains(".trycloudflare.com"))
        .map(|w| w.trim_end_matches(['|', ' ']).to_string())
}
