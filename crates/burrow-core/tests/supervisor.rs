//! Supervisor tests against a fake `cloudflared` (a Python script that mimics its CLI, logs,
//! metrics server and signal handling).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use burrow_core::model::{Tunnel, TunnelKind};
use burrow_core::proxy::capture::CaptureOptions;
use burrow_core::supervisor::{reap_orphans, Event, RunSpec, Status, Supervisor, SupervisorConfig, TunnelState};

const FAKE: &str = r#"#!/usr/bin/env python3
import json, os, signal, sys, threading, time
from http.server import BaseHTTPRequestHandler, HTTPServer
here = os.path.dirname(os.path.abspath(__file__))
mode = open(os.path.join(here, "mode")).read().strip()
args = sys.argv[1:]
with open(os.path.join(here, "argv.json"), "w") as f:
    json.dump({"argv": args, "cwd": os.getcwd()}, f)
def log(level, msg):
    sys.stderr.write(json.dumps({"level": level, "message": msg, "time": "2026-10-10T06:00:00Z"}) + "\n")
    sys.stderr.flush()
if "ingress" in args:
    print("OK"); sys.exit(0)
if mode == "crash":
    log("error", "failed to connect to the edge"); sys.exit(1)
class H(BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_GET(self):
        if self.path == "/ready":
            body = {"status": 200, "readyConnections": 1}; code = 200
        elif self.path == "/quicktunnel":
            body = {"hostname": "fake-words-here.trycloudflare.com"}; code = 200
        else:
            body = {}; code = 404
        data = json.dumps(body).encode()
        self.send_response(code); self.send_header("Content-Length", str(len(data))); self.end_headers(); self.wfile.write(data)
srv = HTTPServer(("127.0.0.1", 0), H)
threading.Thread(target=srv.serve_forever, daemon=True).start()
def term(*a):
    log("info", "Initiating graceful shutdown due to signal terminated ...")
    os._exit(0)
signal.signal(signal.SIGTERM, term)
log("info", "Starting tunnel")
log("info", "Starting metrics server on 127.0.0.1:%d/metrics" % srv.server_address[1])
while True:
    time.sleep(0.2)
"#;

struct Fake {
    _dir: tempfile::TempDir,
    bin: PathBuf,
    data: PathBuf,
    root: PathBuf,
}

fn fake(mode: &str) -> Fake {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let bindir = root.join("bin");
    std::fs::create_dir_all(&bindir).unwrap();
    let bin = bindir.join("cloudflared");
    std::fs::write(&bin, FAKE).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    std::fs::write(bindir.join("mode"), mode).unwrap();
    let data = root.join("data");
    Fake { _dir: dir, bin, data, root }
}

type States = Arc<Mutex<Vec<TunnelState>>>;

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
        project_id: "p".into(),
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

fn argv(f: &Fake) -> (Vec<String>, String) {
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(f.root.join("bin/argv.json")).unwrap()).unwrap();
    (
        v["argv"].as_array().unwrap().iter().map(|a| a.as_str().unwrap().to_string()).collect(),
        v["cwd"].as_str().unwrap().to_string(),
    )
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
    assert!(s.detail.as_deref().unwrap().contains("failed to connect to the edge"), "{:?}", s.detail);
    for _ in 0..100 {
        if states.lock().unwrap().iter().any(|s| s.detail.as_deref().map(|d| d.starts_with("Gave up after 3 attempts")).unwrap_or(false)) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let details: Vec<String> = states.lock().unwrap().iter().filter_map(|s| s.detail.clone()).collect();
    assert!(details.iter().any(|d| d.contains("Restarting in")), "{details:#?}");
    assert!(details.iter().any(|d| d.starts_with("Gave up after 3 attempts")), "{details:#?}");
    let attempts = states.lock().unwrap().iter().filter(|s| s.status == Status::Starting && s.pid.is_some()).count();
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
    assert!(s.detail.unwrap().contains("has no service"));
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
    let shim = env!("CARGO_BIN_EXE_burrow-shim");
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
    let shim = std::path::PathBuf::from(env!("CARGO_BIN_EXE_burrow-shim"));
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
