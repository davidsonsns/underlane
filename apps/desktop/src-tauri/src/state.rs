//! Application state shared by commands, the tray and background tasks.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use underlane_core::cloudflared::{self, Binary};
use underlane_core::db::{Db, RequestSummary};
use underlane_core::model::{LogLine, Settings, Tunnel, TunnelKind};
use underlane_core::proxy::capture::{CaptureOptions, CapturedExchange, SharedOptions};
use underlane_core::proxy::now_ms;
use underlane_core::supervisor::{ConfigChange, Event, RunSpec, Status, Supervisor, SupervisorConfig, TunnelState};
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;

use crate::{platform, tokens, tray};

/// Records waiting for the batched SQLite writer. The channel is bounded: when storage falls
/// behind, records are dropped (and counted) rather than slowing tunnel traffic.
pub enum Write {
    Log(LogLine),
    Request(CapturedExchange, u64),
}

const WRITE_QUEUE: usize = 20_000;
/// Body bytes allowed to wait in the write queue; beyond this, captures are dropped and counted.
const QUEUE_BYTES: usize = 64 * 1024 * 1024;
/// Trigger a size-cap sweep after this many bytes were written since the last one.
const SWEEP_AFTER_BYTES: usize = 32 * 1024 * 1024;

fn record_bytes(x: &CapturedExchange) -> usize {
    x.req_body.bytes.len() + x.resp_body.bytes.len() + 512
}

#[derive(Default)]
struct CaptureState {
    paused: bool,
    generation: u64,
}

#[derive(Default)]
struct RequestCapture {
    tunnels: RwLock<HashMap<String, CaptureState>>,
    // Pause waits for writes without holding the state lock used by the capture sink.
    storage: Mutex<()>,
}

impl RequestCapture {
    fn generation(&self, id: &str) -> Option<u64> {
        match self.tunnels.read().unwrap().get(id) {
            Some(s) if s.paused => None,
            Some(s) => Some(s.generation),
            None => Some(0),
        }
    }

    fn set_paused<T>(&self, id: &str, paused: bool, notify: impl FnOnce() -> T) -> T {
        let _storage = self.storage.lock().unwrap();
        let mut tunnels = self.tunnels.write().unwrap();
        let state = tunnels.entry(id.to_string()).or_default();
        if state.paused != paused {
            state.paused = paused;
            // Invalidate queued captures even if capture resumes before the writer runs.
            state.generation = state.generation.wrapping_add(1);
        }
        // Publish changes under the same lock so concurrent windows observe them in order.
        notify()
    }

    fn enqueue(&self, x: CapturedExchange, epoch: u64, tx: &mpsc::Sender<(u64, Write)>, queued: &AtomicUsize, dropped: &AtomicU64) {
        let Some(generation) = self.generation(&x.tunnel_id) else { return };
        let n = record_bytes(&x);
        if queued.fetch_add(n, Ordering::AcqRel) + n > QUEUE_BYTES {
            queued.fetch_sub(n, Ordering::AcqRel);
            dropped.fetch_add(1, Ordering::Relaxed);
        } else if tx.try_send((epoch, Write::Request(x, generation))).is_err() {
            queued.fetch_sub(n, Ordering::AcqRel);
            dropped.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn store(&self, db: &Db, x: &CapturedExchange, generation: u64) -> Option<RequestSummary> {
        let _storage = self.storage.lock().unwrap();
        if self.generation(&x.tunnel_id) != Some(generation) { return None; }
        db.upsert_request(x).ok().map(|_| summary_of(x))
    }
}

#[cfg(test)]
mod request_capture_tests {
    use super::*;
    use underlane_core::proxy::capture::{BodyCapture, ExchangeKind};

    fn exchange(id: &str, tunnel_id: &str) -> CapturedExchange {
        CapturedExchange {
            id: id.into(), tunnel_id: tunnel_id.into(), rule_index: 0, started_at_ms: 1,
            duration_ms: Some(1), kind: ExchangeKind::Http, method: "GET".into(),
            host: "localhost".into(), path: "/".into(), status: Some(200),
            req_headers: vec![], resp_headers: vec![], req_body: BodyCapture::default(),
            resp_body: BodyCapture::default(), error: None,
        }
    }

    #[test]
    fn reset_rejects_queued_history_and_allows_new_writes_on_the_same_connection() {
        let db = Db::open_in_memory().unwrap();
        let capture = RequestCapture::default();
        let epoch = RwLock::new(0);
        let add_tunnel = || db.upsert_tunnel(&Tunnel {
            id: "a".into(), group_id: None, name: "Local".into(), kind: TunnelKind::Quick,
            config_path: None, origin_url: Some("http://localhost:3000".into()),
            inspect: false, autostart: false, loglevel: "info".into(), created_at: 0, sort: 0,
        }).unwrap();
        add_tunnel();
        let log = || LogLine { id: 0, tunnel_id: "a".into(), ts: 1, level: "info".into(), message: "history".into(), fields: "{}".into() };
        store_batch(&db, &capture, &epoch, vec![(0, log())], vec![(0, exchange("before", "a"), 0)]);
        assert_eq!(db.query_logs(&Default::default()).unwrap().len(), 1);
        {
            let mut current = epoch.write().unwrap();
            db.reset().unwrap();
            *current += 1;
        }
        let (logs, requests, bytes) = store_batch(&db, &capture, &epoch, vec![(0, log())], vec![(0, exchange("stale", "a"), 0)]);
        assert!(logs.is_empty() && requests.is_empty());
        assert!(bytes > 0, "discarded captures still release queue bytes");
        assert!(db.query_logs(&Default::default()).unwrap().is_empty());
        assert!(db.get_request("stale").unwrap().is_none());
        // An old in-flight response can arrive after the reset and receive the new epoch.
        store_batch(&db, &capture, &epoch, vec![(1, log())], vec![(1, exchange("late", "a"), 0)]);
        assert!(db.query_logs(&Default::default()).unwrap().is_empty());
        assert!(db.get_request("late").unwrap().is_none());
        add_tunnel();
        store_batch(&db, &capture, &epoch, vec![(1, log())], vec![(1, exchange("fresh", "a"), 0)]);
        assert_eq!(db.query_logs(&Default::default()).unwrap().len(), 1);
        assert!(db.get_request("fresh").unwrap().is_some());
    }

    #[test]
    fn pause_skips_capture_without_counting_drops_or_affecting_other_tunnels() {
        let capture = RequestCapture::default();
        let (tx, mut rx) = mpsc::channel(1);
        let queued = AtomicUsize::new(0);
        let dropped = AtomicU64::new(0);
        capture.set_paused("a", true, || {});
        capture.enqueue(exchange("paused", "a"), 0, &tx, &queued, &dropped);
        assert!(rx.try_recv().is_err());
        assert_eq!(queued.load(Ordering::Relaxed), 0);
        assert_eq!(dropped.load(Ordering::Relaxed), 0);
        capture.enqueue(exchange("other", "b"), 0, &tx, &queued, &dropped);
        assert!(matches!(rx.try_recv().unwrap(), (0, Write::Request(x, 0)) if x.id == "other"));
        assert_eq!(dropped.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn pause_invalidates_queued_captures_even_after_resuming() {
        let capture = RequestCapture::default();
        let db = Db::open_in_memory().unwrap();
        let (tx, mut rx) = mpsc::channel(3);
        let queued = AtomicUsize::new(0);
        let dropped = AtomicU64::new(0);
        capture.enqueue(exchange("queued", "a"), 0, &tx, &queued, &dropped);
        capture.set_paused("a", true, || {});
        let (0, Write::Request(x, generation)) = rx.try_recv().unwrap() else { panic!("request expected") };
        assert!(capture.store(&db, &x, generation).is_none());
        capture.set_paused("a", false, || {});
        assert!(capture.store(&db, &x, generation).is_none());
        assert!(db.get_request("queued").unwrap().is_none());
        capture.enqueue(exchange("resumed", "a"), 0, &tx, &queued, &dropped);
        let (0, Write::Request(x, generation)) = rx.try_recv().unwrap() else { panic!("request expected") };
        capture.set_paused("a", false, || {}); // An unchanged toggle must preserve fresh captures.
        assert!(capture.store(&db, &x, generation).is_some());
        assert!(db.get_request("resumed").unwrap().is_some());
        assert_eq!(dropped.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn pause_waits_for_storage_without_blocking_the_capture_sink() {
        let capture = Arc::new(RequestCapture::default());
        let storage = capture.storage.lock().unwrap();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (paused_tx, paused_rx) = std::sync::mpsc::channel();
        let pause_capture = capture.clone();
        let pause = std::thread::spawn(move || {
            started_tx.send(()).unwrap();
            pause_capture.set_paused("a", true, || paused_tx.send(()).unwrap());
        });
        started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(paused_rx.recv_timeout(Duration::from_millis(20)).is_err());
        assert_eq!(capture.generation("a"), Some(0), "pause cannot acknowledge before writes finish");
        let (tx, mut rx) = mpsc::channel(1);
        capture.enqueue(exchange("queued", "a"), 0, &tx, &AtomicUsize::new(0), &AtomicU64::new(0));
        let (0, Write::Request(x, generation)) = rx.try_recv().unwrap() else { panic!("request expected") };
        drop(storage);
        paused_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        pause.join().unwrap();
        let db = Db::open_in_memory().unwrap();
        assert!(capture.store(&db, &x, generation).is_none());
        assert!(db.get_request("queued").unwrap().is_none());
    }
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
    pub operations: tokio::sync::RwLock<()>,
    pub data_epoch: Arc<RwLock<u64>>,
    request_capture: Arc<RequestCapture>,
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
    pub group_name: String,
    pub state: TunnelState,
    pub has_token: bool,
    pub setup_needed: bool,
    pub cloudflare_tunnel_id: Option<String>,
    pub separating_tunnel: bool,
    pub separate_tunnel_pending: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Overview {
    pub groups: Vec<underlane_core::model::Group>,
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

#[cfg(test)]
mod config_change_tests {
    use super::*;
    use std::cell::Cell;
    use underlane_core::config::ConfigFingerprint;
    use underlane_core::supervisor::{ConfigChangeStatus, LoadedConfig};

    fn fixture() -> (Db, Tunnel, TunnelState, PathBuf) {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let nonce = format!("{}-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos(), NEXT.fetch_add(1, Ordering::Relaxed));
        let path = std::env::temp_dir().join(format!("underlane-config-check-{}-{nonce}.yml", std::process::id()));
        let text = "url: http://localhost:1\n";
        std::fs::write(&path, text).unwrap();
        let db = Db::open_in_memory().unwrap();
        let group = db.ensure_group("Synthetic config check").unwrap();
        let tunnel = Tunnel {
            id: "config-check".into(), group_id: Some(group.id), name: "Synthetic".into(),
            kind: TunnelKind::Config, config_path: Some(path.to_string_lossy().into_owned()),
            origin_url: None, inspect: false, autostart: false, loglevel: "info".into(), created_at: 0, sort: 0,
        };
        db.upsert_tunnel(&tunnel).unwrap();
        let mut state = TunnelState::stopped(&tunnel.id);
        state.supervised = true;
        state.run = 7;
        state.config_revision = 2;
        state.loaded_config = Some(LoadedConfig {
            source_path: tunnel.config_path.clone().unwrap(), fingerprint: ConfigFingerprint::of(text.as_bytes()),
        });
        (db, tunnel, state, path)
    }

    #[test]
    fn comparison_discards_a_replaced_run_revision_baseline_or_stop() {
        let (db, tunnel, state, path) = fixture();
        for change in 0..4 {
            let mut newer = state.clone();
            match change {
                0 => newer.run += 1,
                1 => newer.config_revision += 1,
                2 => newer.loaded_config.as_mut().unwrap().fingerprint = ConfigFingerprint::of(b"different loaded bytes"),
                _ => { newer.supervised = false; newer.loaded_config = None; }
            }
            let calls = Cell::new(0);
            let result = check_config_change_inner(&db, &tunnel.id, || {
                calls.set(calls.get() + 1);
                if calls.get() == 1 { state.clone() } else { newer.clone() }
            }).unwrap();
            assert!(result.is_none(), "superseded comparison {change} discarded");
        }
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn comparison_discards_a_changed_saved_source_path() {
        let (db, mut tunnel, state, path) = fixture();
        let id = tunnel.id.clone();
        tunnel.config_path = Some("/synthetic/replacement.yml".into());
        let result = check_config_change_inner(&db, &id, || {
            db.upsert_tunnel(&tunnel).unwrap();
            state.clone()
        }).unwrap();
        assert!(result.is_none());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn comparison_is_offline_and_stopped_or_unsupported_tunnels_have_no_mismatch() {
        let (db, mut tunnel, state, path) = fixture();
        assert_eq!(check_config_change_inner(&db, &tunnel.id, || state.clone()).unwrap().unwrap().status, ConfigChangeStatus::Unchanged);
        std::fs::write(&path, "url: http://localhost:2\n").unwrap();
        assert_eq!(check_config_change_inner(&db, &tunnel.id, || state.clone()).unwrap().unwrap().status, ConfigChangeStatus::Changed);
        let mut stopped = state.clone();
        stopped.supervised = false;
        stopped.loaded_config = None;
        assert_eq!(check_config_change_inner(&db, &tunnel.id, || stopped.clone()).unwrap().unwrap().status, ConfigChangeStatus::NotRunning);
        for kind in [TunnelKind::Quick, TunnelKind::Token] {
            tunnel.kind = kind;
            db.upsert_tunnel(&tunnel).unwrap();
            assert!(check_config_change_inner(&db, &tunnel.id, || panic!("unsupported checks must not inspect state or disk")).unwrap().is_none());
        }
        std::fs::remove_file(path).unwrap();
    }
}

fn check_config_change_inner(db: &Db, id: &str, config_state: impl Fn() -> TunnelState) -> anyhow::Result<Option<ConfigChange>> {
    let tunnel = db.tunnel(id)?.ok_or_else(|| anyhow::anyhow!("Tunnel not found"))?;
    if tunnel.kind != TunnelKind::Config { return Ok(None); }
    let Some(path) = tunnel.config_path else { return Ok(None) };
    let snapshot = config_state();
    let result = snapshot.check_config_change(&path);
    let current_tunnel = db.tunnel(id)?;
    let current = config_state();
    if current.run != snapshot.run || current.config_revision != snapshot.config_revision
        || current.loaded_config != snapshot.loaded_config || current.supervised != snapshot.supervised
        || current_tunnel.map(|t| t.kind != TunnelKind::Config || t.config_path.as_deref() != Some(path.as_str())).unwrap_or(true) {
        return Ok(None);
    }
    Ok(Some(result))
}

async fn discover_binary(data_dir: &std::path::Path, configured: Option<&str>) -> Option<Binary> {
    if let Some(found) = cloudflared::discover(configured).await {
        return Some(found);
    }
    if configured.is_some_and(|path| !path.trim().is_empty()) {
        return None;
    }
    crate::installer::validate(&crate::installer::managed_path(data_dir)).await.ok()
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
        if !db.tunnels()?.is_empty() {
            db.kv_set("onboarding_complete", "1")?;
        }
        let binary = discover_binary(&data_dir, settings.cloudflared_path.as_deref()).await;

        let (write_tx, write_rx) = mpsc::channel::<(u64, Write)>(WRITE_QUEUE);
        let (state_tx, state_rx) = mpsc::unbounded_channel::<(u64, TunnelState)>();
        let data_epoch = Arc::new(RwLock::new(0));
        let sink_epoch = data_epoch.clone();
        let dropped = Arc::new(AtomicU64::new(0));
        let dropped_sink = dropped.clone();
        let queued_bytes = Arc::new(AtomicUsize::new(0));
        let queued_sink = queued_bytes.clone();
        let request_capture = Arc::new(RequestCapture::default());
        let capture_sink = request_capture.clone();
        let sink = Arc::new(move |e: Event| {
            let epoch = *sink_epoch.read().unwrap();
            match e {
            Event::State(s) => {
                let _ = state_tx.send((epoch, s));
            }
            Event::Log(l) => {
                if write_tx.try_send((epoch, Write::Log(l))).is_err() {
                    dropped_sink.fetch_add(1, Ordering::Relaxed);
                }
            }
            Event::Request(x) => {
                capture_sink.enqueue(x, epoch, &write_tx, &queued_sink, &dropped_sink);
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
            operations: tokio::sync::RwLock::new(()),
            data_epoch,
            request_capture,
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

    pub fn request_capture_paused(&self, id: &str) -> anyhow::Result<bool> {
        anyhow::ensure!(self.db.tunnel(id)?.is_some(), "Tunnel not found");
        Ok(self.request_capture.generation(id).is_none())
    }

    pub fn set_request_capture_paused(&self, id: &str, paused: bool) -> anyhow::Result<()> {
        anyhow::ensure!(self.db.tunnel(id)?.is_some(), "Tunnel not found");
        self.request_capture.set_paused(id, paused, || {
            self.handle.emit("request-capture-paused", serde_json::json!({ "tunnel_id": id, "paused": paused }))
        })?;
        Ok(())
    }

    /// Called on the blocking pool; reject results superseded by a reload, stop or definition edit.
    pub fn check_config_change(&self, id: &str) -> anyhow::Result<Option<ConfigChange>> {
        check_config_change_inner(&self.db, id, || self.sup.config_state(id).unwrap_or_else(|| {
            let mut state = self.state_of(id);
            state.supervised = false;
            state.loaded_config = None;
            state
        }))
    }

    /// State plus the routes parsed from the config file when the tunnel isn't running, so a
    /// stopped tunnel still shows where it would send traffic.
    fn state_with_routes(&self, t: &Tunnel) -> (TunnelState, bool) {
        let mut s = self.state_of(&t.id);
        let loaded = if t.kind == TunnelKind::Config {
            t.config_path.as_deref().map(|p| underlane_core::config::TunnelConfig::load(std::path::Path::new(p)))
        } else { None };
        let setup_needed = loaded.as_ref().and_then(|result| result.as_ref().ok())
            .is_some_and(underlane_core::projects::needs_setup);
        if (s.mappings.is_empty() || s.status == Status::Stopped) && t.kind == TunnelKind::Config {
            if let Some(Err(e)) = &loaded {
                if s.status == Status::Stopped && s.detail.is_none() {
                    s.detail = Some(underlane_core::secret::redact_text(&format!("The config file can't be read: {e}")));
                }
            }
            if let Some(Ok(c)) = loaded {
                if s.status == Status::Stopped && setup_needed && s.detail.is_none() {
                    s.detail = Some(underlane_core::projects::DRAFT_DETAIL.into());
                    s.hint = Some(underlane_core::projects::DRAFT_HINT.into());
                }
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
        (s, setup_needed)
    }

    /// Tell every window that the tunnel list or a tunnel's definition changed.
    pub fn notify_changed(&self) {
        let _ = self.handle.emit("tunnels-changed", ());
    }

    pub fn active_count(&self) -> usize {
        self.states.lock().unwrap().values().filter(|s| s.status.is_active()).count()
    }

    pub fn overview(&self) -> anyhow::Result<Overview> {
        let groups = self.db.groups()?;
        let names: HashMap<String, String> = groups.iter().map(|p| (p.id.clone(), p.name.clone())).collect();
        let tunnels = self
            .db
            .tunnels()?
            .into_iter()
            .map(|t| {
                let (state, setup_needed) = self.state_with_routes(&t);
                TunnelView {
                    group_name: t.group_id.as_ref().and_then(|id| names.get(id)).cloned().unwrap_or_default(),
                    state,
                    setup_needed,
                    separating_tunnel: crate::separate_tunnel::is_running(&t.id),
                    separate_tunnel_pending: crate::separate_tunnel::start_blocked(self, &t.id).unwrap_or(true),
                    cloudflare_tunnel_id: t.config_path.as_deref()
                        .filter(|_| t.kind == TunnelKind::Config)
                        .and_then(|path| underlane_core::config::TunnelConfig::load(std::path::Path::new(path)).ok())
                        .and_then(|config| underlane_core::supervisor::config_tunnel_identity(&config).ok().flatten()),
                    has_token: t.kind == TunnelKind::Token && self.db.kv_get(&format!("token:{}", t.id)).ok().flatten().is_some(),
                    tunnel: t,
                }
            })
            .collect();
        Ok(Overview {
            groups,
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
        let _operation = self.operations.try_read().map_err(|_| anyhow::anyhow!("App reset is in progress."))?;
        anyhow::ensure!(!crate::separate_tunnel::is_running(id), "Finish creating the separate tunnel before starting this project.");
        anyhow::ensure!(!crate::separate_tunnel::start_blocked(self, id).map_err(anyhow::Error::msg)?, "Separate tunnel setup is unfinished. Retry Create separate tunnel to finish updating DNS before starting this project.");
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
        let _operation = self.operations.try_read().map_err(|_| anyhow::anyhow!("App reset is in progress."))?;
        anyhow::ensure!(!crate::separate_tunnel::is_running(id), "Finish creating the separate tunnel before restarting this project.");
        anyhow::ensure!(!crate::separate_tunnel::start_blocked(self, id).map_err(anyhow::Error::msg)?, "Separate tunnel setup is unfinished. Retry Create separate tunnel to finish updating DNS before restarting this project.");
        self.sup.restart(self.spec(id)?);
        Ok(())
    }

    /// Apply a changed tunnel definition to a running process.
    pub fn restart_if_running(&self, id: &str) -> anyhow::Result<()> {
        let _operation = self.operations.try_read().map_err(|_| anyhow::anyhow!("App reset is in progress."))?;
        anyhow::ensure!(!crate::separate_tunnel::is_running(id), "Finish creating the separate tunnel before restarting this project.");
        anyhow::ensure!(!crate::separate_tunnel::start_blocked(self, id).map_err(anyhow::Error::msg)?, "Separate tunnel setup is unfinished. Retry Create separate tunnel to finish updating DNS before restarting this project.");
        self.sup.restart_if_running(self.spec(id)?);
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

    /// Called under the data epoch write lock after all supervised runs have stopped.
    pub fn reset_runtime(&self) -> anyhow::Result<()> {
        self.states.lock().unwrap().clear();
        self.latest_run.lock().unwrap().clear();
        self.request_capture.tunnels.write().unwrap().clear();
        self.dropped.store(0, Ordering::Relaxed);
        self.written_since_sweep.store(0, Ordering::Relaxed);
        *self.activity.lock().unwrap() = None;
        self.apply_settings(Settings::default())?;
        tray::set_count(&self.handle, 0);
        Ok(())
    }

    pub async fn rediscover(&self) -> Option<Binary> {
        let path = self.settings.read().unwrap().cloudflared_path.clone();
        let b = discover_binary(&self.data_dir, path.as_deref()).await;
        *self.sup.config().cloudflared.write().unwrap() = b.as_ref().map(|b| PathBuf::from(&b.path));
        *self.binary.write().unwrap() = b.clone();
        self.notify_changed();
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

fn store_batch(
    db: &Db,
    request_capture: &RequestCapture,
    data_epoch: &RwLock<u64>,
    batch_logs: Vec<(u64, LogLine)>,
    batch_reqs: Vec<(u64, CapturedExchange, u64)>,
) -> (Vec<LogLine>, Vec<(RequestSummary, u64)>, usize) {
    // Reset takes the write lock before clearing storage. Old queued batches
    // cannot write into the fresh database, even if they run after reset.
    let epoch = data_epoch.read().unwrap();
    let tunnels: HashSet<_> = db.tunnels().unwrap_or_default().into_iter().map(|tunnel| tunnel.id).collect();
    let batch_logs: Vec<_> = batch_logs.into_iter().filter(|(queued, log)| *queued == *epoch && tunnels.contains(&log.tunnel_id)).map(|(_, log)| log).collect();
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
    for (queued_epoch, x, generation) in batch_reqs {
        bytes += record_bytes(&x);
        if queued_epoch != *epoch || !tunnels.contains(&x.tunnel_id) { continue; }
        if let Some(summary) = request_capture.store(&db, &x, generation) {
            out_reqs.push((summary, generation));
        }
    }
    (out_logs, out_reqs, bytes)
}

/// Batch writes into SQLite and forward what was stored to the UI.
async fn writer_task(core: Arc<Core>, mut rx: mpsc::Receiver<(u64, Write)>) {
    let mut logs: Vec<(u64, LogLine)> = Vec::new();
    let mut reqs: Vec<(u64, CapturedExchange, u64)> = Vec::new();
    loop {
        let Some(first) = rx.recv().await else { break };
        let deadline = tokio::time::Instant::now() + Duration::from_millis(150);
        let mut next = Some(first);
        while let Some((epoch, w)) = next.take() {
            match w {
                Write::Log(l) => logs.push((epoch, l)),
                Write::Request(x, generation) => reqs.push((epoch, x, generation)),
            }
            if logs.len() + reqs.len() >= 2000 {
                break;
            }
            if let Ok(Some(w)) = tokio::time::timeout_at(deadline, rx.recv()).await {
                next = Some(w);
            }
        }
        let db = core.db.clone();
        let request_capture = core.request_capture.clone();
        let data_epoch = core.data_epoch.clone();
        let batch_logs = std::mem::take(&mut logs);
        let batch_reqs = std::mem::take(&mut reqs);
        let stored = tokio::task::spawn_blocking(move || {
            store_batch(&db, &request_capture, &data_epoch, batch_logs, batch_reqs)
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
            for (s, generation) in r {
                let capture = core.request_capture.tunnels.read().unwrap();
                if capture.get(&s.tunnel_id).map(|s| !s.paused && s.generation == generation).unwrap_or(generation == 0) {
                    let _ = core.handle.emit("request", &s);
                }
            }
        }
    }
}

async fn state_task(core: Arc<Core>, mut rx: mpsc::UnboundedReceiver<(u64, TunnelState)>) {
    while let Some((queued_epoch, s)) = rx.recv().await {
        let epoch = core.data_epoch.read().unwrap();
        if queued_epoch == *epoch && core.db.tunnel(&s.id).ok().flatten().is_some() {
            core.on_state(s);
        }
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
