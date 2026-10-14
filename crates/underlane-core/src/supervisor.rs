//! Runs cloudflared processes as children, watches their health, restarts them with backoff
//! and owns the inspecting proxies that front them.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use bytes::Bytes;
use base64::Engine;
use http_body_util::{BodyExt, Empty};
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use serde::Serialize;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;

use crate::cloudflared::{self, FailureExplanation, Validation};
use crate::config::{ConfigFingerprint, TunnelConfig};
use crate::derived::{self, Upstream};
use crate::model::{LogLine, Tunnel, TunnelKind};
use crate::platform::{self, kill_tree as terminate, sweep_group};
use crate::proxy::capture::{CapturedExchange, SharedOptions};
use crate::proxy::{self, now_ms, ProxyContext, ProxyHandle};
use crate::secret::Secret;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Stopped,
    Starting,
    Connected,
    Degraded,
    Crashed,
}

impl Status {
    /// Counts toward the menu bar number.
    pub fn is_active(self) -> bool {
        matches!(self, Status::Connected | Status::Degraded)
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RouteMapping {
    pub hostname: Option<String>,
    pub path: Option<String>,
    pub service: String,
    pub inspected: bool,
    /// Why an HTTP rule isn't inspected while inspection is on.
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TunnelState {
    pub id: String,
    pub status: Status,
    /// Human-readable explanation of the status (errors, backoff countdown, warnings).
    pub detail: Option<String>,
    /// Plain-language validation hint, when the config failed validation.
    pub hint: Option<String>,
    pub failure: Option<FailureExplanation>,
    /// The identity reserved by this supervisor run, including while it retries.
    pub cloudflare_tunnel_id: Option<String>,
    pub tunnel_conflict: Option<TunnelConflict>,
    /// Public URL for quick tunnels.
    pub public_url: Option<String>,
    /// hostname -> service, from the config (or the metrics `/config` endpoint for token tunnels).
    pub mappings: Vec<RouteMapping>,
    pub pid: Option<u32>,
    pub metrics_addr: Option<String>,
    pub connections: u32,
    pub restarts: u32,
    pub started_at: Option<i64>,
    /// Inspection requested and actually running (false after fail-open).
    pub inspecting: bool,
    /// A supervisor run exists (true even while crashed and waiting to retry), so the UI can
    /// offer Stop for a crash loop.
    pub supervised: bool,
    /// Which run produced this state; later runs supersede earlier ones.
    pub run: u64,
    /// Successful child loads within this supervisor task, including internal respawns.
    pub config_revision: u64,
    #[serde(skip)]
    pub loaded_config: Option<LoadedConfig>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TunnelConflict {
    pub tunnel_id: String,
    pub project_id: String,
    pub project_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedConfig {
    pub source_path: String,
    pub fingerprint: ConfigFingerprint,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConfigChangeStatus {
    Unchanged,
    Changed,
    Unreadable,
    NotRunning,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ConfigChange {
    pub run: u64,
    pub config_revision: u64,
    pub source_path: String,
    pub status: ConfigChangeStatus,
    pub message: Option<String>,
}

impl TunnelState {
    /// Offline source-byte check against the last successfully spawned child's config.
    pub fn check_config_change(&self, source_path: &str) -> ConfigChange {
        let mut result = ConfigChange {
            run: self.run,
            config_revision: self.config_revision,
            source_path: source_path.to_string(),
            status: ConfigChangeStatus::NotRunning,
            message: None,
        };
        if let Some(loaded) = self.loaded_config.as_ref().filter(|c| self.supervised && c.source_path == source_path) {
            match std::fs::read(source_path) {
                Ok(bytes) => {
                    result.status = if ConfigFingerprint::of(&bytes) == loaded.fingerprint {
                        ConfigChangeStatus::Unchanged
                    } else {
                        ConfigChangeStatus::Changed
                    };
                }
                Err(error) => {
                    result.status = ConfigChangeStatus::Unreadable;
                    result.message = Some(crate::secret::redact_text(&format!("Could not read the config on disk: {error}")));
                }
            }
        }
        result
    }
}

impl TunnelState {
    pub fn stopped(id: &str) -> Self {
        TunnelState {
            id: id.to_string(),
            status: Status::Stopped,
            detail: None,
            hint: None,
            failure: None,
            cloudflare_tunnel_id: None,
            tunnel_conflict: None,
            public_url: None,
            mappings: Vec::new(),
            pid: None,
            metrics_addr: None,
            connections: 0,
            restarts: 0,
            started_at: None,
            inspecting: false,
            supervised: false,
            run: 0,
            config_revision: 0,
            loaded_config: None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Event {
    State(TunnelState),
    Log(LogLine),
    Request(CapturedExchange),
}

pub type EventSink = Arc<dyn Fn(Event) + Send + Sync>;

/// What to run.
#[derive(Debug, Clone)]
pub struct RunSpec {
    pub tunnel: Tunnel,
    /// Token for remotely managed tunnels.
    pub token: Option<Secret<String>>,
}

#[derive(Clone)]
pub struct SupervisorConfig {
    pub data_dir: PathBuf,
    pub cloudflared: Arc<RwLock<Option<PathBuf>>>,
    pub capture: SharedOptions,
    /// Executable that understands `--cloudflared-shim` (Underlane itself). None = spawn directly.
    pub shim: Option<PathBuf>,
    pub backoff_base: Duration,
    pub backoff_max: Duration,
    pub max_failures: u32,
    pub health_interval: Duration,
    /// cloudflared `--grace-period`; Underlane waits a little longer before SIGKILL.
    pub grace_period: Duration,
}

impl SupervisorConfig {
    pub fn new(data_dir: PathBuf, cloudflared: Option<PathBuf>, capture: SharedOptions) -> Self {
        SupervisorConfig {
            data_dir,
            cloudflared: Arc::new(RwLock::new(cloudflared)),
            capture,
            shim: None,
            backoff_base: Duration::from_secs(1),
            backoff_max: Duration::from_secs(60),
            max_failures: 10,
            health_interval: Duration::from_secs(2),
            grace_period: Duration::from_secs(4),
        }
    }

    fn run_dir(&self) -> PathBuf {
        self.data_dir.join("run")
    }

    fn derived_dir(&self) -> PathBuf {
        self.data_dir.join("derived")
    }

    /// Minimal config for quick and token tunnels: keeps a user's default
    /// `~/.cloudflared/config.yml` from being picked up, and marks the process as Underlane's.
    fn neutral_config(&self) -> std::io::Result<PathBuf> {
        crate::config::neutral_config(&self.data_dir)
    }

    pub fn marker(&self) -> String {
        self.data_dir.to_string_lossy().into_owned()
    }
}

enum Cmd {
    Stop,
    Restart(Box<RunSpec>),
}

struct RunHandle {
    tx: mpsc::UnboundedSender<Cmd>,
    task: JoinHandle<()>,
    /// Becomes true when the run task has fully finished (process gone, pidfile cleaned).
    done: watch::Receiver<bool>,
    /// A stop was requested; the entry stays registered until the process is gone so a new
    /// start can't overlap with it.
    stopping: bool,
    run: u64,
    config_state: Arc<Mutex<TunnelState>>,
}

impl RunHandle {
    fn live(&self) -> bool {
        !self.task.is_finished()
    }
}

pub struct Supervisor {
    cfg: SupervisorConfig,
    sink: EventSink,
    runs: Mutex<HashMap<String, RunHandle>>,
    identities: Arc<Mutex<HashMap<String, IdentityOwner>>>,
    /// Captured at construction so tunnels can be started from any thread (UI callbacks,
    /// app setup), not just from inside the runtime.
    rt: tokio::runtime::Handle,
    /// Monotonic run counter; every state event carries the run it belongs to so consumers can
    /// drop late events from a run that has been replaced.
    runs_started: AtomicU64,
}

impl Supervisor {
    /// Must be called from within a Tokio runtime.
    pub fn new(cfg: SupervisorConfig, sink: EventSink) -> Arc<Self> {
        Arc::new(Supervisor {
            cfg,
            sink,
            runs: Mutex::new(HashMap::new()),
            identities: Arc::new(Mutex::new(HashMap::new())),
            rt: tokio::runtime::Handle::current(),
            runs_started: AtomicU64::new(0),
        })
    }

    pub fn config(&self) -> &SupervisorConfig {
        &self.cfg
    }

    /// A run task exists for this tunnel (including crashed-and-retrying and stopping).
    pub fn is_running(&self, id: &str) -> bool {
        self.runs.lock().unwrap().get(id).map(RunHandle::live).unwrap_or(false)
    }

    pub fn running_ids(&self) -> Vec<String> {
        self.runs.lock().unwrap().iter().filter(|(_, r)| r.live()).map(|(id, _)| id.clone()).collect()
    }

    /// Authoritative state for comparisons, before the application's queued state events.
    pub fn config_state(&self, id: &str) -> Option<TunnelState> {
        let runs = self.runs.lock().unwrap();
        let handle = runs.get(id).filter(|h| h.live() && !h.stopping)?;
        let state = handle.config_state.lock().unwrap().clone();
        Some(state)
    }

    fn spawn_run(&self, runs: &mut HashMap<String, RunHandle>, spec: RunSpec) {
        let run_no = self.runs_started.fetch_add(1, Ordering::SeqCst) + 1;
        let (tx, rx) = mpsc::unbounded_channel();
        let (done_tx, done_rx) = watch::channel(false);
        let id = spec.tunnel.id.clone();
        let mut initial = TunnelState::stopped(&id);
        initial.run = run_no;
        initial.supervised = true;
        let config_state = Arc::new(Mutex::new(initial));
        let ownership = TunnelOwnership {
            identities: self.identities.clone(),
            owner: IdentityOwner { project_id: id.clone(), project_name: spec.tunnel.name.clone(), run: run_no },
            identity: None,
        };
        let fut = run(spec, self.cfg.clone(), self.sink.clone(), rx, run_no, config_state.clone(), ownership);
        let task = self.rt.spawn(async move {
            fut.await;
            let _ = done_tx.send(true);
        });
        runs.insert(id, RunHandle { tx, task, done: done_rx, stopping: false, run: run_no, config_state });
    }

    /// Start once the current run (if it is stopping) has fully exited.
    fn start_after(self: &Arc<Self>, mut done: watch::Receiver<bool>, spec: RunSpec, restart: bool) {
        let me = self.clone();
        self.rt.spawn(async move {
            let _ = done.wait_for(|d| *d).await;
            if restart {
                me.restart(spec);
            } else {
                me.start(spec);
            }
        });
    }

    /// Start a tunnel. A no-op if it is already running; deferred if it is still stopping.
    pub fn start(self: &Arc<Self>, spec: RunSpec) {
        let mut runs = self.runs.lock().unwrap();
        if let Some(h) = runs.get(&spec.tunnel.id).filter(|h| h.live()) {
            if h.stopping {
                let done = h.done.clone();
                drop(runs);
                self.start_after(done, spec, false);
            }
            return;
        }
        self.spawn_run(&mut runs, spec);
    }

    /// Restart with a (possibly updated) spec; starts it if it isn't running.
    pub fn restart(self: &Arc<Self>, spec: RunSpec) {
        let mut runs = self.runs.lock().unwrap();
        if let Some(h) = runs.get(&spec.tunnel.id).filter(|h| h.live()) {
            if h.stopping {
                let done = h.done.clone();
                drop(runs);
                self.start_after(done, spec, true);
            } else {
                let _ = h.tx.send(Cmd::Restart(Box::new(spec)));
            }
            return;
        }
        self.spawn_run(&mut runs, spec);
    }

    /// Apply an edit only to an existing run; a concurrent user Stop always wins.
    pub fn restart_if_running(&self, spec: RunSpec) -> bool {
        let runs = self.runs.lock().unwrap();
        let Some(handle) = runs.get(&spec.tunnel.id).filter(|h| h.live() && !h.stopping) else { return false };
        handle.tx.send(Cmd::Restart(Box::new(spec))).is_ok()
    }

    /// Stop a tunnel and wait until its process has exited.
    pub async fn stop(&self, id: &str) {
        let waiting = {
            let mut runs = self.runs.lock().unwrap();
            match runs.get_mut(id) {
                Some(h) if h.live() => {
                    h.stopping = true;
                    let _ = h.tx.send(Cmd::Stop);
                    Some((h.done.clone(), h.run))
                }
                _ => {
                    runs.remove(id);
                    None
                }
            }
        };
        match waiting {
            Some((mut done, run_no)) => {
                let _ = done.wait_for(|d| *d).await;
                let mut runs = self.runs.lock().unwrap();
                if runs.get(id).map(|h| h.run == run_no).unwrap_or(false) {
                    runs.remove(id);
                }
            }
            None => {
                let mut s = TunnelState::stopped(id);
                s.run = self.runs_started.load(Ordering::SeqCst);
                (self.sink)(Event::State(s));
            }
        }
    }

    pub async fn stop_all(self: &Arc<Self>) {
        let ids = self.running_ids();
        let tasks: Vec<_> = ids
            .into_iter()
            .map(|id| {
                let me = self.clone();
                self.rt.spawn(async move { me.stop(&id).await })
            })
            .collect();
        for t in tasks {
            let _ = t.await;
        }
    }
}

// ---------------------------------------------------------------------------------------------

struct Prepared {
    args: Vec<String>,
    env: Vec<(String, Secret<String>)>,
    cwd: Option<PathBuf>,
    mappings: Vec<RouteMapping>,
    inspecting: bool,
    warning: Option<String>,
    loaded_config: Option<LoadedConfig>,
}

/// Won't get better by retrying (bad config, missing binary, already running elsewhere...).
struct Fatal {
    failure: FailureExplanation,
    conflict: Option<Box<TunnelConflict>>,
}

fn fatal(detail: impl Into<String>) -> Fatal {
    let raw = detail.into();
    Fatal { failure: explain_diagnostic(&raw), conflict: None }
}

fn observed_failure(cause: &str, action: &str, raw: &str) -> Fatal {
    Fatal { failure: FailureExplanation::new(cause, Some(action), raw), conflict: None }
}

struct IdentityOwner {
    project_id: String,
    project_name: String,
    run: u64,
}

struct TunnelOwnership {
    identities: Arc<Mutex<HashMap<String, IdentityOwner>>>,
    owner: IdentityOwner,
    identity: Option<String>,
}

impl TunnelOwnership {
    /// Called only before a child starts, after the previous child's process group is gone.
    fn claim(&mut self, identity: Option<String>) -> Result<(), Fatal> {
        let mut owners = self.identities.lock().unwrap();
        if let Some(id) = &identity {
            if let Some(owner) = owners.get(id).filter(|owner| owner.run != self.owner.run) {
                let conflict = TunnelConflict {
                    tunnel_id: id.clone(), project_id: owner.project_id.clone(), project_name: owner.project_name.clone(),
                };
                let cause = format!("Cannot start while {} is running.", crate::secret::redact_text(&owner.project_name));
                let action = "These projects use the same Cloudflare tunnel ID. Stop the running project, or create a separate tunnel for this project.";
                return Err(Fatal { failure: FailureExplanation::new(&cause, Some(action), &cause), conflict: Some(Box::new(conflict)) });
            }
        }
        if self.identity != identity {
            self.release(&mut owners);
        }
        if let Some(id) = &identity {
            owners.insert(id.clone(), IdentityOwner {
                project_id: self.owner.project_id.clone(), project_name: self.owner.project_name.clone(), run: self.owner.run,
            });
        }
        self.identity = identity;
        Ok(())
    }

    fn release(&self, owners: &mut HashMap<String, IdentityOwner>) {
        if let Some(id) = self.identity.as_ref().filter(|id| owners.get(*id).is_some_and(|owner| owner.run == self.owner.run)) {
            owners.remove(id);
        }
    }
}

impl Drop for TunnelOwnership {
    fn drop(&mut self) {
        self.release(&mut self.identities.lock().unwrap());
    }
}

fn non_nil_uuid(value: &str) -> Option<String> {
    uuid::Uuid::parse_str(value.trim()).ok().filter(|id| !id.is_nil()).map(|id| id.to_string())
}

/// Cloudflared tokens are standard-base64 JSON; only their nonsecret tunnel UUID escapes.
pub fn token_tunnel_identity(token: &str) -> Result<String, String> {
    #[derive(serde::Deserialize)]
    struct Token<'a> { #[serde(borrow)] t: &'a str }
    let invalid = || "The tunnel token has no valid tunnel UUID. Copy a fresh token from Cloudflare.".to_string();
    let bytes = Secret::new(base64::engine::general_purpose::STANDARD.decode(token.trim()).map_err(|_| invalid())?);
    let value: Token<'_> = serde_json::from_slice(bytes.expose()).map_err(|_| invalid())?;
    non_nil_uuid(value.t).ok_or_else(invalid)
}

struct ConfigIdentity {
    id: Option<String>,
    token: Option<Secret<String>>,
}

/// Resolve source authentication without Cloudflare access or returning any credentials.
/// Names need explicit credentials; unresolved names never silently bypass duplicate protection.
pub fn config_tunnel_identity(config: &TunnelConfig) -> Result<Option<String>, String> {
    Ok(resolve_config_identity(config)?.id)
}

fn resolve_config_identity(config: &TunnelConfig) -> Result<ConfigIdentity, String> {
    let map = config.raw.as_mapping().expect("parsed config mapping");
    let field = |key: &str| map.get(key).and_then(serde_norway::Value::as_str).filter(|value| !value.trim().is_empty());
    let token = if let Some(token) = field("token") {
        Some(Secret::new(token.trim().to_string()))
    } else if let Some(path) = field("token-file") {
        let path = crate::config::resolve_path(config.source.parent().unwrap_or(Path::new(".")), path);
        Some(Secret::new(std::fs::read_to_string(path).map_err(|_| "The token file cannot be read. Check token-file in the source config.".to_string())?))
    } else { None };
    if let Some(token) = token {
        let token = Secret::new(token.expose().trim().to_string());
        return Ok(ConfigIdentity { id: Some(token_tunnel_identity(token.expose())?), token: Some(token) });
    }

    let configured = config.tunnel.as_deref().filter(|id| !id.trim().is_empty());
    if configured.and_then(|id| uuid::Uuid::parse_str(id.trim()).ok()).is_some_and(|id| id.is_nil()) {
        return Err("The config contains an empty tunnel UUID. Set tunnel to a valid Cloudflare UUID.".into());
    }
    let uuid = configured.and_then(non_nil_uuid);
    let credentials = if let Some(contents) = field("credentials-contents") {
        Some(Secret::new(contents.as_bytes().to_vec()))
    } else if let Some(path) = config.credentials_file.clone().or_else(|| field("cred-file").map(|path| crate::config::resolve_path(config.source.parent().unwrap_or(Path::new(".")), path))) {
        Some(Secret::new(std::fs::read(path).map_err(|_| "The credentials file cannot be read. Check credentials-file in the source config.".to_string())?))
    } else { None };
    let credential_id = if let Some(bytes) = credentials {
        #[derive(serde::Deserialize)]
        struct Credentials<'a> { #[serde(rename = "TunnelID", borrow)] id: Option<&'a str> }
        let value: Credentials<'_> = serde_json::from_slice(bytes.expose()).map_err(|_| "The tunnel credentials are not valid JSON.".to_string())?;
        value.id.map(|id| non_nil_uuid(id).ok_or_else(|| "The credentials contain an invalid TunnelID.".to_string())).transpose()?
    } else { None };
    if let Some(uuid) = uuid {
        if credential_id.as_ref().is_some_and(|id| *id != uuid) {
            return Err("The credentials file belongs to a different tunnel than the config UUID. Check credentials-file and tunnel in the source config.".into());
        }
        return Ok(ConfigIdentity { id: Some(uuid), token: None });
    }
    if configured.is_some() {
        let id = credential_id.ok_or_else(|| "The tunnel name cannot be resolved safely. Set tunnel to its Cloudflare UUID, or select credentials containing TunnelID.".to_string())?;
        return Ok(ConfigIdentity { id: Some(id), token: None });
    }
    if config.is_single_origin() {
        return Ok(ConfigIdentity { id: None, token: None });
    }
    Err("The config has no tunnel UUID. Set up this project's tunnel before starting.".into())
}

fn explain_diagnostic(diagnostics: &str) -> FailureExplanation {
    let error = crate::cli::CfError::classify(Some(1), "", diagnostics).expect_err("failure diagnostic");
    FailureExplanation::from_error(&error)
}

fn explain_exit(code: Option<i32>, diagnostics: &str) -> FailureExplanation {
    let mut failure = match crate::cli::CfError::classify(code, "", diagnostics) {
        Err(error) => FailureExplanation::from_error(&error),
        Ok(_) => FailureExplanation::new("cloudflared stopped unexpectedly.", Some("Review the diagnostic and logs, then try again."), diagnostics),
    };
    let exit_status = code.map(|code| code.to_string()).unwrap_or_else(|| "not reported".into());
    failure.raw = cloudflared::safe_diagnostic(&format!("cloudflared exit status: {exit_status}\n{}", failure.raw));
    failure
}

/// Ensure a running proxy exists for each upstream, reusing ports across restarts.
async fn ensure_proxies(
    upstreams: &[Upstream],
    proxies: &mut HashMap<usize, ProxyHandle>,
    ctx: &ProxyContext,
) -> anyhow::Result<HashMap<usize, u16>> {
    proxies.retain(|idx, h| upstreams.iter().any(|u| u.rule_index == *idx && *u == h.upstream));
    let mut ports = HashMap::new();
    for up in upstreams {
        let reuse = proxies.get(&up.rule_index).filter(|h| h.is_running()).map(|h| h.port());
        let port = match reuse {
            Some(p) => p,
            None => {
                let previous = proxies.remove(&up.rule_index).map(|h| h.port()).unwrap_or(0);
                let handle = match proxy::start(previous, up.clone(), ctx.clone()).await {
                    Ok(h) => h,
                    Err(_) if previous != 0 => proxy::start(0, up.clone(), ctx.clone()).await?,
                    Err(e) => return Err(e),
                };
                let p = handle.port();
                proxies.insert(up.rule_index, handle);
                p
            }
        };
        ports.insert(up.rule_index, port);
    }
    Ok(ports)
}

fn base_args(t: &Tunnel, config: &Path, grace: Duration) -> Vec<String> {
    vec![
        "tunnel".into(),
        "--config".into(),
        config.to_string_lossy().into_owned(),
        "--no-autoupdate".into(),
        "--metrics".into(),
        "127.0.0.1:0".into(),
        "--grace-period".into(),
        format!("{}s", grace.as_secs().max(1)),
        "--loglevel".into(),
        t.loglevel.clone(),
        "--output".into(),
        "json".into(),
    ]
}

async fn prepare(
    spec: &RunSpec,
    cfg: &SupervisorConfig,
    bin: &Path,
    proxies: &mut HashMap<usize, ProxyHandle>,
    ctx: &ProxyContext,
    force_no_inspect: bool,
    ownership: &mut TunnelOwnership,
) -> Result<Prepared, Fatal> {
    let t = &spec.tunnel;
    let want_inspect = t.inspect && t.kind.supports_inspection() && !force_no_inspect;

    match t.kind {
        TunnelKind::Config => {
            let path = PathBuf::from(t.config_path.as_deref().ok_or_else(|| observed_failure("No config file is set for this tunnel.", "Select a config file in the tunnel settings, then restart.", "No config file is set for this tunnel."))?);
            let config = TunnelConfig::load(&path).map_err(|e| {
                let action = match &e {
                    crate::config::ConfigError::Io { .. } => "Check that the imported config file exists and is readable, then restart.",
                    crate::config::ConfigError::Yaml { .. } => "Check YAML indentation and syntax in the source file, then restart.",
                    crate::config::ConfigError::Invalid(_) => "Fix the source config using the diagnostic below, then restart.",
                };
                observed_failure("The config could not be read or parsed.", action, &e.to_string())
            })?;

            if crate::projects::needs_setup(&config) {
                return Err(observed_failure(crate::projects::DRAFT_DETAIL, crate::projects::DRAFT_HINT,
                    "The source config has no tunnel identity. Add tunnel and credentials-file in your editor."));
            }

            let resolved = resolve_config_identity(&config).map_err(|message| observed_failure(
                "The Cloudflare tunnel identity could not be resolved.",
                &message, &message,
            ))?;
            ownership.claim(resolved.id.clone())?;

            let home_cf = crate::config::home_dir().join(".cloudflared");
            let identity = cloudflared::TunnelIdentity {
                config_path: Some(path.to_string_lossy().into_owned()),
                tunnel: config.tunnel.clone(),
                is_default_config: path.parent() == Some(home_cf.as_path())
                    && matches!(path.file_name().and_then(|f| f.to_str()), Some("config.yml" | "config.yaml")),
            };
            if let Some(ext) = cloudflared::find_external(&identity, &cfg.marker()).first() {
                return Err(observed_failure(
                    "This tunnel is already running outside Underlane.",
                    "Stop the other connector first, then start the tunnel here.",
                    &format!("This tunnel is already running outside Underlane (pid {}).\n{}", ext.pid, ext.command),
                ));
            }

            if config.ingress.is_empty() {
                return Err(fatal("The config has no ingress rules and no `url:`, so there is nothing to route."));
            }
            if !config.is_single_origin() {
                let Validation { ok, raw, failure, .. } = cloudflared::validate(bin, &path).await;
                if !ok {
                    return Err(Fatal { failure: failure.unwrap_or_else(|| explain_exit(Some(1), &raw)), conflict: None });
                }
            }
            if resolved.token.is_none() {
                derived::preflight_credentials(&config).map_err(|raw| observed_failure("The tunnel credentials are missing or do not match the config.", "Check credentials-file and the tunnel ID in the source config, then restart.", &raw))?;
            }

            let mut mappings: Vec<RouteMapping> = config
                .ingress
                .iter()
                .map(|r| RouteMapping {
                    hostname: r.hostname.clone(),
                    path: r.path.clone(),
                    service: r.service.clone(),
                    inspected: false,
                    note: None,
                })
                .collect();
            let mut ports = HashMap::new();
            let mut inspecting = false;
            let mut warning = None;
            if want_inspect {
                let plan = derived::plan(&config);
                for (idx, why) in &plan.skipped {
                    if let Some(m) = mappings.get_mut(*idx) {
                        m.note = Some(why.clone());
                    }
                }
                if plan.upstreams.is_empty() {
                    warning = Some("Inspection is on, but no rule can be inspected.".into());
                    proxies.clear();
                } else {
                    match ensure_proxies(&plan.upstreams, proxies, ctx).await {
                        Ok(p) => {
                            for idx in p.keys() {
                                if let Some(m) = mappings.get_mut(*idx) {
                                    m.inspected = true;
                                }
                            }
                            ports = p;
                            inspecting = true;
                        }
                        Err(e) => {
                            proxies.clear();
                            warning = Some(format!(
                                "Inspection is off for this run: the proxy could not start ({e}). Traffic goes straight to the origin."
                            ));
                        }
                    }
                }
            } else {
                proxies.clear();
            }
            // Always run a derived copy from the data dir, even without inspection, so every
            // process Underlane starts is identifiable as its own.
            let mut doc = derived::derive(&config, &ports);
            if resolved.token.is_some() {
                let map = doc.as_mapping_mut().expect("derived config mapping");
                map.remove("token");
                map.remove("token-file");
            }
            let out = cfg.derived_dir().join(format!("{}.yml", t.id));
            derived::write_derived(&doc, &path, &out).map_err(|e| fatal(format!("Could not write the derived config: {e}")))?;
            let mut args = base_args(t, &out, cfg.grace_period);
            args.push("run".into());
            if let Some(id) = resolved.id { args.push(id); }
            Ok(Prepared {
                args,
                // Snapshot token-file authentication so an external edit cannot change the
                // identity between reservation and cloudflared's file read.
                env: resolved.token.map(|token| vec![("TUNNEL_TOKEN".into(), token)]).unwrap_or_default(),
                cwd: path.parent().filter(|d| d.is_dir()).map(Path::to_path_buf),
                mappings,
                inspecting,
                warning,
                loaded_config: Some(LoadedConfig { source_path: path.to_string_lossy().into_owned(), fingerprint: config.fingerprint }),
            })
        }
        TunnelKind::Quick => {
            ownership.claim(None)?;
            let origin = normalize_origin(t.origin_url.as_deref().ok_or_else(|| observed_failure("No origin URL is set for this quick tunnel.", "Set the origin URL in the tunnel settings, then restart.", "No local URL is set for this quick tunnel."))?);
            if derived::split_service(&origin).is_none() {
                return Err(observed_failure("The origin URL is invalid.", "Use a complete HTTP or HTTPS origin URL, then restart.", &format!("`{origin}` isn't an http:// or https:// URL.")));
            }
            let neutral = cfg.neutral_config().map_err(|e| fatal(e.to_string()))?;
            let mut url = origin.clone();
            let mut inspecting = false;
            let mut warning = None;
            if want_inspect {
                let up = Upstream::from_service(0, None, &origin).expect("validated above");
                match ensure_proxies(&[up], proxies, ctx).await {
                    Ok(ports) => {
                        url = format!("http://127.0.0.1:{}", ports[&0]);
                        inspecting = true;
                    }
                    Err(e) => {
                        proxies.clear();
                        warning = Some(format!("Inspection is off for this run: the proxy could not start ({e})."));
                    }
                }
            } else {
                proxies.clear();
            }
            let mut args = base_args(t, &neutral, cfg.grace_period);
            args.push("--url".into());
            args.push(url);
            let mappings = vec![RouteMapping { hostname: None, path: None, service: origin, inspected: inspecting, note: None }];
            Ok(Prepared { args, env: vec![], cwd: None, mappings, inspecting, warning, loaded_config: None })
        }
        TunnelKind::Token => {
            let token = spec
                .token
                .as_ref()
                .filter(|t| !t.expose().trim().is_empty())
                .ok_or_else(|| observed_failure("No token is stored for this tunnel.", "Paste the token again in the tunnel settings, then restart.", "No token is stored for this tunnel."))?;
            let identity = token_tunnel_identity(token.expose()).map_err(|message| observed_failure(
                "The tunnel token could not be read.", "Copy a fresh token from Cloudflare, then try again.", &message,
            ))?;
            ownership.claim(Some(identity))?;
            let neutral = cfg.neutral_config().map_err(|e| fatal(e.to_string()))?;
            let mut args = base_args(t, &neutral, cfg.grace_period);
            args.push("run".into());
            let warning = t
                .inspect
                .then(|| "Inspection isn't available for token tunnels: their routes live in the Cloudflare dashboard.".to_string());
            Ok(Prepared {
                args,
                env: vec![("TUNNEL_TOKEN".into(), Secret::new(token.expose().trim().to_string()))],
                cwd: None,
                mappings: vec![],
                inspecting: false,
                warning,
                loaded_config: None,
            })
        }
    }
}

/// Accept `3000`, `localhost:3000` and full URLs for quick tunnels.
pub fn normalize_origin(s: &str) -> String {
    let s = s.trim();
    if !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()) {
        return format!("http://localhost:{s}");
    }
    if s.contains("://") {
        s.to_string()
    } else {
        format!("http://{s}")
    }
}

fn spawn_child(bin: &Path, p: &Prepared, shim: Option<&Path>) -> std::io::Result<Child> {
    let mut cmd = match shim {
        Some(shim) => {
            let mut c = Command::new(shim);
            c.arg(crate::shim::SHIM_FLAG).arg(bin).stdin(Stdio::piped());
            c
        }
        None => {
            let mut c = Command::new(bin);
            c.stdin(Stdio::null());
            c
        }
    };
    cmd.args(&p.args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_remove("TUNNEL_TOKEN")
        .env_remove("TUNNEL_TOKEN_FILE")
        .env_remove("TUNNEL_CRED_FILE")
        .env_remove("TUNNEL_CRED_CONTENTS")
        .env_remove("TUNNEL_CREDENTIALS_CONTENTS");
    if let Some(dir) = &p.cwd {
        cmd.current_dir(dir);
    }
    for (k, v) in &p.env {
        cmd.env(k, v.expose());
    }
    platform::configure_group(&mut cmd)?;
    cmd.spawn()
}

fn pidfile(cfg: &SupervisorConfig, id: &str) -> PathBuf {
    cfg.run_dir().join(format!("{id}.pid"))
}

/// Kill cloudflared processes left behind by a previous Underlane that died without its shim
/// getting a chance (e.g. older builds or SIGKILL of the whole tree). A pid is only touched
/// if its command line still references Underlane's data directory.
pub fn reap_orphans(data_dir: &Path) -> Vec<u32> {
    platform::reap_orphans(data_dir)
}

type HttpClient = Client<hyper_util::client::legacy::connect::HttpConnector, Empty<Bytes>>;

async fn get_json(client: &HttpClient, url: &str) -> Option<(u16, serde_json::Value)> {
    let fut = async {
        let resp = client.get(url.parse().ok()?).await.ok()?;
        let status = resp.status().as_u16();
        let body = resp.into_body().collect().await.ok()?.to_bytes();
        let json = serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
        Some((status, json))
    };
    tokio::time::timeout(Duration::from_millis(1500), fut).await.ok().flatten()
}

fn mappings_from_remote_config(v: &serde_json::Value) -> Vec<RouteMapping> {
    v.pointer("/config/ingress")
        .and_then(|i| i.as_array())
        .map(|rules| {
            rules
                .iter()
                .map(|r| RouteMapping {
                    hostname: r.get("hostname").and_then(|h| h.as_str()).filter(|h| !h.is_empty()).map(String::from),
                    path: r.get("path").and_then(|p| p.as_str()).filter(|p| !p.is_empty()).map(String::from),
                    service: r.get("service").and_then(|s| s.as_str()).unwrap_or("").to_string(),
                    inspected: false,
                    note: None,
                })
                .collect()
        })
        .unwrap_or_default()
}

enum RunOutcome {
    Stopped,
    Restart(Box<RunSpec>),
    Exited { code: Option<i32>, ran_for: Duration, diagnostics: String },
    FailOpen,
}

/// Per-run log handling: parse, redact, track errors and the metrics address.
struct LineCtx<'a> {
    id: &'a str,
    cfg: &'a SupervisorConfig,
    sink: &'a EventSink,
}

struct LineInfo {
    level: String,
    message: String,
    raw: String,
}

impl LineCtx<'_> {
    fn handle(&self, line: &str) -> Option<LineInfo> {
        let redactor = self.cfg.capture.read().unwrap().redactor();
        let p = cloudflared::parse_log_line_with(line, &redactor.headers, Some(&redactor.query))?;
        let mut raw = serde_json::from_str::<serde_json::Value>(&p.fields).unwrap_or_else(|_| serde_json::json!({}));
        raw["level"] = p.level.clone().into();
        raw["message"] = p.message.clone().into();
        let safe_raw = if matches!(serde_json::from_str::<serde_json::Value>(line), Ok(serde_json::Value::Object(_))) { raw.to_string() } else { p.message.clone() };
        let info = LineInfo { level: p.level.clone(), message: p.message.clone(), raw: safe_raw };
        (self.sink)(Event::Log(LogLine {
            id: 0,
            tunnel_id: self.id.to_string(),
            ts: p.ts_ms.unwrap_or_else(now_ms),
            level: p.level,
            message: p.message,
            fields: p.fields,
        }));
        Some(info)
    }
}

/// Kills a whole process group if the run task unwinds (panic) while a child is alive, so a
/// cloudflared behind a dead shim can't be orphaned.
struct GroupGuard(Option<u32>);

impl Drop for GroupGuard {
    fn drop(&mut self) {
        if let Some(pgid) = self.0 {
            let _ = platform::signal_group(pgid, platform::SIGKILL);
        }
    }
}

/// Remove the pidfile only if it still names this run's process.
fn remove_own_pidfile(cfg: &SupervisorConfig, id: &str, pid: Option<u32>) {
    let path = pidfile(cfg, id);
    let ours = match (std::fs::read_to_string(&path), pid) {
        (Ok(content), Some(pid)) => content.trim() == pid.to_string(),
        _ => false,
    };
    if ours {
        let _ = std::fs::remove_file(path);
    }
}

async fn run(mut spec: RunSpec, cfg: SupervisorConfig, sink: EventSink, mut rx: mpsc::UnboundedReceiver<Cmd>, run_no: u64, config_state: Arc<Mutex<TunnelState>>, mut ownership: TunnelOwnership) {
    let id = spec.tunnel.id.clone();
    let mut state = TunnelState::stopped(&id);
    state.run = run_no;
    state.supervised = true;
    let emit_state = |s: &TunnelState| {
        *config_state.lock().unwrap() = s.clone();
        sink(Event::State(s.clone()));
    };
    let mut proxies: HashMap<usize, ProxyHandle> = HashMap::new();
    let req_sink = sink.clone();
    let ctx = ProxyContext {
        tunnel_id: id.clone(),
        options: cfg.capture.clone(),
        sink: Arc::new(move |x| req_sink(Event::Request(x))),
    };
    let lines_ctx = LineCtx { id: &id, cfg: &cfg, sink: &sink };
    let http: HttpClient = Client::builder(TokioExecutor::new()).build_http();
    let mut failures: u32 = 0;
    let mut force_no_inspect = false;
    let _ = std::fs::create_dir_all(cfg.run_dir());

    'outer: loop {
        ownership.owner.project_name = spec.tunnel.name.clone();
        state.status = Status::Starting;
        state.detail = None;
        state.hint = None;
        state.failure = None;
        state.tunnel_conflict = None;
        state.public_url = None;
        state.connections = 0;
        state.pid = None;
        state.metrics_addr = None;
        emit_state(&state);

        let bin = cfg.cloudflared.read().unwrap().clone();
        let Some(bin) = bin else {
            state.status = Status::Crashed;
            state.detail = Some("cloudflared wasn't found. Install it with `brew install cloudflared` or set its path in Settings.".into());
            state.failure = Some(FailureExplanation::new("cloudflared could not be found.", Some("Install cloudflared or set its executable path in Settings, then restart."), state.detail.as_deref().unwrap()));
            state.detail = state.failure.as_ref().map(|f| f.cause.clone());
            state.hint = state.failure.as_ref().and_then(|f| f.action.clone());
            emit_state(&state);
            match rx.recv().await {
                Some(Cmd::Restart(s)) => {
                    spec = *s;
                    continue 'outer;
                }
                _ => break 'outer,
            }
        };

        // Preparing runs `ingress validate`; stay responsive to Stop while it does.
        let prepared = tokio::select! {
            p = prepare(&spec, &cfg, &bin, &mut proxies, &ctx, force_no_inspect, &mut ownership) => p,
            cmd = rx.recv() => match cmd {
                Some(Cmd::Restart(s)) => {
                    spec = *s;
                    failures = 0;
                    continue 'outer;
                }
                _ => break 'outer,
            },
        };
        state.cloudflare_tunnel_id = ownership.identity.clone();
        let prepared = match prepared {
            Ok(p) => p,
            Err(Fatal { failure, conflict: Some(conflict) }) => {
                drop(proxies);
                drop(ownership);
                let mut blocked = TunnelState::stopped(&id);
                blocked.run = run_no;
                blocked.config_revision = state.config_revision;
                blocked.restarts = state.restarts;
                blocked.detail = Some(failure.cause.clone());
                blocked.hint = failure.action.clone();
                blocked.failure = Some(failure);
                blocked.tunnel_conflict = Some(*conflict);
                blocked.mappings = state.mappings;
                emit_state(&blocked);
                return;
            }
            Err(Fatal { failure, conflict: None }) => {
                state.status = Status::Crashed;
                state.detail = Some(failure.cause.clone());
                state.hint = failure.action.clone();
                state.failure = Some(failure);
                emit_state(&state);
                // Wait for the user to fix things and restart, or stop.
                match rx.recv().await {
                    Some(Cmd::Restart(s)) => {
                        spec = *s;
                        failures = 0;
                        continue 'outer;
                    }
                    _ => break 'outer,
                }
            }
        };
        let mut child = match spawn_child(&bin, &prepared, cfg.shim.as_deref()) {
            Ok(c) => c,
            Err(e) => {
                state.status = Status::Crashed;
                let failure = FailureExplanation::new("cloudflared could not start.", Some("Check the executable path and file permissions in Settings, then restart."), &format!("Could not start cloudflared: {e}"));
                state.detail = Some(failure.cause.clone());
                state.hint = failure.action.clone();
                state.failure = Some(failure);
                emit_state(&state);
                match rx.recv().await {
                    Some(Cmd::Restart(s)) => {
                        spec = *s;
                        continue 'outer;
                    }
                    _ => break 'outer,
                }
            }
        };
        state.mappings = prepared.mappings.clone();
        state.inspecting = prepared.inspecting;
        state.config_revision += 1;
        state.loaded_config = prepared.loaded_config.clone();
        // Holding the shim's stdin open is what keeps it from killing cloudflared.
        let _shim_stdin: Option<ChildStdin> = child.stdin.take();
        let child_pid = child.id();
        let mut group_guard = GroupGuard(child_pid);
        let started = Instant::now();
        state.pid = child.id();
        state.started_at = Some(now_ms());
        state.detail = prepared.warning.clone();
        if let Some(pid) = state.pid {
            let _ = std::fs::write(pidfile(&cfg, &id), pid.to_string());
        }
        emit_state(&state);

        // Merge stdout and stderr into one line stream.
        let (line_tx, mut line_rx) = mpsc::unbounded_channel::<String>();
        for reader in [
            child.stdout.take().map(|s| Box::new(s) as Box<dyn tokio::io::AsyncRead + Unpin + Send>),
            child.stderr.take().map(|s| Box::new(s) as Box<dyn tokio::io::AsyncRead + Unpin + Send>),
        ]
        .into_iter()
        .flatten()
        {
            let tx = line_tx.clone();
            let secrets: Vec<_> = prepared.env.iter().map(|(_, secret)| secret.clone()).collect();
            tokio::spawn(async move {
                let mut lines = BufReader::new(reader).lines();
                while let Ok(Some(l)) = lines.next_line().await {
                    let raw = Secret::new(l);
                    let mut safe = cloudflared::safe_diagnostic(raw.expose());
                    for secret in &secrets {
                        if !secret.expose().is_empty() { safe = safe.replace(secret.expose().as_str(), "[redacted]"); }
                    }
                    if tx.send(safe).is_err() {
                        break;
                    }
                }
            });
        }
        drop(line_tx);

        let mut health = tokio::time::interval(cfg.health_interval);
        health.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut connected_once = false;
        let mut misses = 0u32;
        let mut last_error: Option<String> = None;
        let mut diagnostics: VecDeque<String> = VecDeque::new();
        let mut fetched_remote_config = false;
        let mut lines_open = true;

        let outcome = loop {
            tokio::select! {
                line = line_rx.recv(), if lines_open => {
                    match line {
                        Some(line) => {
                            if let Some(info) = lines_ctx.handle(&line) {
                                if diagnostics.len() == 32 { diagnostics.pop_front(); }
                                diagnostics.push_back(info.raw.clone());
                                if matches!(info.level.as_str(), "error" | "fatal") {
                                    last_error = Some(info.raw.clone());
                                    state.failure = Some(explain_diagnostic(&info.raw));
                                    emit_state(&state);
                                }
                                if state.metrics_addr.is_none() {
                                    if let Some(addr) = cloudflared::metrics_addr_from_message(&info.message) {
                                        state.metrics_addr = Some(addr);
                                        health.reset_immediately();
                                    }
                                }
                                if spec.tunnel.kind == TunnelKind::Quick && state.public_url.is_none() {
                                    if let Some(u) = cloudflared::quick_url_from_message(&info.message) {
                                        state.public_url = Some(u);
                                        emit_state(&state);
                                    }
                                }
                            }
                        }
                        None => lines_open = false,
                    }
                }
                _ = health.tick() => {
                    // Proxy listeners must be alive for inspected traffic to flow.
                    if state.inspecting && proxies.values().any(|h| !h.is_running()) {
                        let ups: Vec<Upstream> = proxies.values().map(|h| h.upstream.clone()).collect();
                        if ensure_proxies(&ups, &mut proxies, &ctx).await.is_err() {
                            break RunOutcome::FailOpen;
                        }
                    }
                    let Some(metrics) = state.metrics_addr.clone() else {
                        if started.elapsed() > Duration::from_secs(20) && state.detail.is_none() {
                            state.detail = Some("Waiting for cloudflared to start its metrics server…".into());
                            emit_state(&state);
                        }
                        continue;
                    };
                    let before = (state.status, state.connections, state.public_url.clone(), state.mappings.clone(), state.detail.clone(), state.failure.clone());
                    match get_json(&http, &format!("http://{metrics}/ready")).await {
                        Some((200, v)) => {
                            connected_once = true;
                            misses = 0;
                            state.status = Status::Connected;
                            // Edge readiness cannot prove that a previously failing origin recovered.
                            let origin_failure = last_error.as_ref().is_some_and(|raw| {
                                crate::cli::CfError::classify(Some(1), "", raw).err().is_some_and(|e| e.kind == crate::cli::ErrorKind::OriginConnection)
                            });
                            if !origin_failure {
                                state.failure = None;
                                state.hint = None;
                                last_error = None;
                                diagnostics.clear();
                            }
                            state.connections = v.get("readyConnections").and_then(|c| c.as_u64()).unwrap_or(1) as u32;
                            if state.detail.as_deref().map(|d| d.starts_with("Lost connection") || d.starts_with("Still connecting")).unwrap_or(false) {
                                state.detail = prepared.warning.clone();
                            }
                            if spec.tunnel.kind == TunnelKind::Quick && state.public_url.is_none() {
                                if let Some((200, q)) = get_json(&http, &format!("http://{metrics}/quicktunnel")).await {
                                    if let Some(h) = q.get("hostname").and_then(|h| h.as_str()).filter(|h| !h.is_empty()) {
                                        state.public_url = Some(format!("https://{h}"));
                                    }
                                }
                            }
                            if spec.tunnel.kind == TunnelKind::Quick {
                                if let (Some(url), Some(m)) = (&state.public_url, state.mappings.first_mut()) {
                                    m.hostname = Some(url.trim_start_matches("https://").to_string());
                                }
                            }
                            if spec.tunnel.kind == TunnelKind::Token && !fetched_remote_config {
                                if let Some((200, c)) = get_json(&http, &format!("http://{metrics}/config")).await {
                                    let m = mappings_from_remote_config(&c);
                                    if !m.is_empty() {
                                        state.mappings = m;
                                        fetched_remote_config = true;
                                    }
                                }
                            }
                        }
                        _ if connected_once => {
                            misses += 1;
                            if misses >= 2 {
                                state.status = Status::Degraded;
                                state.connections = 0;
                                state.detail = Some("Lost connection to the Cloudflare edge; cloudflared is reconnecting.".into());
                                state.failure = last_error.as_ref().map(|raw| explain_diagnostic(raw));
                            }
                        }
                        _ => {
                            if started.elapsed() > Duration::from_secs(20) {
                                state.detail = Some("Still connecting…".into());
                            }
                        }
                    }
                    if before != (state.status, state.connections, state.public_url.clone(), state.mappings.clone(), state.detail.clone(), state.failure.clone()) {
                        emit_state(&state);
                    }
                }
                status = child.wait() => {
                    // Drain whatever is left in the pipes so the last error is visible.
                    while let Ok(Some(line)) = tokio::time::timeout(Duration::from_millis(200), line_rx.recv()).await {
                        if let Some(info) = lines_ctx.handle(&line) {
                            if matches!(info.level.as_str(), "error" | "fatal") {
                                last_error = Some(info.raw.clone());
                            }
                            if diagnostics.len() == 32 { diagnostics.pop_front(); }
                            diagnostics.push_back(info.raw);
                        }
                    }
                    // Keep the observed error even if a later burst of info logs filled the tail.
                    if let Some(error) = last_error.filter(|error| !diagnostics.contains(error)) {
                        diagnostics.push_front(error);
                    }
                    break RunOutcome::Exited { code: status.ok().and_then(|s| s.code()), ran_for: started.elapsed(), diagnostics: diagnostics.into_iter().collect::<Vec<_>>().join("\n") };
                }
                cmd = rx.recv() => {
                    let grace = cfg.grace_period + Duration::from_secs(1);
                    match cmd {
                        Some(Cmd::Restart(s)) => {
                            terminate(&mut child, grace).await;
                            break RunOutcome::Restart(s);
                        }
                        Some(Cmd::Stop) | None => {
                            terminate(&mut child, grace).await;
                            break RunOutcome::Stopped;
                        }
                    }
                }
            }
        };
        // The direct child is gone; clean up anything left in its process group.
        sweep_group(child_pid, cfg.grace_period).await;
        group_guard.0 = None;
        remove_own_pidfile(&cfg, &id, child_pid);
        state.pid = None;
        state.metrics_addr = None;

        match outcome {
            RunOutcome::Stopped => break 'outer,
            RunOutcome::Restart(s) => {
                spec = *s;
                failures = 0;
                force_no_inspect = false;
                state.restarts += 1;
                continue 'outer;
            }
            RunOutcome::FailOpen => {
                terminate(&mut child, cfg.grace_period + Duration::from_secs(1)).await;
                force_no_inspect = true;
                state.restarts += 1;
                continue 'outer;
            }
            RunOutcome::Exited { code, ran_for, diagnostics } => {
                if ran_for > Duration::from_secs(120) {
                    failures = 0;
                }
                failures += 1;
                state.status = Status::Crashed;
                state.connections = 0;
                state.failure = Some(explain_exit(code, &diagnostics));
                state.hint = state.failure.as_ref().and_then(|f| f.action.clone());
                if failures >= cfg.max_failures {
                    state.detail = Some(format!("Gave up after {failures} attempts."));
                    emit_state(&state);
                    match rx.recv().await {
                        Some(Cmd::Restart(s)) => {
                            spec = *s;
                            failures = 0;
                            continue 'outer;
                        }
                        _ => break 'outer,
                    }
                }
                let delay = backoff(cfg.backoff_base, cfg.backoff_max, failures);
                state.detail = Some(format!(
                    "Restarting in {}s (attempt {}/{}).",
                    delay.as_secs().max(1),
                    failures,
                    cfg.max_failures
                ));
                emit_state(&state);
                tokio::select! {
                    _ = tokio::time::sleep(delay) => {
                        state.restarts += 1;
                        continue 'outer;
                    }
                    cmd = rx.recv() => match cmd {
                        Some(Cmd::Restart(s)) => {
                            spec = *s;
                            failures = 0;
                            continue 'outer;
                        }
                        _ => break 'outer,
                    }
                }
            }
        }
    }

    drop(proxies);
    let mut final_state = TunnelState::stopped(&id);
    final_state.run = run_no;
    final_state.config_revision = state.config_revision;
    final_state.restarts = state.restarts;
    final_state.mappings = state.mappings;
    emit_state(&final_state);
}

/// 1s, 2s, 4s, ... capped.
pub fn backoff(base: Duration, max: Duration, attempt: u32) -> Duration {
    let exp = attempt.saturating_sub(1).min(16);
    (base * 2u32.pow(exp)).min(max)
}
