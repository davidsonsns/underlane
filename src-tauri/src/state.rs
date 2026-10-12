//! Application state shared by commands, the tray and background tasks.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use underlane_core::cloudflared::{self, Binary};
use underlane_core::db::{Db, RequestSummary};
use underlane_core::model::{LogLine, Settings, Tunnel, TunnelKind};
use underlane_core::proxy::capture::{CaptureOptions, CapturedExchange, SharedOptions};
use underlane_core::proxy::now_ms;
use underlane_core::supervisor::{Event, RunSpec, Status, Supervisor, SupervisorConfig, TunnelState};
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;

use crate::{platform, tokens, tray};

/// Records waiting for the batched SQLite writer. The channel is bounded: when storage falls
/// behind, records are dropped (and counted) rather than slowing tunnel traffic.
pub enum Write {
    Log(LogLine),
    Request(CapturedExchange),
}

const WRITE_QUEUE: usize = 20_000;
/// Body bytes allowed to wait in the write queue; beyond this, captures are dropped and counted.
const QUEUE_BYTES: usize = 64 * 1024 * 1024;
/// Trigger a size-cap sweep after this many bytes were written since the last one.
const SWEEP_AFTER_BYTES: usize = 32 * 1024 * 1024;

fn record_bytes(x: &CapturedExchange) -> usize {
    x.req_body.bytes.len() + x.resp_body.bytes.len() + 512
}

pub struct Core {
    pub handle: AppHandle,
    pub db: Arc<Db>,
    pub sup: Arc<Supervisor>,
    pub states: Mutex<HashMap<String, TunnelState>>,
    pub capture: SharedOptions,
    pub binary: RwLock<Option<Binary>>,
    pub settings: RwLock<Settings>,
    pub data_dir: PathBuf,
    pub dropped: Arc<AtomicU64>,
    queued_bytes: Arc<AtomicUsize>,
    written_since_sweep: AtomicUsize,
    sweep_now: tokio::sync::Notify,
    /// Latest supervisor run seen per tunnel; older runs' late events are ignored.
    latest_run: Mutex<HashMap<String, u64>>,
    activity: Mutex<Option<platform::Activity>>,
}

/// Everything the UI needs to render a tunnel.
#[derive(Debug, Clone, Serialize)]
pub struct TunnelView {
    #[serde(flatten)]
    pub tunnel: Tunnel,
    pub project_name: String,
    pub state: TunnelState,
    pub has_token: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Overview {
    pub projects: Vec<underlane_core::model::Project>,
    pub tunnels: Vec<TunnelView>,
    pub cloudflared: Option<Binary>,
    pub settings: Settings,
    pub active_count: usize,
    pub dropped_captures: u64,
    pub data_dir: String,
    /// `{"at": ms, "reason": "login_item" | "normal"}` for the current launch.
    pub last_launch: Option<serde_json::Value>,
}

pub fn capture_options(s: &Settings) -> CaptureOptions {
    CaptureOptions {
        max_body_bytes: s.max_body_kb as usize * 1024,
        redact_headers: s.redact_headers.clone(),
        redact_query: s.redact_query.clone(),
    }
}

impl Core {
    pub async fn init(handle: AppHandle, data_dir: PathBuf, shim: Option<PathBuf>) -> anyhow::Result<Arc<Core>> {
        std::fs::create_dir_all(&data_dir)?;
        let killed = underlane_core::supervisor::reap_orphans(&data_dir);
        if !killed.is_empty() {
            tracing::warn!("stopped {} cloudflared process(es) left over from a previous run", killed.len());
        }
        let db = Arc::new(Db::open(&data_dir.join("underlane.db"))?);
        let settings = db.settings()?;
        let capture: SharedOptions = Arc::new(RwLock::new(capture_options(&settings)));
        let binary = cloudflared::discover(settings.cloudflared_path.as_deref()).await;

        let (write_tx, write_rx) = mpsc::channel::<Write>(WRITE_QUEUE);
        let (state_tx, state_rx) = mpsc::unbounded_channel::<TunnelState>();
        let dropped = Arc::new(AtomicU64::new(0));
        let dropped_sink = dropped.clone();
        let queued_bytes = Arc::new(AtomicUsize::new(0));
        let queued_sink = queued_bytes.clone();
        let sink = Arc::new(move |e: Event| match e {
            Event::State(s) => {
                let _ = state_tx.send(s);
            }
            Event::Log(l) => {
                if write_tx.try_send(Write::Log(l)).is_err() {
                    dropped_sink.fetch_add(1, Ordering::Relaxed);
                }
            }
            Event::Request(x) => {
                let n = record_bytes(&x);
                if queued_sink.fetch_add(n, Ordering::AcqRel) + n > QUEUE_BYTES {
                    queued_sink.fetch_sub(n, Ordering::AcqRel);
                    dropped_sink.fetch_add(1, Ordering::Relaxed);
                } else if write_tx.try_send(Write::Request(x)).is_err() {
                    queued_sink.fetch_sub(n, Ordering::AcqRel);
                    dropped_sink.fetch_add(1, Ordering::Relaxed);
                }
            }
        });

        let mut cfg = SupervisorConfig::new(data_dir.clone(), binary.as_ref().map(|b| PathBuf::from(&b.path)), capture.clone());
        cfg.shim = shim;
        let sup = Supervisor::new(cfg, sink);

        let core = Arc::new(Core {
            handle,
            db,
            sup,
            states: Mutex::new(HashMap::new()),
            capture,
            binary: RwLock::new(binary),
            settings: RwLock::new(settings),
            data_dir,
            dropped,
            queued_bytes,
            written_since_sweep: AtomicUsize::new(0),
            sweep_now: tokio::sync::Notify::new(),
            latest_run: Mutex::new(HashMap::new()),
            activity: Mutex::new(None),
        });
        tokio::spawn(writer_task(core.clone(), write_rx));
        tokio::spawn(state_task(core.clone(), state_rx));
        tokio::spawn(sweep_task(core.clone()));
        Ok(core)
    }

    pub fn state_of(&self, id: &str) -> TunnelState {
        self.states.lock().unwrap().get(id).cloned().unwrap_or_else(|| TunnelState::stopped(id))
    }

    /// State plus the routes parsed from the config file when the tunnel isn't running, so a
    /// stopped tunnel still shows where it would send traffic.
    fn state_with_routes(&self, t: &Tunnel) -> TunnelState {
        let mut s = self.state_of(&t.id);
        if s.mappings.is_empty() && t.kind == TunnelKind::Config {
            let loaded = t.config_path.as_deref().map(|p| underlane_core::config::TunnelConfig::load(std::path::Path::new(p)));
            if let Some(Err(e)) = &loaded {
                if s.status == Status::Stopped && s.detail.is_none() {
                    s.detail = Some(format!("The config file can't be read: {e}"));
                }
            }
            if let Some(Ok(c)) = loaded {
                s.mappings = c
                    .ingress
                    .iter()
                    .map(|r| underlane_core::supervisor::RouteMapping {
                        hostname: r.hostname.clone(),
                        path: r.path.clone(),
                        service: r.service.clone(),
                        inspected: false,
                        note: None,
                    })
                    .collect();
            }
        }
        s
    }

    /// Tell every window that the tunnel list or a tunnel's definition changed.
    pub fn notify_changed(&self) {
        let _ = self.handle.emit("tunnels-changed", ());
    }

    pub fn active_count(&self) -> usize {
        self.states.lock().unwrap().values().filter(|s| s.status.is_active()).count()
    }

    pub fn overview(&self) -> anyhow::Result<Overview> {
        let projects = self.db.projects()?;
        let names: HashMap<String, String> = projects.iter().map(|p| (p.id.clone(), p.name.clone())).collect();
        let tunnels = self
            .db
            .tunnels()?
            .into_iter()
            .map(|t| TunnelView {
                project_name: names.get(&t.project_id).cloned().unwrap_or_default(),
                state: self.state_with_routes(&t),
                has_token: t.kind == TunnelKind::Token && self.db.kv_get(&format!("token:{}", t.id)).ok().flatten().is_some(),
                tunnel: t,
            })
            .collect();
        Ok(Overview {
            projects,
            tunnels,
            cloudflared: self.binary.read().unwrap().clone(),
            settings: self.settings.read().unwrap().clone(),
            active_count: self.active_count(),
            dropped_captures: self.dropped.load(Ordering::Relaxed),
            data_dir: self.data_dir.to_string_lossy().into_owned(),
            last_launch: self.db.kv_get("last_launch").ok().flatten().and_then(|s| serde_json::from_str(&s).ok()),
        })
    }

    pub fn spec(&self, id: &str) -> anyhow::Result<RunSpec> {
        let tunnel = self.db.tunnel(id)?.ok_or_else(|| anyhow::anyhow!("No tunnel with id {id}"))?;
        let token = if tunnel.kind == TunnelKind::Token { tokens::get(&tunnel.id) } else { None };
        Ok(RunSpec { tunnel, token })
    }

    /// Start (or retry, if it is sitting in a crashed state).
    pub fn start(&self, id: &str) -> anyhow::Result<()> {
        let spec = self.spec(id)?;
        let status = self.state_of(id).status;
        if self.sup.is_running(id) && status == Status::Crashed {
            self.sup.restart(spec);
        } else {
            self.sup.start(spec);
        }
        Ok(())
    }

    pub fn restart(&self, id: &str) -> anyhow::Result<()> {
        self.sup.restart(self.spec(id)?);
        Ok(())
    }

    /// Apply a changed tunnel definition to a running process.
    pub fn restart_if_running(&self, id: &str) -> anyhow::Result<()> {
        if self.sup.is_running(id) {
            self.restart(id)?;
        }
        Ok(())
    }

    pub async fn stop(&self, id: &str) {
        self.sup.stop(id).await;
    }

    pub fn start_autostart_tunnels(&self) {
        if let Ok(tunnels) = self.db.tunnels() {
            for t in tunnels.into_iter().filter(|t| t.autostart) {
                if let Err(e) = self.start(&t.id) {
                    tracing::warn!("autostart {}: {e}", t.name);
                }
            }
        }
    }

    pub fn apply_settings(&self, s: Settings) -> anyhow::Result<()> {
        self.db.save_settings(&s)?;
        *self.capture.write().unwrap() = capture_options(&s);
        *self.settings.write().unwrap() = s.clone();
        crate::tray::apply_window_theme(&self.handle, &s.theme);
        let _ = self.handle.emit("settings-changed", &s);
        Ok(())
    }

    pub async fn rediscover(&self) -> Option<Binary> {
        let path = self.settings.read().unwrap().cloudflared_path.clone();
        let b = cloudflared::discover(path.as_deref()).await;
        *self.sup.config().cloudflared.write().unwrap() = b.as_ref().map(|b| PathBuf::from(&b.path));
        *self.binary.write().unwrap() = b.clone();
        b
    }

    fn on_state(&self, s: TunnelState) {
        {
            let mut latest = self.latest_run.lock().unwrap();
            let seen = latest.entry(s.id.clone()).or_insert(0);
            if s.run < *seen {
                return; // a late event from a run that has been replaced
            }
            *seen = s.run;
        }
        let prev = self.states.lock().unwrap().insert(s.id.clone(), s.clone());
        if prev.as_ref() == Some(&s) {
            return;
        }
        let _ = self.handle.emit("tunnel-state", &s);
        let active = self.active_count();
        tray::set_count(&self.handle, active);
        // Keep App Nap from throttling timers and the proxy while tunnels run.
        let any_running = self.states.lock().unwrap().values().any(|s| s.status != Status::Stopped && s.status != Status::Crashed);
        let mut act = self.activity.lock().unwrap();
        match (any_running, act.is_some()) {
            (true, false) => *act = platform::Activity::begin("Running cloudflared tunnels"),
            (false, true) => *act = None,
            _ => {}
        }
    }
}

pub fn summary_of(x: &CapturedExchange) -> RequestSummary {
    RequestSummary {
        id: x.id.clone(),
        tunnel_id: x.tunnel_id.clone(),
        ts: x.started_at_ms,
        duration_ms: x.duration_ms,
        kind: x.kind.as_str().into(),
        method: x.method.clone(),
        host: x.host.clone(),
        path: x.path.clone(),
        status: x.status,
        resp_size: x.resp_body.size,
        error: x.error.clone(),
    }
}

/// Batch writes into SQLite and forward what was stored to the UI.
async fn writer_task(core: Arc<Core>, mut rx: mpsc::Receiver<Write>) {
    let mut logs: Vec<LogLine> = Vec::new();
    let mut reqs: Vec<CapturedExchange> = Vec::new();
    loop {
        let Some(first) = rx.recv().await else { break };
        let deadline = tokio::time::Instant::now() + Duration::from_millis(150);
        let mut next = Some(first);
        while let Some(w) = next.take() {
            match w {
                Write::Log(l) => logs.push(l),
                Write::Request(x) => reqs.push(x),
            }
            if logs.len() + reqs.len() >= 2000 {
                break;
            }
            if let Ok(Some(w)) = tokio::time::timeout_at(deadline, rx.recv()).await {
                next = Some(w);
            }
        }
        let db = core.db.clone();
        let batch_logs = std::mem::take(&mut logs);
        let batch_reqs = std::mem::take(&mut reqs);
        let stored = tokio::task::spawn_blocking(move || {
            let mut out_logs = Vec::new();
            if !batch_logs.is_empty() {
                if let Ok(ids) = db.insert_logs(&batch_logs) {
                    out_logs = batch_logs.into_iter().zip(ids).map(|(mut l, id)| {
                        l.id = id;
                        l
                    }).collect();
                }
            }
            let mut out_reqs = Vec::new();
            let mut bytes = 0usize;
            for x in batch_reqs {
                bytes += record_bytes(&x);
                if db.upsert_request(&x).is_ok() {
                    out_reqs.push(summary_of(&x));
                }
            }
            (out_logs, out_reqs, bytes)
        })
        .await;
        if let Ok((l, r, bytes)) = stored {
            core.queued_bytes.fetch_sub(bytes.min(core.queued_bytes.load(Ordering::Acquire)), Ordering::AcqRel);
            let log_bytes: usize = l.iter().map(|x| x.message.len() + x.fields.len() + 64).sum();
            if core.written_since_sweep.fetch_add(bytes + log_bytes, Ordering::AcqRel) + bytes + log_bytes > SWEEP_AFTER_BYTES {
                core.written_since_sweep.store(0, Ordering::Release);
                core.sweep_now.notify_one();
            }
            if !l.is_empty() {
                let _ = core.handle.emit("logs", &l);
            }
            for s in r {
                let _ = core.handle.emit("request", &s);
            }
        }
    }
}

async fn state_task(core: Arc<Core>, mut rx: mpsc::UnboundedReceiver<TunnelState>) {
    while let Some(s) = rx.recv().await {
        core.on_state(s);
    }
}

/// Expire old logs/requests at startup and every 10 minutes.
async fn sweep_task(core: Arc<Core>) {
    let mut tick = tokio::time::interval(Duration::from_secs(600));
    loop {
        // Every 10 minutes, or sooner when a lot has been written.
        tokio::select! {
            _ = tick.tick() => {}
            _ = core.sweep_now.notified() => {}
        }
        let (ttl_days, max_mb) = {
            let s = core.settings.read().unwrap();
            (s.log_ttl_days.max(1) as i64, s.max_db_mb.max(16) as u64)
        };
        let db = core.db.clone();
        let res = tokio::task::spawn_blocking(move || db.sweep(now_ms(), ttl_days * 86_400_000, max_mb * 1024 * 1024)).await;
        if let Ok(Ok(stats)) = res {
            if stats != Default::default() {
                tracing::info!("sweep: {stats:?}");
            }
        }
    }
}
