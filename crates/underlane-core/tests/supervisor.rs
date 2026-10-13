//! Supervisor tests against a fake `cloudflared` (a Python script that mimics its CLI, logs,
//! metrics server and signal handling).

use std::path::Path;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use underlane_core::model::{Tunnel, TunnelKind};
use underlane_core::proxy::capture::CaptureOptions;
use underlane_core::supervisor::{reap_orphans, ConfigChangeStatus, Event, RunSpec, Status, Supervisor, SupervisorConfig, TunnelState};

#[path = "common/fake_cloudflared.rs"]
mod fake_cloudflared;
use fake_cloudflared::{argv, fake, Fake};

type States = Arc<Mutex<Vec<TunnelState>>>;

#[tokio::test]
async fn applying_edits_never_starts_a_stopped_or_stopping_tunnel() {
    let f = fake("ignore-term");
    let (sup, states, _) = supervisor(&f, |cfg| cfg.grace_period = Duration::from_millis(10));
    let spec = RunSpec { tunnel: tunnel(TunnelKind::Quick), token: None };
    assert!(!sup.restart_if_running(spec.clone()));
    assert!(!sup.is_running("tun-1"));
    sup.start(spec.clone());
    wait_state(&states, |s| s.pid.is_some()).await;
    // Wait for the fake connector to install its signal handler before stopping.
    for _ in 0..100 {
        if f.root.join("bin/pids.json").exists() { break; }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let stopping_sup = sup.clone();
    let stop = tokio::spawn(async move { stopping_sup.stop("tun-1").await });
    while sup.config_state("tun-1").is_some() { tokio::task::yield_now().await; }
    assert!(!sup.restart_if_running(spec.clone()));
    stop.await.unwrap();
    assert!(!sup.restart_if_running(spec));
    assert!(!sup.is_running("tun-1"));
}

#[tokio::test]
async fn token_echo_is_redacted_before_persisted_logs() {
    use underlane_core::db::{Db, LogQuery};
    use underlane_core::secret::Secret;
    let f = fake("token-echo");
    let states: States = Arc::new(Mutex::new(Vec::new()));
    let db = Arc::new(Db::open_in_memory().unwrap());
    let mut cfg = SupervisorConfig::new(f.data.clone(), Some(f.bin.clone()), Arc::new(RwLock::new(CaptureOptions::default())));
    cfg.health_interval = Duration::from_millis(100);
    let (state_sink, log_db) = (states.clone(), db.clone());
    let sup = Supervisor::new(cfg, Arc::new(move |event| match event {
        Event::State(s) => state_sink.lock().unwrap().push(s),
        Event::Log(log) => { log_db.insert_logs(&[log]).unwrap(); }
        Event::Request(_) => {}
    }));
    sup.start(RunSpec { tunnel: tunnel(TunnelKind::Token), token: Some(Secret::new("invented-supervisor-token".into())) });
    wait_status(&states, Status::Connected).await;
    sup.stop("tun-1").await;
    let logs = db.query_logs(&LogQuery { limit: 100, ..Default::default() }).unwrap();
    assert!(logs.iter().any(|log| log.message.contains("synthetic child echoed [redacted]")));
    assert!(!format!("{logs:?} {states:?}").contains("invented-supervisor-token"));
}

fn supervisor(f: &Fake, tune: impl FnOnce(&mut SupervisorConfig)) -> (Arc<Supervisor>, States, Arc<Mutex<usize>>) {
    let mut cfg = SupervisorConfig::new(f.data.clone(), Some(f.bin.clone()), Arc::new(RwLock::new(CaptureOptions::default())));
    cfg.health_interval = Duration::from_millis(100);
    cfg.backoff_base = Duration::from_millis(50);
    cfg.backoff_max = Duration::from_millis(100);
    tune(&mut cfg);
    let states: States = Arc::new(Mutex::new(Vec::new()));
    let logs = Arc::new(Mutex::new(0usize));
    let (s2, l2) = (states.clone(), logs.clone());
    let sup = Supervisor::new(
        cfg,
        Arc::new(move |e| match e {
            Event::State(s) => s2.lock().unwrap().push(s),
            Event::Log(_) => *l2.lock().unwrap() += 1,
            Event::Request(_) => {}
        }),
    );
    (sup, states, logs)
}

fn tunnel(kind: TunnelKind) -> Tunnel {
    Tunnel {
        id: "tun-1".into(),
        group_id: Some("p".into()),
        name: "web".into(),
        kind,
        config_path: None,
        origin_url: Some("http://localhost:3000".into()),
        inspect: false,
        autostart: false,
        loglevel: "info".into(),
        created_at: 0,
        sort: 0,
    }
}

async fn wait_status(states: &States, want: Status) -> TunnelState {
    for _ in 0..200 {
        if let Some(s) = states.lock().unwrap().iter().rev().find(|s| s.status == want) {
            return s.clone();
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("never reached {want:?}: {:#?}", states.lock().unwrap().iter().map(|s| (s.status, s.detail.clone())).collect::<Vec<_>>());
}

async fn wait_state(states: &States, predicate: impl Fn(&TunnelState) -> bool) -> TunnelState {
    for _ in 0..200 {
        if let Some(state) = states.lock().unwrap().last().filter(|s| predicate(s)).cloned() { return state; }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("state condition not reached: {:?}", states.lock().unwrap().last());
}

fn config_spec(f: &Fake, text: &str) -> RunSpec {
    let path = f.root.join("source.yml");
    std::fs::write(&path, text).unwrap();
    let mut t = tunnel(TunnelKind::Config);
    t.config_path = Some(path.to_string_lossy().into_owned());
    RunSpec { tunnel: t, token: None }
}

#[tokio::test]
async fn project_drafts_are_refused_before_starting_cloudflared() {
    let f = fake("ok");
    let spec = config_spec(&f, "ingress:\n  - service: http_status:404\n");
    let (sup, states, _) = supervisor(&f, |_| {});
    sup.start(spec);
    let state = wait_status(&states, Status::Crashed).await;
    assert!(state.detail.as_deref().unwrap().contains("tunnel ID"));
    assert!(state.hint.as_deref().unwrap().contains("Set up tunnel"));
    assert!(!f.root.join("bin/argv.json").exists(), "neither validation nor run may start cloudflared");
    assert!(!f.data.join("derived/tun-1.yml").exists());
    sup.stop("tun-1").await;
}

#[tokio::test]
async fn loaded_source_bytes_survive_edits_and_update_only_after_restart() {
    let f = fake("ok");
    let original = "url: http://localhost:4000\noriginRequest:\n  accessToken: invented-config-secret\n";
    let spec = config_spec(&f, original);
    let path = spec.tunnel.config_path.as_deref().unwrap();
    let (sup, states, _) = supervisor(&f, |_| {});
    sup.start(spec.clone());
    let loaded = wait_status(&states, Status::Connected).await;
    assert_eq!(loaded.config_revision, 1);
    assert_eq!(loaded.check_config_change(path).status, ConfigChangeStatus::Unchanged);
    let serialized = serde_json::to_string(&loaded).unwrap();
    assert!(!serialized.contains("loaded_config"));
    assert!(!serialized.contains("fingerprint"));
    assert!(!serialized.contains("invented-config-secret"));
    assert!(!format!("{loaded:?}").contains("invented-config-secret"));
    let derived = f.data.join("derived/tun-1.yml");
    let child_config = std::fs::read(&derived).unwrap();
    let original_argv = std::fs::read(f.root.join("bin/argv.json")).unwrap();

    std::fs::write(path, format!("{original}# comment-only change\n")).unwrap();
    assert_eq!(loaded.check_config_change(path).status, ConfigChangeStatus::Changed);
    std::fs::write(path, original).unwrap();
    assert_eq!(loaded.check_config_change(path).status, ConfigChangeStatus::Unchanged);
    let replacement = f.root.join("replacement.yml");
    let updated = original.replace("4000", "5000");
    std::fs::write(&replacement, &updated).unwrap();
    std::fs::rename(&replacement, path).unwrap();
    assert_eq!(loaded.check_config_change(path).status, ConfigChangeStatus::Changed);
    assert_eq!(std::fs::read(&derived).unwrap(), child_config, "running child still has A");
    assert_eq!(std::fs::read(f.root.join("bin/argv.json")).unwrap(), original_argv, "checks never invoke cloudflared");

    sup.restart(spec.clone());
    let restarted = wait_state(&states, |s| s.status == Status::Connected && s.config_revision == 2).await;
    assert_eq!(restarted.run, loaded.run, "restart stays in one supervisor task");
    assert_eq!(restarted.mappings[0].service, "http://localhost:5000");
    assert_eq!(restarted.check_config_change(path).status, ConfigChangeStatus::Unchanged);
    assert_eq!(std::fs::read_to_string(path).unwrap(), updated, "source stays read-only");
    sup.stop("tun-1").await;
    let stopped = states.lock().unwrap().last().unwrap().clone();
    assert!(stopped.loaded_config.is_none());
    assert_eq!(stopped.check_config_change(path).status, ConfigChangeStatus::NotRunning);
    assert!(sup.config_state("tun-1").is_none());
}

#[tokio::test]
async fn loaded_source_deletion_and_read_failure_are_unreadable() {
    let f = fake("ok");
    let original = "url: http://localhost:4000\n";
    let spec = config_spec(&f, original);
    let path = spec.tunnel.config_path.as_deref().unwrap();
    let (sup, states, _) = supervisor(&f, |_| {});
    sup.start(spec.clone());
    let loaded = wait_status(&states, Status::Connected).await;
    std::fs::remove_file(path).unwrap();
    let missing = loaded.check_config_change(path);
    assert_eq!(missing.status, ConfigChangeStatus::Unreadable);
    assert!(missing.message.is_some());
    std::fs::create_dir(path).unwrap();
    assert_eq!(loaded.check_config_change(path).status, ConfigChangeStatus::Unreadable);
    std::fs::remove_dir(path).unwrap();
    std::fs::write(path, original).unwrap();
    assert_eq!(loaded.check_config_change(path).status, ConfigChangeStatus::Unchanged);
    assert_eq!(sup.config_state("tun-1").unwrap().config_revision, 1);
    sup.stop("tun-1").await;
}

#[tokio::test]
async fn failed_validation_and_spawn_keep_the_previous_loaded_baseline() {
    let f = fake("ok");
    let original = "tunnel: test-id\ningress:\n  - service: http://localhost:4000\n";
    let spec = config_spec(&f, original);
    let path = spec.tunnel.config_path.as_deref().unwrap();
    let (sup, states, _) = supervisor(&f, |_| {});
    sup.start(spec.clone());
    let loaded = wait_status(&states, Status::Connected).await;
    std::fs::write(path, original.replace("4000", "5000")).unwrap();
    std::fs::write(f.root.join("bin/mode"), "invalid-config").unwrap();
    sup.restart(spec.clone());
    let failed = wait_state(&states, |s| s.status == Status::Crashed && s.failure.as_ref().is_some_and(|f| f.cause.contains("config"))).await;
    assert_eq!(failed.loaded_config, loaded.loaded_config);
    assert_eq!(failed.config_revision, loaded.config_revision);
    assert_eq!(failed.check_config_change(path).status, ConfigChangeStatus::Changed);
    assert_eq!(failed.mappings, loaded.mappings);

    // Single-origin preparation does not invoke validation, so a missing executable tests spawn.
    std::fs::write(path, "url: http://localhost:6000\n").unwrap();
    *sup.config().cloudflared.write().unwrap() = Some(f.root.join("missing-binary"));
    sup.restart(spec.clone());
    let failed = wait_state(&states, |s| s.status == Status::Crashed && s.failure.as_ref().is_some_and(|f| f.cause.contains("could not start"))).await;
    assert_eq!(failed.loaded_config, loaded.loaded_config);
    assert_eq!(failed.config_revision, loaded.config_revision);
    assert_eq!(failed.check_config_change(path).status, ConfigChangeStatus::Changed);
    assert_eq!(failed.mappings, loaded.mappings);
    sup.stop("tun-1").await;
}

#[tokio::test]
async fn edit_during_preparation_keeps_the_bytes_used_for_derived_config() {
    let f = fake("ok");
    let script = std::fs::read_to_string(&f.bin).unwrap().replace(
        "if \"ingress\" in args:\n    print(\"OK\"); sys.exit(0)",
        "if \"ingress\" in args:\n    open(os.path.join(here, 'validating'), 'w').close()\n    while not os.path.exists(os.path.join(here, 'release-validation')): time.sleep(0.01)\n    print(\"OK\"); sys.exit(0)",
    );
    std::fs::write(&f.bin, script).unwrap();
    let original = "tunnel: test-id\ningress:\n  - service: http://localhost:4000\n";
    let spec = config_spec(&f, original);
    let path = spec.tunnel.config_path.as_deref().unwrap();
    let (sup, states, _) = supervisor(&f, |_| {});
    sup.start(spec.clone());
    let marker = f.root.join("bin/validating");
    for _ in 0..200 {
        if marker.exists() { break; }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(marker.exists(), "validation delay reached after parsing A");
    std::fs::write(path, original.replace("4000", "5000")).unwrap();
    std::fs::write(f.root.join("bin/release-validation"), "").unwrap();
    let loaded = wait_status(&states, Status::Connected).await;
    assert_eq!(loaded.mappings[0].service, "http://localhost:4000");
    assert_eq!(loaded.check_config_change(path).status, ConfigChangeStatus::Changed);
    assert!(std::fs::read_to_string(f.data.join("derived/tun-1.yml")).unwrap().contains("4000"));
    assert_eq!(sup.config_state("tun-1").unwrap().loaded_config, loaded.loaded_config);
    sup.stop("tun-1").await;
}

#[tokio::test]
async fn internal_respawn_advances_load_revision_within_same_run() {
    let f = fake("crash");
    let spec = config_spec(&f, "url: http://localhost:4000\n");
    let (sup, states, _) = supervisor(&f, |c| c.max_failures = 3);
    sup.start(spec.clone());
    let state = wait_state(&states, |s| s.status == Status::Crashed && s.config_revision == 3).await;
    let mut loaded: Vec<_> = states.lock().unwrap().iter().filter(|s| s.pid.is_some()).cloned().collect();
    loaded.dedup_by_key(|s| s.config_revision);
    assert_eq!(loaded.iter().map(|s| s.config_revision).collect::<Vec<_>>(), vec![1, 2, 3]);
    assert!(loaded.iter().all(|s| s.run == state.run));
    assert_eq!(sup.config_state("tun-1").unwrap().config_revision, 3);
    sup.stop("tun-1").await;
}

#[tokio::test]
async fn quick_tunnel_connects_reports_url_and_stops() {
    let f = fake("ok");
    let (sup, states, logs) = supervisor(&f, |_| {});
    sup.start(RunSpec { tunnel: tunnel(TunnelKind::Quick), token: None });
    let s = wait_status(&states, Status::Connected).await;
    assert_eq!(s.connections, 1);
    let s = {
        let mut last = s;
        for _ in 0..40 {
            if last.public_url.is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
            last = states.lock().unwrap().last().unwrap().clone();
        }
        last
    };
    assert_eq!(s.public_url.as_deref(), Some("https://fake-words-here.trycloudflare.com"));
    assert_eq!(s.mappings[0].service, "http://localhost:3000");
    assert!(sup.is_running("tun-1"));
    assert!(f.data.join("run/tun-1.pid").exists());

    let (args, _) = argv(&f);
    let joined = args.join(" ");
    assert!(joined.contains("--metrics 127.0.0.1:0"), "{joined}");
    assert!(joined.contains("--grace-period 4s"), "{joined}");
    assert!(joined.contains("--output json"), "{joined}");
    assert!(joined.contains(&format!("--config {}", f.data.join("neutral.yml").display())), "neutral config keeps defaults away: {joined}");
    assert!(joined.ends_with("--url http://localhost:3000"), "{joined}");

    sup.stop("tun-1").await;
    assert_eq!(states.lock().unwrap().last().unwrap().status, Status::Stopped);
    assert!(!sup.is_running("tun-1"));
    assert!(!f.data.join("run/tun-1.pid").exists());
    assert!(*logs.lock().unwrap() >= 2);
}

#[tokio::test]
async fn crashing_tunnel_backs_off_then_gives_up() {
    let f = fake("crash");
    let (sup, states, _) = supervisor(&f, |c| c.max_failures = 3);
    sup.start(RunSpec { tunnel: tunnel(TunnelKind::Quick), token: None });
    let s = wait_status(&states, Status::Crashed).await;
    assert!(s.failure.as_ref().unwrap().raw.contains("failed to connect to the edge"), "{:?}", s.failure);
    for _ in 0..100 {
        if states.lock().unwrap().iter().any(|s| s.detail.as_deref().map(|d| d.starts_with("Gave up after 3 attempts")).unwrap_or(false)) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let details: Vec<String> = states.lock().unwrap().iter().filter_map(|s| s.detail.clone()).collect();
    assert!(details.iter().any(|d| d.contains("Restarting in")), "{details:#?}");
    assert!(details.iter().any(|d| d.starts_with("Gave up after 3 attempts")), "{details:#?}");
    let attempts = states.lock().unwrap().iter().filter_map(|s| s.pid).collect::<std::collections::HashSet<_>>().len();
    assert_eq!(attempts, 3);
    sup.stop("tun-1").await;
}

#[tokio::test]
async fn config_tunnel_runs_a_derived_copy_from_the_config_dir() {
    let f = fake("ok");
    let proj = f.root.join("project/.cloudflared");
    std::fs::create_dir_all(&proj).unwrap();
    let id = "00000000-0000-4000-8000-000000000101";
    std::fs::write(proj.join("creds.json"), format!(r#"{{"TunnelID":"{id}"}}"#)).unwrap();
    let original = format!("tunnel: {id}\ncredentials-file: creds.json\ningress:\n  - hostname: app.example.com\n    service: http://localhost:4000\n  - service: http_status:404\n");
    let cfg_path = proj.join("config.yml");
    std::fs::write(&cfg_path, &original).unwrap();

    let (sup, states, _) = supervisor(&f, |_| {});
    let mut t = tunnel(TunnelKind::Config);
    t.config_path = Some(cfg_path.to_string_lossy().into_owned());
    t.inspect = true;
    sup.start(RunSpec { tunnel: t, token: None });
    let s = wait_status(&states, Status::Connected).await;
    assert!(s.inspecting);
    assert!(s.mappings[0].inspected);
    assert_eq!(s.mappings[0].hostname.as_deref(), Some("app.example.com"));

    let (args, cwd) = argv(&f);
    let derived = f.data.join("derived/tun-1.yml");
    assert!(args.windows(2).any(|w| w[0] == "--config" && Path::new(&w[1]) == derived), "{args:?}");
    assert_eq!(Path::new(&cwd).canonicalize().unwrap(), proj.canonicalize().unwrap(), "runs from the config dir");
    let text = std::fs::read_to_string(&derived).unwrap();
    assert!(text.contains("service: http://127.0.0.1:"), "{text}");
    assert_eq!(std::fs::read_to_string(&cfg_path).unwrap(), original, "original untouched");
    sup.stop("tun-1").await;
}

#[tokio::test]
async fn invalid_config_is_reported_without_retrying() {
    let f = fake("ok");
    let cfg_path = f.root.join("broken.yml");
    std::fs::write(&cfg_path, "tunnel: x\ningress:\n  - hostname: a.com\n").unwrap();
    let (sup, states, _) = supervisor(&f, |_| {});
    let mut t = tunnel(TunnelKind::Config);
    t.config_path = Some(cfg_path.to_string_lossy().into_owned());
    sup.start(RunSpec { tunnel: t, token: None });
    let s = wait_status(&states, Status::Crashed).await;
    assert!(s.failure.unwrap().raw.contains("has no service"));
    tokio::time::sleep(Duration::from_millis(300)).await;
    let starts = states.lock().unwrap().iter().filter(|s| s.status == Status::Starting).count();
    assert_eq!(starts, 1, "no retry loop for a bad config");
    assert!(states.lock().unwrap().iter().all(|s| s.pid.is_none()), "cloudflared never spawned");
    sup.stop("tun-1").await;
}

#[tokio::test]
async fn shim_kills_cloudflared_when_the_app_goes_away() {
    use std::io::Read;
    use std::process::{Command, Stdio};
    let f = fake("ok");
    let shim = env!("CARGO_BIN_EXE_underlane-shim");
    let mut child = Command::new(shim)
        .arg("--cloudflared-shim")
        .arg(&f.bin)
        .args(["tunnel", "--url", "http://localhost:1"])
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut err = child.stderr.take().unwrap();
    let mut buf = [0u8; 256];
    let n = err.read(&mut buf).unwrap();
    assert!(String::from_utf8_lossy(&buf[..n]).contains("Starting"), "fake started");
    // Closing stdin is what happens when the app process dies.
    drop(child.stdin.take());
    let start = std::time::Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert_eq!(status.code(), Some(0), "fake exited via its SIGTERM handler");
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(8), "shim did not stop cloudflared");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn reaps_orphans_marked_with_the_data_dir() {
    use std::process::{Command, Stdio};
    let f = fake("ok");
    std::fs::create_dir_all(f.data.join("run")).unwrap();
    // A leftover "cloudflared" whose command line references the data dir.
    let mut orphan = Command::new(&f.bin)
        .args(["tunnel", "--config", &f.data.join("neutral.yml").to_string_lossy()])
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(300));
    std::fs::write(f.data.join("run/x.pid"), orphan.id().to_string()).unwrap();
    // An unrelated process with a stale pidfile must not be touched.
    let mut unrelated = Command::new("sleep").arg("30").spawn().unwrap();
    std::fs::write(f.data.join("run/y.pid"), unrelated.id().to_string()).unwrap();

    let killed = reap_orphans(&f.data);
    assert_eq!(killed, vec![orphan.id()]);
    let status = orphan.wait().unwrap();
    assert!(status.code() == Some(0) || status.code().is_none());
    assert!(unrelated.try_wait().unwrap().is_none(), "unrelated process left alone");
    unrelated.kill().unwrap();
    assert!(std::fs::read_dir(f.data.join("run")).unwrap().next().is_none(), "pidfiles cleaned");
}

#[test]
fn can_start_from_a_thread_outside_the_runtime() {
    // App setup and tray menu callbacks run on the main thread, outside Tokio.
    let rt = tokio::runtime::Runtime::new().unwrap();
    let f = fake("ok");
    let (sup, states, _) = rt.block_on(async { supervisor(&f, |_| {}) });
    std::thread::spawn({
        let sup = sup.clone();
        move || sup.start(RunSpec { tunnel: tunnel(TunnelKind::Quick), token: None })
    })
    .join()
    .unwrap();
    rt.block_on(async {
        wait_status(&states, Status::Connected).await;
        sup.stop("tun-1").await;
    });
}

#[tokio::test]
async fn stop_then_immediate_start_does_not_overlap() {
    let f = fake("ok");
    let (sup, states, _) = supervisor(&f, |_| {});
    let spec = || RunSpec { tunnel: tunnel(TunnelKind::Quick), token: None };
    sup.start(spec());
    wait_status(&states, Status::Connected).await;
    let first_pid = states.lock().unwrap().iter().rev().find_map(|s| s.pid).unwrap();
    // Stop and start again without waiting: the new run must begin only after the old one exits.
    let stopper = { let sup = sup.clone(); tokio::spawn(async move { sup.stop("tun-1").await }) };
    tokio::time::sleep(Duration::from_millis(20)).await;
    sup.start(spec());
    stopper.await.unwrap();
    for _ in 0..200 {
        let last = states.lock().unwrap().last().cloned().unwrap();
        if last.status == Status::Connected && last.pid.is_some() && last.pid != Some(first_pid) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let all = states.lock().unwrap().clone();
    let last = all.last().unwrap();
    assert_eq!(last.status, Status::Connected, "{:?}", all.iter().map(|s| (s.run, s.status)).collect::<Vec<_>>());
    assert!(last.run > all.iter().find(|s| s.pid == Some(first_pid)).unwrap().run, "newer run");
    // The first run's final Stopped was emitted before the new run started.
    let stopped_idx = all.iter().position(|s| s.status == Status::Stopped).unwrap();
    let second_start = all.iter().position(|s| s.run == last.run).unwrap();
    assert!(stopped_idx < second_start);
    assert_eq!(std::fs::read_to_string(f.data.join("run/tun-1.pid")).unwrap().trim(), last.pid.unwrap().to_string());
    sup.stop("tun-1").await;
}

#[tokio::test]
async fn killing_only_the_shim_does_not_orphan_cloudflared() {
    let f = fake("ok");
    let shim = std::path::PathBuf::from(env!("CARGO_BIN_EXE_underlane-shim"));
    let (sup, states, _) = supervisor(&f, |c| {
        c.shim = Some(shim.clone());
        c.max_failures = 1;
    });
    sup.start(RunSpec { tunnel: tunnel(TunnelKind::Quick), token: None });
    let s = wait_status(&states, Status::Connected).await;
    let shim_pid = s.pid.unwrap();
    // Find the fake cloudflared (child of the shim).
    let out = std::process::Command::new("pgrep").args(["-P", &shim_pid.to_string()]).output().unwrap();
    let cf_pid: i32 = String::from_utf8_lossy(&out.stdout).trim().parse().unwrap();
    unsafe { libc::kill(shim_pid as i32, libc::SIGKILL) };
    for _ in 0..100 {
        if unsafe { libc::kill(cf_pid, 0) } != 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_ne!(unsafe { libc::kill(cf_pid, 0) }, 0, "cloudflared behind a killed shim was cleaned up");
    sup.stop("tun-1").await;
}

#[tokio::test]
async fn runtime_known_unknown_and_rc_zero_failures_keep_safe_diagnostics_and_retry_controls() {
    for (mode, expected) in [
        ("runtime-known", "credentials"),
        ("runtime-unknown", "unrecognized"),
        ("unknown-flag", "command option"),
        ("login-refused", "certificate"),
    ] {
        let f = fake(mode);
        let (sup, states, _) = supervisor(&f, |cfg| cfg.max_failures = 1);
        let spec = RunSpec { tunnel: tunnel(TunnelKind::Quick), token: None };
        sup.start(spec.clone());
        let crashed = wait_status(&states, Status::Crashed).await;
        let failure = crashed.failure.as_ref().unwrap();
        assert!(failure.cause.contains(expected), "{mode}: {failure:?}");
        assert!(failure.action.is_some(), "{mode}: {failure:?}");
        assert!(failure.raw.contains("exit status"), "{failure:?}");
        assert!(!failure.raw.contains("Some("), "exit status is plain text: {failure:?}");
        assert!(!format!("{crashed:?}").contains("invented-"));
        assert!(crashed.supervised && crashed.pid.is_none());
        assert!(crashed.detail.unwrap().contains("Gave up"));
        std::fs::write(f.root.join("bin/mode"), "ok").unwrap();
        sup.restart(spec);
        let recovered = wait_status(&states, Status::Connected).await;
        assert!(recovered.failure.is_none() && recovered.hint.is_none());
        sup.stop("tun-1").await;
        assert!(states.lock().unwrap().last().unwrap().failure.is_none());
    }
}

#[tokio::test]
async fn runtime_warning_and_success_messages_never_create_explanations() {
    let f = fake("runtime-warning");
    let (sup, states, _) = supervisor(&f, |_| {});
    sup.start(RunSpec { tunnel: tunnel(TunnelKind::Quick), token: None });
    wait_status(&states, Status::Connected).await;
    tokio::time::sleep(Duration::from_millis(250)).await;
    sup.stop("tun-1").await;
    assert!(states.lock().unwrap().iter().all(|state| state.failure.is_none()));
}

#[tokio::test]
async fn readiness_clears_edge_failure_without_restart_but_not_an_origin_failure() {
    for mode in ["runtime-recover", "runtime-origin"] {
        let f = fake(mode);
        let (sup, states, _) = supervisor(&f, |_| {});
        sup.start(RunSpec { tunnel: tunnel(TunnelKind::Quick), token: None });
        let reported = wait_state(&states, |state| state.failure.is_some()).await;
        assert!(reported.pid.is_some());
        assert!(!format!("{reported:?}").contains("invented-"));
        if mode == "runtime-recover" {
            assert!(reported.failure.unwrap().cause.contains("edge"));
            std::fs::write(f.root.join("bin/mode"), "ok").unwrap();
            let recovered = wait_state(&states, |state| state.status == Status::Connected && state.failure.is_none()).await;
            assert_eq!(recovered.restarts, 0);
        } else {
            let connected = wait_state(&states, |state| state.status == Status::Connected && state.failure.is_some()).await;
            assert!(connected.failure.unwrap().cause.contains("origin"));
            tokio::time::sleep(Duration::from_millis(250)).await;
            assert!(states.lock().unwrap().last().unwrap().failure.is_some(), "edge readiness does not prove origin recovery");
        }
        sup.stop("tun-1").await;
        assert!(states.lock().unwrap().last().unwrap().failure.is_none());
    }
}

#[tokio::test]
async fn preparation_failures_explain_missing_config_credentials_token_and_binary() {
    for mode in ["config", "credentials", "token", "binary"] {
        let f = fake("ok");
        let (sup, states, _) = supervisor(&f, |_| {});
        let spec = match mode {
            "config" => {
                let mut t = tunnel(TunnelKind::Config);
                t.config_path = Some(f.root.join("unauthorized-429.yml").to_string_lossy().into_owned());
                RunSpec { tunnel: t, token: None }
            }
            "credentials" => config_spec(&f, "url: http://localhost:3000\ncredentials-file: missing.json\n"),
            "token" => RunSpec { tunnel: tunnel(TunnelKind::Token), token: None },
            _ => {
                *sup.config().cloudflared.write().unwrap() = None;
                RunSpec { tunnel: tunnel(TunnelKind::Quick), token: None }
            }
        };
        sup.start(spec);
        let crashed = wait_status(&states, Status::Crashed).await;
        let failure = crashed.failure.unwrap();
        assert!(failure.action.is_some(), "{mode}: {failure:?}");
        assert!(!failure.cause.contains("Cloudflare rejected"));
        assert!(!failure.raw.is_empty());
        assert!(crashed.pid.is_none());
        sup.stop("tun-1").await;
        assert!(states.lock().unwrap().last().unwrap().failure.is_none());
    }
}

#[tokio::test]
async fn runtime_error_survives_a_burst_of_info_logs_before_exit() {
    let f = fake("runtime-burst");
    let (sup, states, _) = supervisor(&f, |cfg| cfg.max_failures = 1);
    sup.start(RunSpec { tunnel: tunnel(TunnelKind::Quick), token: None });
    let crashed = wait_status(&states, Status::Crashed).await;
    let failure = crashed.failure.unwrap();
    assert!(failure.cause.contains("credentials"), "{failure:?}");
    assert!(failure.raw.contains("authentication failed: status 403"));
    assert!(failure.raw.contains("synthetic exit context 39"), "recent context retained");
    sup.stop("tun-1").await;
}
