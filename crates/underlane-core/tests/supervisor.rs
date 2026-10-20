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

async fn wait_project(states: &States, id: &str, predicate: impl Fn(&TunnelState) -> bool) -> TunnelState {
    for _ in 0..200 {
        if let Some(state) = states.lock().unwrap().iter().rev().find(|s| s.id == id && predicate(s)).cloned() {
            return state;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("project {id} never reached expected state: {:?}", states.lock().unwrap());
}

fn project_spec(f: &Fake, id: &str, identity: &str) -> RunSpec {
    let path = f.root.join(format!("{id}.yml"));
    std::fs::write(&path, format!("tunnel: {identity}\ningress:\n  - hostname: {id}.example.com\n    service: http://localhost:3000\n  - service: http_status:404\n")).unwrap();
    let mut t = tunnel(TunnelKind::Config);
    t.id = id.into();
    t.name = id.into();
    t.config_path = Some(path.to_string_lossy().into_owned());
    RunSpec { tunnel: t, token: None }
}

fn tunnel_token(identity: &str) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(format!(r#"{{"a":"invented-account","s":"aW52ZW50ZWQtc3VwZXJ2aXNvci10b2tlbg==","t":"{identity}"}}"#))
}

#[tokio::test]
async fn a_second_project_cannot_start_the_same_cloudflare_tunnel() {
    let f = fake("ok");
    let (sup, states, _) = supervisor(&f, |_| {});
    let identity = uuid::Uuid::new_v4().to_string();
    let first = project_spec(&f, "first", &identity);
    let second = project_spec(&f, "second", &identity.to_ascii_uppercase());
    sup.start(first);
    wait_project(&states, "first", |s| s.status == Status::Connected).await;
    sup.start(second.clone());
    let rejected = wait_project(&states, "second", |s| s.status == Status::Connected || (s.status == Status::Stopped && s.detail.is_some())).await;
    sup.stop_all().await;
    assert_eq!(rejected.status, Status::Stopped);
    assert!(!rejected.supervised);
    assert!(rejected.pid.is_none());
    assert!(rejected.detail.as_deref().unwrap().contains("first"));
    assert!(rejected.failure.is_some());
    let conflict = rejected.tunnel_conflict.unwrap();
    assert_eq!(conflict.tunnel_id, identity);
    assert_eq!(conflict.project_id, "first");
    assert_eq!(conflict.project_name, "first");
    assert!(!f.data.join("derived/second.yml").exists());

    sup.start(second);
    let connected = wait_project(&states, "second", |s| s.status == Status::Connected).await;
    assert!(connected.tunnel_conflict.is_none());
    assert!(connected.failure.is_none());
    sup.stop_all().await;
}

#[tokio::test]
async fn stopping_keeps_identity_until_the_process_group_is_gone() {
    let f = fake("ignore-term");
    let (sup, states, _) = supervisor(&f, |cfg| cfg.grace_period = Duration::from_millis(10));
    let identity = uuid::Uuid::new_v4().to_string();
    let id = identity.as_str();
    let first = project_spec(&f, "first", id);
    std::fs::write(first.tunnel.config_path.as_ref().unwrap(), format!("tunnel: {id}\nurl: http://localhost:3000\n")).unwrap();
    sup.start(first);
    wait_project(&states, "first", |s| s.pid.is_some()).await;
    for _ in 0..200 {
        if f.root.join("bin/pids.json").exists() { break; }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(f.root.join("bin/pids.json").exists(), "fake installed its slow-stop handler");
    let stopping = sup.clone();
    let stop = tokio::spawn(async move { stopping.stop("first").await });
    while sup.config_state("first").is_some() { tokio::task::yield_now().await; }
    let second = project_spec(&f, "second", id);
    sup.start(second.clone());
    let blocked = wait_project(&states, "second", |s| s.tunnel_conflict.is_some()).await;
    assert_eq!(blocked.tunnel_conflict.unwrap().project_id, "first");
    assert!(!stop.is_finished(), "second start was refused before slow stop completed");
    stop.await.unwrap();
    std::fs::write(f.root.join("bin/mode"), "ok").unwrap();
    sup.start(second);
    wait_project(&states, "second", |s| s.status == Status::Connected).await;
    sup.stop_all().await;
}

#[tokio::test]
async fn stopping_during_validation_releases_the_reservation_without_a_connector() {
    let f = fake("ok");
    let script = std::fs::read_to_string(&f.bin).unwrap().replace(
        "if \"ingress\" in args:\n    print(\"OK\"); sys.exit(0)",
        "if \"ingress\" in args:\n    open(os.path.join(here, 'validating'), 'w').close()\n    while not os.path.exists(os.path.join(here, 'release-validation')): time.sleep(0.01)\n    print(\"OK\"); sys.exit(0)",
    );
    std::fs::write(&f.bin, script).unwrap();
    let (sup, states, _) = supervisor(&f, |_| {});
    let identity = uuid::Uuid::new_v4().to_string();
    let id = identity.as_str();
    sup.start(project_spec(&f, "first", id));
    for _ in 0..200 {
        if f.root.join("bin/validating").exists() { break; }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(f.root.join("bin/validating").exists());
    sup.stop("first").await;
    assert!(!f.data.join("derived/first.yml").exists());
    std::fs::write(f.root.join("bin/release-validation"), "").unwrap();
    sup.start(project_spec(&f, "second", id));
    wait_project(&states, "second", |s| s.status == Status::Connected).await;
    sup.stop_all().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_projects_reserve_one_connector_atomically() {
    let f = fake("ok");
    let (sup, states, _) = supervisor(&f, |_| {});
    let identity = uuid::Uuid::new_v4().to_string();
    let a = project_spec(&f, "a", &identity);
    let b = project_spec(&f, "b", &identity);
    let start_a = sup.clone();
    let start_b = sup.clone();
    let (a_start, b_start) = tokio::join!(
        tokio::spawn(async move { start_a.start(a); }),
        tokio::spawn(async move { start_b.start(b); }),
    );
    a_start.unwrap();
    b_start.unwrap();
    let terminal = |s: &TunnelState| s.status == Status::Connected || s.tunnel_conflict.is_some();
    let a = wait_project(&states, "a", terminal).await;
    let b = wait_project(&states, "b", terminal).await;
    sup.stop_all().await;
    assert_eq!([&a, &b].into_iter().filter(|s| s.status == Status::Connected).count(), 1);
    assert_eq!([&a, &b].into_iter().filter(|s| s.tunnel_conflict.is_some()).count(), 1);
    assert_eq!([&a, &b].into_iter().filter(|s| s.pid.is_some()).count(), 1);
}

#[tokio::test]
async fn source_edits_do_not_release_runtime_identity_and_conflicting_restart_releases_it() {
    let f = fake("ok");
    let (sup, states, _) = supervisor(&f, |_| {});
    let original_identity = uuid::Uuid::new_v4().to_string();
    let other_identity = uuid::Uuid::new_v4().to_string();
    let original_id = original_identity.as_str();
    let other_id = other_identity.as_str();
    let first = project_spec(&f, "first", original_id);
    let second = project_spec(&f, "second", other_id);
    let third = project_spec(&f, "third", original_id);
    sup.start(first.clone());
    let original = wait_project(&states, "first", |s| s.status == Status::Connected).await;
    sup.start(second);
    let other = wait_project(&states, "second", |s| s.status == Status::Connected).await;
    let path = first.tunnel.config_path.as_ref().unwrap();
    let text = std::fs::read_to_string(path).unwrap().replace(original_id, other_id);
    std::fs::write(path, text).unwrap();
    sup.start(third.clone());
    let blocked = wait_project(&states, "third", |s| s.tunnel_conflict.is_some()).await;
    assert_eq!(blocked.tunnel_conflict.unwrap().project_id, "first");
    assert_eq!(sup.config_state("first").unwrap().cloudflare_tunnel_id.as_deref(), Some(original_id));
    sup.restart(first);
    let blocked = wait_project(&states, "first", |s| s.tunnel_conflict.is_some()).await;
    assert_eq!(blocked.status, Status::Stopped);
    assert_eq!(blocked.tunnel_conflict.unwrap().project_id, "second");
    assert_eq!(blocked.config_revision, original.config_revision);
    assert!(blocked.cloudflare_tunnel_id.is_none());
    assert_eq!(sup.config_state("second").unwrap().pid, other.pid);
    sup.start(third);
    wait_project(&states, "third", |s| s.status == Status::Connected).await;
    sup.stop_all().await;
}

#[tokio::test]
async fn retrying_and_exhausted_runs_keep_identity_until_stopped() {
    let f = fake("crash");
    let (sup, states, _) = supervisor(&f, |cfg| {
        cfg.max_failures = 2;
        cfg.backoff_base = Duration::from_millis(400);
        cfg.backoff_max = Duration::from_millis(400);
    });
    let identity = uuid::Uuid::new_v4().to_string();
    let id = identity.as_str();
    sup.start(project_spec(&f, "first", id));
    wait_project(&states, "first", |s| s.status == Status::Crashed && s.config_revision == 1).await;
    let second = project_spec(&f, "second", id);
    sup.start(second.clone());
    let first_rejection = wait_project(&states, "second", |s| s.tunnel_conflict.is_some()).await;
    let exhausted = wait_project(&states, "first", |s| s.status == Status::Crashed && s.config_revision == 2).await;
    assert_eq!(exhausted.cloudflare_tunnel_id.as_deref(), Some(id));
    sup.start(second.clone());
    wait_project(&states, "second", |s| s.run > first_rejection.run && s.tunnel_conflict.is_some()).await;
    sup.stop("first").await;
    std::fs::write(f.root.join("bin/mode"), "ok").unwrap();
    sup.start(second);
    wait_project(&states, "second", |s| s.status == Status::Connected).await;
    sup.stop_all().await;
}

#[tokio::test]
async fn credential_names_and_all_token_forms_share_the_resolved_uuid() {
    use underlane_core::secret::Secret;
    let f = fake("ok");
    let (sup, states, _) = supervisor(&f, |_| {});
    let identity = uuid::Uuid::new_v4().to_string();
    let id = identity.as_str();
    sup.start(project_spec(&f, "owner", id));
    wait_project(&states, "owner", |s| s.status == Status::Connected).await;
    std::fs::write(f.root.join("credentials.json"), format!(r#"{{"TunnelID":"{id}","TunnelSecret":"invented-credentials-secret"}}"#)).unwrap();
    let named = project_spec(&f, "named", "friendly-name");
    let path = named.tunnel.config_path.as_ref().unwrap();
    let source = std::fs::read_to_string(path).unwrap();
    std::fs::write(path, format!("{source}credentials-file: credentials.json\n")).unwrap();
    sup.start(named.clone());
    let blocked = wait_project(&states, "named", |s| s.tunnel_conflict.is_some()).await;
    assert_eq!(blocked.tunnel_conflict.unwrap().project_id, "owner");

    let token = tunnel_token(id);
    let mut remote = tunnel(TunnelKind::Token);
    remote.id = "remote".into();
    sup.start(RunSpec { tunnel: remote, token: Some(Secret::new(token.clone())) });
    wait_project(&states, "remote", |s| s.tunnel_conflict.is_some()).await;
    std::fs::write(f.root.join("token.txt"), &token).unwrap();
    for (project, authentication) in [("inline", format!("token: {token}\n")), ("file", "token-file: token.txt\n".into())] {
        let spec = project_spec(&f, project, &uuid::Uuid::new_v4().to_string());
        let path = spec.tunnel.config_path.as_ref().unwrap();
        let source = std::fs::read_to_string(path).unwrap();
        std::fs::write(path, format!("{source}{authentication}")).unwrap();
        sup.start(spec);
        let blocked = wait_project(&states, project, |s| s.tunnel_conflict.is_some()).await;
        assert_eq!(blocked.tunnel_conflict.as_ref().unwrap().tunnel_id, id, "token authentication takes precedence over tunnel");
        assert!(!serde_json::to_string(&blocked).unwrap().contains(&token));
    }
    sup.stop("owner").await;
    sup.start(named);
    let connected = wait_project(&states, "named", |s| s.status == Status::Connected).await;
    assert_eq!(connected.cloudflare_tunnel_id.as_deref(), Some(id));
    assert_eq!(argv(&f).0.last().map(String::as_str), Some(id), "pin the credential UUID instead of leaving name resolution to a later file read");
    sup.stop_all().await;
}

#[tokio::test]
async fn unresolved_names_and_invalid_tokens_never_start_cloudflared() {
    use underlane_core::secret::Secret;
    for name in ["bare-name", "invalid-token"] {
        let f = fake("ok");
        let (sup, states, _) = supervisor(&f, |_| {});
        let spec = if name == "bare-name" { project_spec(&f, name, "friendly-name") } else {
            let mut remote = tunnel(TunnelKind::Token);
            remote.id = name.into();
            RunSpec { tunnel: remote, token: Some(Secret::new("invented-malformed-secret".into())) }
        };
        sup.start(spec);
        let failed = wait_project(&states, name, |s| s.status == Status::Crashed).await;
        assert!(failed.failure.is_some());
        assert!(failed.pid.is_none());
        assert!(!f.root.join("bin/argv.json").exists());
        assert!(!serde_json::to_string(&failed).unwrap().contains("invented-malformed-secret"));
        sup.stop_all().await;
    }
}

#[test]
fn identity_resolution_rejects_nil_and_mismatched_credentials() {
    use underlane_core::{config::TunnelConfig, supervisor::config_tunnel_identity};
    let f = fake("ok");
    std::fs::write(f.root.join("credentials.json"), r#"{"TunnelID":"00000000-0000-4000-8000-000000000001"}"#).unwrap();
    let path = f.root.join("source.yml");
    for identity in ["00000000-0000-0000-0000-000000000000", "00000000-0000-4000-8000-000000000002"] {
        let config = TunnelConfig::parse(&path, &format!("tunnel: {identity}\ncredentials-file: credentials.json\nurl: http://localhost:3000\n")).unwrap();
        assert!(config_tunnel_identity(&config).is_err(), "{identity} must not be replaced by an unrelated credential identity");
    }
    let unnamed = TunnelConfig::parse(&path, "url: http://localhost:3000\n").unwrap();
    assert_eq!(config_tunnel_identity(&unnamed).unwrap(), None);
}

#[tokio::test]
async fn token_file_edits_during_validation_do_not_change_reserved_launch_identity() {
    let f = fake("ok");
    let script = std::fs::read_to_string(&f.bin).unwrap()
        .replace("import json,", "import base64, json,")
        .replace("args = sys.argv[1:]", "args = sys.argv[1:]\nif 'run' in args and os.environ.get('TUNNEL_TOKEN'):\n    with open(os.path.join(here, 'seen-token-tunnel'), 'w') as f: f.write(json.loads(base64.b64decode(os.environ['TUNNEL_TOKEN']))['t'])\n    with open(os.path.join(here, 'seen-token-trimmed'), 'w') as f: f.write('yes' if os.environ['TUNNEL_TOKEN'] == os.environ['TUNNEL_TOKEN'].strip() else 'no')")
        .replace("if \"ingress\" in args:\n    print(\"OK\"); sys.exit(0)", "if \"ingress\" in args:\n    open(os.path.join(here, 'validating'), 'w').close()\n    while not os.path.exists(os.path.join(here, 'release-validation')): time.sleep(0.01)\n    print(\"OK\"); sys.exit(0)");
    std::fs::write(&f.bin, script).unwrap();
    let original_identity = uuid::Uuid::new_v4().to_string();
    let original_id = original_identity.as_str();
    std::fs::write(f.root.join("token.txt"), format!(" \t{}\r\n", tunnel_token(original_id))).unwrap();
    let spec = project_spec(&f, "file", &uuid::Uuid::new_v4().to_string());
    let path = spec.tunnel.config_path.as_ref().unwrap();
    let source = std::fs::read_to_string(path).unwrap();
    std::fs::write(path, format!("{source}token-file: token.txt\n")).unwrap();
    let (sup, states, _) = supervisor(&f, |_| {});
    sup.start(spec);
    for _ in 0..200 {
        if f.root.join("bin/validating").exists() { break; }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(f.root.join("bin/validating").exists());
    std::fs::write(f.root.join("token.txt"), tunnel_token(&uuid::Uuid::new_v4().to_string())).unwrap();
    std::fs::write(f.root.join("bin/release-validation"), "").unwrap();
    let loaded = wait_project(&states, "file", |s| s.status == Status::Connected).await;
    sup.stop_all().await;
    assert_eq!(loaded.cloudflare_tunnel_id.as_deref(), Some(original_id));
    assert_eq!(std::fs::read_to_string(f.root.join("bin/seen-token-tunnel")).unwrap(), original_id);
    assert_eq!(std::fs::read_to_string(f.root.join("bin/seen-token-trimmed")).unwrap(), "yes", "token-file whitespace is trimmed before passing the token to cloudflared");
    let derived = std::fs::read_to_string(f.data.join("derived/file.yml")).unwrap();
    assert!(!derived.contains("token-file:"));
    assert!(!derived.contains("token:"));
}

#[tokio::test]
async fn restart_session_snapshot_excludes_stopping_and_keeps_starting_running_and_retrying() {
    let f = fake("ignore-term");
    let (sup, states, _) = supervisor(&f, |cfg| {
        cfg.grace_period = Duration::from_millis(10);
        cfg.backoff_base = Duration::from_secs(10);
        cfg.backoff_max = Duration::from_secs(10);
    });
    let spec = |id: &str| {
        let mut t = tunnel(TunnelKind::Quick);
        t.id = id.into();
        RunSpec { tunnel: t, token: None }
    };
    sup.start(spec("stopping"));
    wait_project(&states, "stopping", |s| s.pid.is_some()).await;
    for _ in 0..200 {
        if f.root.join("bin/pids.json").exists() { break; }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(f.root.join("bin/pids.json").exists(), "fake installed its slow-stop handler");
    std::fs::write(f.root.join("bin/mode"), "ok").unwrap();
    sup.start(spec("running"));
    wait_project(&states, "running", |s| s.status == Status::Connected).await;
    std::fs::write(f.root.join("bin/mode"), "crash").unwrap();
    sup.start(spec("retrying"));
    wait_project(&states, "retrying", |s| s.status == Status::Crashed && s.detail.as_deref().is_some_and(|d| d.starts_with("Restarting in"))).await;

    let stopping = sup.clone();
    let stop = tokio::spawn(async move { stopping.stop("stopping").await });
    tokio::time::timeout(Duration::from_secs(2), async {
        while sup.config_state("stopping").is_some() { tokio::task::yield_now().await; }
    }).await.unwrap();
    assert!(!stop.is_finished(), "snapshot is taken while Stop is still in flight");
    assert!(sup.is_running("stopping"), "process lifetime semantics remain unchanged");
    sup.start(spec("starting"));
    let mut ids = sup.restart_session_ids();
    ids.sort();
    assert_eq!(ids, ["retrying", "running", "starting"]);
    assert!(sup.running_ids().contains(&"stopping".into()), "shutdown must still wait for stopping processes");
    stop.await.unwrap();
    sup.stop_all().await;
}

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
    let token = tunnel_token(&uuid::Uuid::new_v4().to_string());
    sup.start(RunSpec { tunnel: tunnel(TunnelKind::Token), token: Some(Secret::new(token.clone())) });
    wait_status(&states, Status::Connected).await;
    sup.stop("tun-1").await;
    let logs = db.query_logs(&LogQuery { limit: 100, ..Default::default() }).unwrap();
    assert!(logs.iter().any(|log| log.message.contains("synthetic child echoed [redacted]")));
    assert!(!format!("{logs:?} {states:?}").contains("invented-supervisor-token"));
    assert!(!format!("{logs:?} {states:?}").contains(&token));
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
async fn first_hostname_in_large_project_starts_promptly() {
    use underlane_core::config::TunnelConfig;
    use underlane_core::route_control::{RouteKey, RouteTarget};

    let f = fake("ok");
    let mut source = format!("tunnel: {}\ningress:\n", uuid::Uuid::new_v4());
    for index in 0..12 {
        source.push_str(&format!("  - hostname: app{index}.example.com\n    service: http://localhost:3000\n"));
    }
    source.push_str("  - service: http_status:404\n");
    let spec = config_spec(&f, &source);
    let config = TunnelConfig::load(Path::new(spec.tunnel.config_path.as_ref().unwrap())).unwrap();
    let (sup, states, _) = supervisor(&f, |cfg| cfg.health_interval = Duration::from_secs(2));
    let started = std::time::Instant::now();
    sup.start_route(spec.clone(), RouteTarget {
        key: RouteKey::from_rule(&config.ingress[0]).unwrap(), run: 0, config_revision: 0,
    }, |_| Ok(())).unwrap();
    let first = wait_status(&states, Status::Connected).await;
    let first_elapsed = started.elapsed();
    assert!(first.mappings[0].enabled);
    assert!(first.mappings[1..12].iter().all(|mapping| !mapping.enabled));

    let started = std::time::Instant::now();
    sup.start_route(spec, RouteTarget {
        key: RouteKey::from_rule(&config.ingress[1]).unwrap(), run: first.run, config_revision: first.config_revision,
    }, |_| Ok(())).unwrap();
    let second = sup.config_state("tun-1").unwrap();
    let second_elapsed = started.elapsed();
    sup.stop_all().await;
    eprintln!("first hostname: {first_elapsed:?}; second hostname: {second_elapsed:?}");
    assert_eq!((second.pid, second.run, second.config_revision), (first.pid, first.run, first.config_revision));
    assert!(second.mappings[0].enabled && second.mappings[1].enabled);
    assert!(first_elapsed < Duration::from_millis(1500), "first hostname startup took {first_elapsed:?}");
    assert!(second_elapsed < Duration::from_millis(1500));
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
    let original = format!("tunnel: {}\ningress:\n  - service: http://localhost:4000\n", uuid::Uuid::new_v4());
    let spec = config_spec(&f, &original);
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
    let original = format!("tunnel: {}\ningress:\n  - service: http://localhost:4000\n", uuid::Uuid::new_v4());
    let spec = config_spec(&f, &original);
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
    let derived = underlane_core::config::TunnelConfig::load(&f.data.join("derived/tun-1.yml")).unwrap();
    assert!(derived.ingress[0].service.starts_with("http://127.0.0.1:"));
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
    let id = uuid::Uuid::new_v4();
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
