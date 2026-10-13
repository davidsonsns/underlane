//! Bounded, cancellable one-shot cloudflared commands. Long-running tunnels stay in Supervisor.

use std::collections::HashSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use serde::de::DeserializeOwned;
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};
use tokio::sync::{oneshot, watch, Semaphore};
use tokio::time::Instant;

use crate::cloudflared::{diagnostic_text, friendly_hint, parse_log_line, safe_diagnostic, ParsedLog};
use crate::platform;
use crate::secret::{redact_text, Secret};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StdoutMode {
    Json,
    Text,
    Secret,
}

#[derive(Debug)]
pub enum Stdout {
    Json(serde_json::Value),
    Text(String),
    Secret(Secret<Vec<u8>>),
}

impl Stdout {
    /// Payload structs should use serde defaults/aliases for cloudflared's changing key casing.
    pub fn deserialize<T: DeserializeOwned>(&self) -> Result<T, CfError> {
        let result = match self {
            Self::Json(value) => serde_json::from_value(value.clone()),
            Self::Text(text) => serde_json::from_str(text),
            Self::Secret(bytes) => serde_json::from_slice(bytes.expose()),
        };
        result.map_err(|_| CfError::new(ErrorKind::InvalidJson, "cloudflared returned an unexpected JSON payload."))
    }

    pub fn text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Warning {
    OutdatedVersion(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    BinaryMissing,
    Io,
    Timeout,
    Cancelled,
    Usage,
    Config,
    Credentials,
    NotLoggedIn,
    AlreadyLoggedIn,
    Auth,
    TunnelNotFound,
    NameExists,
    ActiveConnections,
    DnsRecordExists,
    RateLimited,
    MetricsBind,
    IncompatibleOrigin,
    EdgeConnection,
    OriginConnection,
    InvalidJson,
    Unknown,
}

fn classification_text(text: &str) -> String {
    let mut scrubbed = String::new();
    let mut remaining = text;
    while let Some(start) = remaining.find(['\'', '"', '`']) {
        scrubbed.push_str(&remaining[..start]);
        let quote = remaining.as_bytes()[start] as char;
        if quote == '\'' && start > 0 && remaining.as_bytes()[start - 1].is_ascii_alphanumeric()
            && remaining.as_bytes().get(start + 1).is_some_and(u8::is_ascii_alphanumeric)
        {
            scrubbed.push(quote);
            remaining = &remaining[start + 1..];
            continue;
        }
        let tail = &remaining[start + 1..];
        let end = tail.find(quote).unwrap_or(tail.len());
        let operand = &tail[..end];
        let is_path = operand.contains('/') || operand.contains('\\')
            || [".yaml", ".yml", ".json", ".pem"].iter().any(|ext| operand.ends_with(ext));
        if !is_path { scrubbed.push_str(operand); }
        else { scrubbed.push(' '); }
        remaining = if end == tail.len() { "" } else { &tail[end + 1..] };
    }
    scrubbed.push_str(remaining);
    scrubbed.split_whitespace().filter(|word| !word.contains('/') && !word.contains('\\'))
        .collect::<Vec<_>>().join(" ").to_ascii_lowercase()
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct CfError {
    pub kind: ErrorKind,
    pub message: String,
    pub hint: Option<String>,
}

impl CfError {
    fn new(kind: ErrorKind, message: &str) -> Self {
        Self { kind, message: safe_diagnostic(message), hint: None }
    }

    /// Exit status alone is insufficient: cloudflared also reports some failures with rc 0.
    pub fn classify(code: Option<i32>, stdout: &str, stderr: &str) -> Result<Vec<Warning>, Self> {
        // JSON payloads can contain arbitrary user names and IDs; they are not diagnostics.
        let diagnostic_out = match serde_json::from_str::<serde_json::Value>(stdout) {
            Ok(serde_json::Value::Object(map)) if map.contains_key("level") => stdout,
            Ok(serde_json::Value::Object(_) | serde_json::Value::Array(_)) => "",
            _ => stdout,
        };
        let text = format!("{diagnostic_out}\n{stderr}");
        let diagnostics = diagnostic_text(&text);
        // File/URL operands may contain diagnostic-looking names; they are not anchors.
        let lower = classification_text(&diagnostics);
        let warnings = text
            .lines()
            .filter(|line| {
                let l = line.to_ascii_lowercase();
                l.contains("version") && l.contains("outdated")
            })
            .map(|line| Warning::OutdatedVersion(safe_diagnostic(line)))
            .collect::<Vec<_>>();
        let known = if lower.contains("incorrect usage") || lower.contains("flag provided but not defined") {
            Some((ErrorKind::Usage, "cloudflared rejected a command option. Check the installed binary's supported flags."))
        } else if lower.contains("existing certificate") && lower.contains("overwrite") {
            Some((ErrorKind::AlreadyLoggedIn, "A certificate already exists. cloudflared refused to overwrite it."))
        } else if code == Some(0) {
            None
        } else if lower.contains("unable to reach the origin service")
            || lower.contains("unable to connect to the origin")
        {
            Some((ErrorKind::OriginConnection, "Check that the origin service is running and that its address in the config is correct."))
        } else if lower.contains("validation failed")
            || lower.contains("cannot read config")
            || lower.contains("error loading config")
            || lower.contains("no ingress rules")
        {
            Some((ErrorKind::Config, "Fix the source config using the diagnostic below, then restart."))
        } else if lower.contains("cannot find a valid certificate")
            || lower.contains("cannot determine default origin certificate")
            || lower.contains("origin cert") && (lower.contains("not found") || lower.contains("does not exist"))
        {
            Some((ErrorKind::NotLoggedIn, "Check the configured origin certificate path and that the certificate exists."))
        } else if lower.contains("credentials")
            && (lower.contains("not found")
                || lower.contains("does not exist")
                || lower.contains("doesn't exist")
                || lower.contains("no such file")
                || lower.contains("invalid"))
        {
            Some((ErrorKind::Credentials, "Check credentials-file and the tunnel credentials, then restart."))
        } else if lower.contains("multiple-origin") && lower.contains("--url") {
            Some((ErrorKind::IncompatibleOrigin, "Use either ingress rules or --url for this tunnel."))
        } else if lower.contains("address already in use") && (lower.contains("metrics") || lower.contains("listen")) {
            Some((ErrorKind::MetricsBind, "The metrics port is already in use. Choose an available local port."))
        } else if lower.contains("tunnel")
            && (lower.contains("not found")
                || lower.contains("does not exist")
                || lower.contains("not a valid tunnel")
                || lower.contains("found 0 tunnels"))
        {
            Some((ErrorKind::TunnelNotFound, "The requested tunnel could not be found."))
        } else if lower.contains("already exists") && lower.contains("tunnel") {
            Some((ErrorKind::NameExists, "A tunnel with this name already exists."))
        } else if lower.contains("active connections") && (lower.contains("delete") || lower.contains("cannot")) {
            Some((ErrorKind::ActiveConnections, "The tunnel still has active connectors."))
        } else if lower.contains("already exists") && (lower.contains("dns") || lower.contains("cname")) {
            Some((ErrorKind::DnsRecordExists, "A DNS record already exists for this hostname."))
        } else if lower.contains("status 429") || lower.contains("status code 429") || lower.contains("rate limit") {
            Some((ErrorKind::RateLimited, "Wait before retrying; review the diagnostic to identify the service that is rate limiting requests."))
        } else if diagnostics.lines().any(|line| { let line = line.trim().to_ascii_lowercase(); line == "unauthorized" || line.starts_with("unauthorized:") })
            || lower.contains("authentication failed")
            || lower.contains("status 403")
            || lower.contains("status 401")
        {
            Some((ErrorKind::Auth, "Check the rejected credentials identified in the diagnostic, then restart."))
        } else if lower.contains("failed to connect to the edge")
            || lower.contains("failed to dial to edge")
            || lower.contains("failed to serve tunnel connection")
        {
            Some((ErrorKind::EdgeConnection, "Check the network connection and firewall, then retry. cloudflared will keep trying while this tunnel is running."))
        } else {
            None
        };
        if let Some((kind, hint)) = known {
            let message = safe_diagnostic(text.trim());
            let hint = Some(safe_diagnostic(&if kind == ErrorKind::Config {
                friendly_hint(&text).unwrap_or_else(|| hint.into())
            } else { hint.into() }));
            return Err(Self { kind, message, hint });
        }
        if code != Some(0) {
            let message =
                if text.trim().is_empty() { format!("cloudflared exited with status {code:?}.") } else { safe_diagnostic(text.trim()) };
            return Err(Self { kind: ErrorKind::Unknown, hint: friendly_hint(&message), message });
        }
        Ok(warnings)
    }
}

#[derive(Debug, Clone)]
pub struct Cancellation(watch::Sender<bool>);

impl Default for Cancellation {
    fn default() -> Self {
        Self::new()
    }
}

impl Cancellation {
    pub fn new() -> Self {
        Self(watch::channel(false).0)
    }
    pub fn cancel(&self) {
        self.0.send_replace(true);
    }
    async fn cancelled(&self) {
        let mut rx = self.0.subscribe();
        loop {
            if *rx.borrow_and_update() {
                return;
            }
            if rx.changed().await.is_err() {
                return;
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum ConfigPolicy {
    /// Only top-level commands (version/Access); tunnel commands cannot use this policy.
    None,
    Target(PathBuf),
    /// App data directory containing neutral.yml; never the user's default cloudflared config.
    Neutral(PathBuf),
}

pub struct Invocation {
    pub args: Vec<OsString>,
    pub config: ConfigPolicy,
    pub env: Vec<(String, Secret<String>)>,
    pub clear_env: bool,
    pub timeout: Duration,
    pub stdout: StdoutMode,
    pub cancel: Cancellation,
}

impl Invocation {
    pub fn new(args: impl IntoIterator<Item = impl Into<OsString>>, config: ConfigPolicy) -> Self {
        Self {
            args: args.into_iter().map(Into::into).collect(),
            config,
            env: Vec::new(),
            clear_env: false,
            timeout: Duration::from_secs(20),
            stdout: StdoutMode::Text,
            cancel: Cancellation::new(),
        }
    }
}

impl std::fmt::Debug for Invocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let args = self.args.iter().map(|a| a.to_string_lossy()).collect::<Vec<_>>().join(" ");
        f.debug_struct("Invocation")
            .field("args", &redact_text(&args))
            .field("config", &self.config)
            .field("stdout", &self.stdout)
            .field("env", &"[redacted]")
            .finish()
    }
}

#[derive(Debug)]
pub struct Outcome {
    pub stdout: Stdout,
    pub stderr: String,
    pub logs: Vec<ParsedLog>,
    pub warnings: Vec<Warning>,
}

#[derive(Clone)]
pub struct Runner {
    permits: Arc<Semaphore>,
}

impl Runner {
    pub fn new(concurrency: usize) -> Self {
        assert!(concurrency > 0);
        Self { permits: Arc::new(Semaphore::new(concurrency)) }
    }

    pub async fn exec(&self, bin: &Path, inv: Invocation) -> Result<Outcome, CfError> {
        let deadline = Instant::now() + inv.timeout;
        let permit = tokio::select! {
            biased;
            _ = inv.cancel.cancelled() => return Err(CfError::new(ErrorKind::Cancelled, "cloudflared command cancelled.")),
            _ = tokio::time::sleep_until(deadline) => return Err(CfError::new(ErrorKind::Timeout, "cloudflared command timed out.")),
            permit = self.permits.clone().acquire_owned() => permit.map_err(|_| CfError::new(ErrorKind::Cancelled, "Command runner closed."))?,
        };
        let mut cmd = command(bin, &inv)?;
        let child = cmd.spawn().map_err(|e| {
            CfError::new(if e.kind() == std::io::ErrorKind::NotFound { ErrorKind::BinaryMissing } else { ErrorKind::Io }, &e.to_string())
        })?;
        drop(cmd);
        let (result_tx, result_rx) = oneshot::channel();
        let (drop_tx, drop_rx) = oneshot::channel::<()>();
        // The task owns cleanup and the permit even if the caller's future disappears.
        tokio::spawn(async move {
            let result = run_child(child, inv, deadline, drop_rx).await;
            drop(permit);
            let _ = result_tx.send(result);
        });
        let result = result_rx.await.map_err(|_| CfError::new(ErrorKind::Io, "Command worker stopped unexpectedly."))?;
        drop(drop_tx);
        result
    }
}

pub async fn exec(bin: &Path, inv: Invocation) -> Result<Outcome, CfError> {
    static RUNNER: OnceLock<Runner> = OnceLock::new();
    RUNNER.get_or_init(|| Runner::new(2)).exec(bin, inv).await
}

fn absolute(path: &Path) -> std::io::Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.into())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

fn command(bin: &Path, inv: &Invocation) -> Result<Command, CfError> {
    let io_error = |e: std::io::Error| CfError::new(ErrorKind::Io, &e.to_string());
    let mut cmd = Command::new(absolute(bin).map_err(io_error)?);
    if inv.clear_env {
        cmd.env_clear();
    }
    match &inv.config {
        ConfigPolicy::None => {
            let is_version = inv.args.len() == 1 && inv.args[0] == "--version";
            let is_access = inv.args.first().is_some_and(|arg| arg == "access");
            if !is_version && !is_access {
                return Err(CfError::new(ErrorKind::Usage, "Only version and Access commands can omit an explicit config."));
            }
            cmd.args(&inv.args);
        }
        policy => {
            let config = match policy {
                ConfigPolicy::Target(path) => absolute(path).map_err(io_error)?,
                ConfigPolicy::Neutral(dir) => crate::config::neutral_config(&absolute(dir).map_err(io_error)?).map_err(io_error)?,
                ConfigPolicy::None => unreachable!(),
            };
            if inv.args.iter().any(|a| a == "--config" || a.to_string_lossy().starts_with("--config=")) {
                return Err(CfError::new(ErrorKind::Usage, "Config selection belongs to the invocation policy."));
            }
            cmd.arg("tunnel").arg("--config").arg(&config).args(&inv.args);
            if matches!(policy, ConfigPolicy::Target(_)) {
                if let Some(parent) = config.parent().filter(|p| p.is_dir()) {
                    cmd.current_dir(parent);
                }
            }
        }
    }
    for key in ["TUNNEL_TOKEN", "TUNNEL_TOKEN_FILE", "TUNNEL_CRED_CONTENTS", "TUNNEL_CREDENTIALS_CONTENTS"] {
        cmd.env_remove(key);
    }
    for (key, value) in &inv.env {
        cmd.env(key, value.expose());
    }
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    platform::configure_group(&mut cmd).map_err(io_error)?;
    Ok(cmd)
}

async fn read_pipe(mut pipe: impl tokio::io::AsyncRead + Unpin) -> std::io::Result<Secret<Vec<u8>>> {
    let mut bytes = Secret::new(Vec::new());
    pipe.read_to_end(bytes.expose_mut()).await?;
    Ok(bytes)
}

async fn run_child(mut child: Child, inv: Invocation, deadline: Instant, drop_rx: oneshot::Receiver<()>) -> Result<Outcome, CfError> {
    let pid = child.id().expect("spawned child has pid");
    let stdout_task = tokio::spawn(read_pipe(child.stdout.take().expect("stdout piped")));
    let stderr_task = tokio::spawn(read_pipe(child.stderr.take().expect("stderr piped")));
    let status = tokio::select! {
        biased;
        _ = drop_rx => Err(CfError::new(ErrorKind::Cancelled, "cloudflared caller dropped.")),
        _ = inv.cancel.cancelled() => Err(CfError::new(ErrorKind::Cancelled, "cloudflared command cancelled.")),
        _ = tokio::time::sleep_until(deadline) => Err(CfError::new(ErrorKind::Timeout, "cloudflared command timed out.")),
        status = child.wait() => status.map_err(|e| CfError::new(ErrorKind::Io, &e.to_string())),
    };
    // Clean descendants on every exit, including a parent that exited before its descendants.
    #[cfg(unix)]
    {
        if platform::signal_group(pid, libc::SIGTERM).is_ok() {
            tokio::time::sleep(Duration::from_millis(150)).await;
            let _ = platform::signal_group(pid, libc::SIGKILL);
        }
    }
    if status.is_err() {
        let _ = child.start_kill();
    }
    let _ = child.wait().await;
    let stdout = stdout_task
        .await
        .map_err(|e| CfError::new(ErrorKind::Io, &e.to_string()))?
        .map_err(|e| CfError::new(ErrorKind::Io, &e.to_string()))?;
    let stderr = stderr_task
        .await
        .map_err(|e| CfError::new(ErrorKind::Io, &e.to_string()))?
        .map_err(|e| CfError::new(ErrorKind::Io, &e.to_string()))?;
    let status = status?;
    let raw_out = String::from_utf8_lossy(stdout.expose());
    let sanitize = |text: &str| {
        let mut safe = safe_diagnostic(text);
        for (_, secret) in &inv.env {
            if !secret.expose().is_empty() {
                safe = safe.replace(secret.expose().as_str(), "[redacted]");
            }
        }
        safe
    };
    let mut safe_err = sanitize(&String::from_utf8_lossy(stderr.expose()));
    // Secret-mode failures never include stdout, even when it is malformed or plain text.
    if inv.stdout == StdoutMode::Secret && !raw_out.trim().is_empty() {
        safe_err = safe_err.replace(raw_out.trim(), "[redacted]");
    }
    let classifier_out = if inv.stdout == StdoutMode::Secret { "".into() } else { sanitize(&raw_out) };
    let warnings = if inv.stdout == StdoutMode::Secret {
        CfError::classify(status.code(), &raw_out, &safe_err).map_err(|mut e| {
            e.message = format!("cloudflared secret-output command failed ({:?}).", e.kind);
            e.hint = None;
            e
        })?;
        CfError::classify(Some(0), "", &safe_err)?
    } else {
        CfError::classify(status.code(), &classifier_out, &safe_err)?
    };
    let logs = safe_err.lines().filter_map(|l| parse_log_line(l, &HashSet::from(["authorization".into(), "cookie".into()]))).collect();
    let output = match inv.stdout {
        StdoutMode::Secret => Stdout::Secret(stdout),
        StdoutMode::Text => Stdout::Text(classifier_out.trim().into()),
        StdoutMode::Json => Stdout::Json(
            serde_json::from_slice(stdout.expose())
                .map_err(|_| CfError::new(ErrorKind::InvalidJson, "cloudflared returned malformed JSON."))?,
        ),
    };
    Ok(Outcome { stdout: output, stderr: safe_err, logs, warnings })
}
