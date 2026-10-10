//! Finding and talking to the `cloudflared` binary.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::Serialize;
use tokio::process::Command;

use crate::config::{expand_tilde, home_dir};
use crate::proxy::capture::redact_json_headers;

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

fn search_path(path_var: &str) -> Option<PathBuf> {
    std::env::split_paths(path_var).map(|d| d.join("cloudflared")).find(|p| is_executable(p))
}

/// Locate cloudflared: explicit setting, then PATH (process and login shell), then common locations.
pub async fn discover(configured: Option<&str>) -> Option<Binary> {
    let mut candidates: Vec<(PathBuf, &str)> = Vec::new();
    if let Some(c) = configured.filter(|c| !c.trim().is_empty()) {
        candidates.push((expand_tilde(c.trim()), "settings"));
    }
    if let Some(p) = std::env::var("PATH").ok().and_then(|p| search_path(&p)) {
        candidates.push((p, "path"));
    }
    if let Some(p) = login_shell_path().await.and_then(|p| search_path(&p)) {
        candidates.push((p, "path"));
    }
    for p in common_locations() {
        candidates.push((p, "common location"));
    }
    for (p, source) in candidates {
        if is_executable(&p) {
            let version = version(&p).await;
            return Some(Binary { path: p.to_string_lossy().into_owned(), version, source: source.into() });
        }
        if source == "settings" {
            // A configured path that doesn't exist shouldn't silently fall back.
            return None;
        }
    }
    None
}

/// `cloudflared version 2026.9.1 (built ...)` -> `2026.9.1`.
pub async fn version(bin: &Path) -> Option<String> {
    let out = Command::new(bin).arg("--version").stdin(Stdio::null()).output().await.ok()?;
    let text = String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    parse_version(&text)
}

pub fn parse_version(text: &str) -> Option<String> {
    let rest = text.split("version").nth(1)?;
    rest.split_whitespace().next().map(String::from)
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Validation {
    pub ok: bool,
    /// cloudflared's own message.
    pub raw: String,
    /// Plain-language explanation and fix, when we recognise the message.
    pub hint: Option<String>,
}

/// Run `cloudflared tunnel --config <file> ingress validate` from the config's directory
/// (cloudflared resolves relative paths against its working directory).
pub async fn validate(bin: &Path, config: &Path) -> Validation {
    let mut cmd = Command::new(bin);
    if let Some(dir) = config.parent().filter(|d| d.is_dir()) {
        cmd.current_dir(dir);
    }
    cmd.arg("tunnel").arg("--config").arg(config).args(["ingress", "validate"]).stdin(Stdio::null()).kill_on_drop(true);
    let out = match tokio::time::timeout(std::time::Duration::from_secs(20), cmd.output()).await {
        Ok(r) => r,
        Err(_) => {
            return Validation {
                ok: false,
                raw: "`cloudflared tunnel ingress validate` didn't finish within 20 seconds.".into(),
                hint: Some("Check that the cloudflared path in Settings points at the real binary.".into()),
            }
        }
    };
    match out {
        Ok(o) => {
            let text = format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
            interpret_validation(o.status.success(), &text)
        }
        Err(e) => Validation { ok: false, raw: e.to_string(), hint: Some("cloudflared could not be started.".into()) },
    }
}

pub fn interpret_validation(success: bool, output: &str) -> Validation {
    let raw = output
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("Validating rules from"))
        .collect::<Vec<_>>()
        .join("\n");
    let raw = raw.strip_prefix("Validation failed: ").unwrap_or(&raw).to_string();
    if success {
        return Validation { ok: true, raw, hint: None };
    }
    Validation { ok: false, hint: friendly_hint(&raw), raw }
}

/// Map known cloudflared validation errors to plain language.
pub fn friendly_hint(msg: &str) -> Option<String> {
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
    } else if m.contains("yaml") || m.contains("unmarshal") {
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
                *s = crate::proxy::capture::redact_query_in_text(s, q);
            }
        }
        serde_json::Value::Array(a) => a.iter_mut().for_each(|x| redact_query_in_json(x, q)),
        serde_json::Value::Object(o) => o.values_mut().for_each(|x| redact_query_in_json(x, q)),
        _ => {}
    }
}

/// Like [`parse_log_line`], also redacting query parameters in the message and all fields.
pub fn parse_log_line_with(line: &str, redact: &HashSet<String>, query_redact: Option<&HashSet<String>>) -> Option<ParsedLog> {
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
            message = crate::proxy::capture::redact_query_in_text(&message, q);
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
    // Untagged plain-text lines are what cloudflared prints for fatal startup problems
    // (e.g. "quick tunnel provisioning failed with status 429"); surface them as errors.
    let lower = line.to_ascii_lowercase();
    if !tagged && (lower.contains("failed") || lower.contains("error")) {
        level = "error".into();
    }
    let message = match query_redact {
        Some(q) => crate::proxy::capture::redact_query_in_text(line, q),
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

/// A cloudflared process that Burrow didn't start.
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
            let has_other_source = tokens.iter().any(|t| *t == "--token" || *t == "--url" || t.starts_with("--token="));
            if runs && !has_config && !has_other_source && !cmd.contains("TUNNEL_TOKEN") {
                return true;
            }
        }
        false
    }
}

/// cloudflared processes not started by Burrow (their command line doesn't mention
/// `data_marker`) that run the given tunnel.
pub fn find_external(id: &TunnelIdentity, data_marker: &str) -> Vec<ExternalProcess> {
    let out = match std::process::Command::new("ps").args(["-axww", "-o", "pid=,command="]).output() {
        Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(),
        Err(_) => return Vec::new(),
    };
    external_from_ps(&out, id, data_marker)
}

pub fn external_from_ps(ps: &str, id: &TunnelIdentity, data_marker: &str) -> Vec<ExternalProcess> {
    ps.lines()
        .filter_map(|l| {
            let l = l.trim_start();
            let (pid, cmd) = l.split_once(' ')?;
            let cmd = cmd.trim();
            let exe = cmd.split_whitespace().next()?;
            let is_cloudflared = Path::new(exe).file_name().and_then(|f| f.to_str()) == Some("cloudflared");
            if !is_cloudflared || cmd.contains(data_marker) {
                return None;
            }
            if !id.matches(cmd) {
                return None;
            }
            Some(ExternalProcess { pid: pid.parse().ok()?, command: cmd.to_string() })
        })
        .collect()
}

/// Pull the public URL out of the quick tunnel banner, as a fallback for `/quicktunnel`.
pub fn quick_url_from_message(msg: &str) -> Option<String> {
    msg.split(|c: char| c.is_whitespace() || c == '|')
        .find(|w| w.starts_with("https://") && w.contains(".trycloudflare.com"))
        .map(|w| w.trim_end_matches(['|', ' ']).to_string())
}
