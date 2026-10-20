mod common;
#[path = "common/fake_cloudflared.rs"]
mod fake_cloudflared;

use std::convert::Infallible;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use bytes::Bytes;
use http_body_util::{BodyExt, Empty, Full, StreamBody};
use hyper::body::Frame;
use hyper_util::client::legacy::Client;
use hyper_util::rt::{TokioExecutor, TokioIo};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use underlane_core::config::TunnelConfig;
use underlane_core::model::{Tunnel, TunnelKind};
use underlane_core::proxy::capture::CaptureOptions;
use underlane_core::route_control::{RouteGate, RouteKey, RouteSelection, RouteTarget};
use underlane_core::supervisor::{Event, RunSpec, Status, Supervisor, SupervisorConfig, TunnelState};

fn setup(fake: &fake_cloudflared::Fake, origins: &[String]) -> (Arc<Supervisor>, RunSpec, Arc<Mutex<usize>>) {
    let source = fake.root.join("source.yml");
    let mut text = format!("tunnel: {}\ningress:\n", uuid::Uuid::new_v4());
    for (name, origin) in ["a", "b", "c"].iter().zip(origins) {
        text.push_str(&format!("  - hostname: {name}.example.com\n    service: {origin}\n"));
    }
    text.push_str("  - service: http_status:404\n");
    std::fs::write(&source, text).unwrap();
    let tunnel = Tunnel { id: "project".into(), group_id: None, name: "Project".into(), kind: TunnelKind::Config,
        config_path: Some(source.to_string_lossy().into_owned()), origin_url: None, inspect: false,
        autostart: false, loglevel: "info".into(), created_at: 0, sort: 0 };
    let captured = Arc::new(Mutex::new(0));
    let count = captured.clone();
    let mut cfg = SupervisorConfig::new(fake.data.clone(), Some(fake.bin.clone()), Arc::new(RwLock::new(CaptureOptions::default())));
    cfg.health_interval = Duration::from_millis(20);
    cfg.backoff_base = Duration::from_millis(30);
    cfg.backoff_max = Duration::from_millis(30);
    cfg.grace_period = Duration::from_millis(10);
    cfg.max_failures = 1;
    let supervisor = Supervisor::new(cfg, Arc::new(move |event| {
        if matches!(event, Event::Request(_)) { *count.lock().unwrap() += 1; }
    }));
    (supervisor, RunSpec { tunnel, token: None }, captured)
}

fn key(spec: &RunSpec, index: usize) -> RouteKey {
    let config = TunnelConfig::load(std::path::Path::new(spec.tunnel.config_path.as_ref().unwrap())).unwrap();
    RouteKey::from_rule(&config.ingress[index]).unwrap()
}

fn target(state: &TunnelState, key: RouteKey) -> RouteTarget {
    RouteTarget { key, run: state.run, config_revision: state.config_revision }
}

async fn connected(supervisor: &Supervisor, revision: u64) -> TunnelState {
    for _ in 0..200 {
        if let Some(state) = supervisor.config_state("project").filter(|state| state.status == Status::Connected && state.config_revision >= revision) { return state; }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("project did not connect: {:?}", supervisor.config_state("project"));
}

fn ports(fake: &fake_cloudflared::Fake) -> Vec<u16> {
    let config = TunnelConfig::load(&fake.data.join("derived/project.yml")).unwrap();
    config.ingress.iter().filter_map(|rule| rule.service.strip_prefix("http://127.0.0.1:").and_then(|value| value.parse().ok())).collect()
}

async fn status(port: u16) -> (hyper::StatusCode, bool) {
    let client = Client::builder(TokioExecutor::new()).build_http::<Empty<Bytes>>();
    let response = client.get(format!("http://127.0.0.1:{port}/headers").parse().unwrap()).await.unwrap();
    let result = (response.status(), response.headers().get("cache-control").is_some_and(|v| v == "no-store"));
    response.into_body().collect().await.unwrap();
    result
}

async fn slow_sse() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            tokio::spawn(async move {
                let service = hyper::service::service_fn(|_| async {
                    let stream = futures_util::stream::unfold(0, |index| async move {
                        tokio::time::sleep(Duration::from_millis(40)).await;
                        Some((Ok::<_, Infallible>(Frame::data(Bytes::from(format!("data: {index}\n\n")))), index + 1))
                    });
                    let mut response = hyper::Response::new(StreamBody::new(Box::pin(stream)));
                    response.headers_mut().insert("content-type", "text/event-stream".parse().unwrap());
                    Ok::<_, Infallible>(response)
                });
                let _ = hyper::server::conn::http1::Builder::new().serve_connection(TokioIo::new(stream), service).await;
            });
        }
    });
    format!("http://{address}")
}

#[tokio::test]
async fn hostname_controls_keep_connector_websocket_sse_and_source_unchanged_without_capture() {
    let fake = fake_cloudflared::fake("ok");
    let origin = format!("http://{}", common::start_origin().await);
    let (supervisor, spec, captured) = setup(&fake, &[origin.clone(), origin, slow_sse().await]);
    let source = std::fs::read(spec.tunnel.config_path.as_ref().unwrap()).unwrap();
    let a = key(&spec, 0);
    let b = key(&spec, 1);
    let c = key(&spec, 2);
    supervisor.start_route(spec.clone(), RouteTarget { key: a.clone(), run: 0, config_revision: 0 }, |_| Ok(())).unwrap();
    let state = connected(&supervisor, 1).await;
    let ports = ports(&fake);
    assert_eq!(ports.len(), 3, "capture off still creates every HTTP gate");
    let derived = std::fs::read(fake.data.join("derived/project.yml")).unwrap();
    assert_eq!(status(ports[0]).await.0, 200);
    assert_eq!(status(ports[1]).await, (hyper::StatusCode::SERVICE_UNAVAILABLE, true));
    assert_eq!(status(ports[2]).await, (hyper::StatusCode::SERVICE_UNAVAILABLE, true));
    supervisor.start_route(spec.clone(), target(&state, c), |_| Ok(())).unwrap();
    let partial = supervisor.config_state("project").unwrap();
    assert!(partial.mappings[0].enabled && !partial.mappings[1].enabled && partial.mappings[2].enabled);
    assert_eq!(status(ports[1]).await.0, 503);
    let client = Client::builder(TokioExecutor::new()).build_http::<Empty<Bytes>>();
    let c_response = client.get(format!("http://127.0.0.1:{}/events", ports[2]).parse().unwrap()).await.unwrap();
    assert_eq!(c_response.status(), 200);
    drop(c_response);
    supervisor.start_all_routes(spec.clone(), |_| Ok(())).unwrap();
    assert_eq!(status(ports[1]).await.0, 200);

    let mut websocket = tokio::net::TcpStream::connect(("127.0.0.1", ports[0])).await.unwrap();
    websocket.write_all(b"GET /ws HTTP/1.1\r\nHost: a.example.com\r\nConnection: Upgrade\r\nUpgrade: websocket\r\n\r\n").await.unwrap();
    let mut head = Vec::new();
    while !head.ends_with(b"\r\n\r\n") { head.push(websocket.read_u8().await.unwrap()); }
    assert!(head.starts_with(b"HTTP/1.1 101"));
    let mut sse = client.get(format!("http://127.0.0.1:{}/events", ports[2]).parse().unwrap()).await.unwrap().into_body();
    assert!(sse.frame().await.unwrap().unwrap().data_ref().is_some());

    for _ in 0..3 {
        supervisor.stop_route("project", target(&state, b.clone()), |_| Ok(())).unwrap();
        assert_eq!(status(ports[1]).await.0, 503);
        supervisor.start_route(spec.clone(), target(&state, b.clone()), |_| Ok(())).unwrap();
        assert_eq!(status(ports[1]).await.0, 200);
    }
    supervisor.stop_route("project", target(&state, b), |_| Ok(())).unwrap();
    assert_eq!(status(ports[1]).await, (hyper::StatusCode::SERVICE_UNAVAILABLE, true));
    websocket.write_all(b"still connected").await.unwrap();
    let mut echoed = [0; 15];
    tokio::time::timeout(Duration::from_secs(1), websocket.read_exact(&mut echoed)).await.unwrap().unwrap();
    assert_eq!(&echoed, b"still connected");
    assert!(tokio::time::timeout(Duration::from_secs(1), sse.frame()).await.unwrap().unwrap().unwrap().data_ref().is_some());
    let current = supervisor.config_state("project").unwrap();
    assert_eq!((current.pid, current.run, current.config_revision, current.restarts), (state.pid, state.run, state.config_revision, state.restarts));
    assert!(!current.inspecting);
    assert!(current.mappings[0].enabled && !current.mappings[1].enabled && current.mappings[2].enabled);
    assert_eq!(std::fs::read(fake.data.join("derived/project.yml")).unwrap(), derived);
    assert_eq!(std::fs::read(spec.tunnel.config_path.as_ref().unwrap()).unwrap(), source);
    assert_eq!(*captured.lock().unwrap(), 0, "capture off produces no request records");
    supervisor.stop_route("project", target(&state, a.clone()), |_| Ok(())).unwrap();
    assert_eq!(status(ports[0]).await.0, 503);
    websocket.write_all(b"drain").await.unwrap();
    let mut drained = [0; 5];
    tokio::time::timeout(Duration::from_secs(1), websocket.read_exact(&mut drained)).await.unwrap().unwrap();
    assert_eq!(&drained, b"drain", "already upgraded requests drain after their own route stops");
    supervisor.start_route(spec.clone(), target(&state, a), |_| Ok(())).unwrap();
    drop(websocket);
    drop(sse);
    supervisor.stop_all().await;
}

#[tokio::test]
async fn persistence_failure_stale_targets_and_source_changes_leave_gates_unchanged() {
    let fake = fake_cloudflared::fake("ok");
    let origin = format!("http://{}", common::start_origin().await);
    let (supervisor, spec, _) = setup(&fake, &[origin.clone(), origin.clone(), origin]);
    let a = key(&spec, 0);
    assert!(supervisor.start_route(spec.clone(), RouteTarget { key: a.clone(), run: 0, config_revision: 0 }, |_| Err("disk full".into())).is_err());
    assert!(!supervisor.is_running("project"));
    supervisor.start(spec.clone());
    let state = connected(&supervisor, 1).await;
    let a_target = target(&state, a);
    assert!(supervisor.stop_route("project", a_target.clone(), |_| Err("disk full".into())).is_err());
    assert_eq!(supervisor.route_selection("project"), RouteSelection::All);
    let mut stale = a_target.clone();
    stale.config_revision += 1;
    assert!(supervisor.stop_route("project", stale, |_| panic!("stale operation must not persist")).is_err());
    let path = spec.tunnel.config_path.as_ref().unwrap();
    let source = std::fs::read_to_string(path).unwrap();
    std::fs::write(path, format!("{source}# external edit\n")).unwrap();
    assert!(supervisor.stop_route("project", a_target, |_| panic!("changed source must not persist")).unwrap_err().contains("source config changed"));
    assert_eq!(status(ports(&fake)[0]).await.0, 200);
    supervisor.stop_all().await;
}

#[tokio::test]
async fn token_authenticated_configs_reject_individual_controls_before_saving_or_spawning() {
    use base64::Engine;

    for token_file in [false, true] {
        let fake = fake_cloudflared::fake("ok");
        let (supervisor, spec, _) = setup(&fake, &vec!["http://localhost:3000".into(); 3]);
        let path = spec.tunnel.config_path.as_ref().unwrap();
        let source = std::fs::read_to_string(path).unwrap();
        let config = TunnelConfig::load(std::path::Path::new(path)).unwrap();
        let token = base64::engine::general_purpose::STANDARD.encode(format!(r#"{{"a":"invented-account","s":"aW52ZW50ZWQtdG9rZW4=","t":"{}"}}"#, config.tunnel.unwrap()));
        std::fs::write(fake.root.join("token.txt"), &token).unwrap();
        let auth = if token_file { "token-file: token.txt".to_string() } else { format!("token: {token}") };
        let source = format!("{source}{auth}\n");
        std::fs::write(path, &source).unwrap();
        let selected = key(&spec, 0);
        let error = supervisor.start_route(spec.clone(), RouteTarget { key: selected.clone(), run: 0, config_revision: 0 }, |_| panic!("remote routing must reject before persistence")).unwrap_err();
        assert!(error.contains("token-authenticated"));
        assert!(!supervisor.is_running("project"));
        assert!(!fake.data.join("derived/project.yml").exists());
        assert_eq!(supervisor.route_selection("project"), RouteSelection::All);
        supervisor.start(spec.clone());
        let running = connected(&supervisor, 1).await;
        assert!(running.mappings.iter().filter(|mapping| mapping.route_key.is_some()).all(|mapping| !mapping.control_supported && mapping.note.as_deref().is_some_and(|note| note.contains("token-authenticated"))));
        assert!(supervisor.stop_route("project", target(&running, selected), |_| panic!("remote routing must reject before persistence")).unwrap_err().contains("token-authenticated"));
        assert_eq!(supervisor.config_state("project").unwrap().pid, running.pid);
        assert_eq!(supervisor.route_selection("project"), RouteSelection::All);
        supervisor.stop_all().await;
        assert_eq!(std::fs::read_to_string(path).unwrap(), source);
    }
}

#[tokio::test]
async fn unsupported_origins_and_duplicate_rules_reject_selective_start_before_spawning() {
    for unsupported in ["http2Origin: true", "matchSNItoHost: true"] {
        let fake = fake_cloudflared::fake("ok");
        let origins = vec!["http://localhost:3000".into(); 3];
        let (supervisor, spec, _) = setup(&fake, &origins);
        let path = spec.tunnel.config_path.as_ref().unwrap();
        let source = std::fs::read_to_string(path).unwrap();
        std::fs::write(path, format!("originRequest:\n  {unsupported}\n{source}")).unwrap();
        assert!(supervisor.start_route(spec.clone(), RouteTarget { key: key(&spec, 0), run: 0, config_revision: 0 }, |_| panic!("unsupported selection must not persist")).is_err());
        assert!(!supervisor.is_running("project"));
        assert!(!fake.data.join("derived/project.yml").exists());
        supervisor.start(spec.clone());
        let state = connected(&supervisor, 1).await;
        assert!(!state.mappings[0].control_supported);
        assert!(supervisor.stop_route("project", target(&state, key(&spec, 0)), |_| panic!("unsupported stop must not persist")).is_err());
        assert_eq!(supervisor.route_selection("project"), RouteSelection::All);
        supervisor.stop_all().await;
    }
    for replacement in ["tcp://localhost:3000", "unix:/tmp/test.sock"] {
        let fake = fake_cloudflared::fake("ok");
        let (supervisor, spec, _) = setup(&fake, &["http://localhost:3000".into(), replacement.into(), "http://localhost:3000".into()]);
        assert!(supervisor.start_route(spec.clone(), RouteTarget { key: key(&spec, 0), run: 0, config_revision: 0 }, |_| Ok(())).is_err());
        assert!(!supervisor.is_running("project"));
    }
    let fake = fake_cloudflared::fake("ok");
    let (supervisor, spec, _) = setup(&fake, &vec!["http://localhost:3000".into(); 3]);
    let path = spec.tunnel.config_path.as_ref().unwrap();
    let source = std::fs::read_to_string(path).unwrap().replace("b.example.com", "a.example.com");
    std::fs::write(path, source).unwrap();
    assert!(supervisor.start_route(spec.clone(), RouteTarget { key: key(&spec, 0), run: 0, config_revision: 0 }, |_| Ok(())).unwrap_err().contains("duplicated"));
}

#[tokio::test]
async fn unnamed_http_siblings_reject_selective_operations_without_changing_running_routes() {
    let fake = fake_cloudflared::fake("ok");
    let origin = format!("http://{}", common::start_origin().await);
    let (supervisor, spec, _) = setup(&fake, &[origin.clone(), origin.clone(), origin]);
    let path = spec.tunnel.config_path.as_ref().unwrap();
    let source = std::fs::read_to_string(path).unwrap().replace("  - hostname: b.example.com\n", "  - path: /health\n");
    std::fs::write(path, source).unwrap();
    let a = key(&spec, 0);
    assert!(supervisor.start_route(spec.clone(), RouteTarget { key: a.clone(), run: 0, config_revision: 0 }, |_| panic!("unnamed sibling must reject before persistence")).is_err());
    assert!(!supervisor.is_running("project"));
    supervisor.start(spec);
    let state = connected(&supervisor, 1).await;
    let ports = ports(&fake);
    assert_eq!(status(ports[0]).await.0, 200);
    assert_eq!(status(ports[1]).await.0, 200);
    assert!(supervisor.stop_route("project", target(&state, a), |_| panic!("unnamed sibling must reject before persistence")).is_err());
    assert_eq!(status(ports[0]).await.0, 200);
    assert_eq!(status(ports[1]).await.0, 200);
    assert_eq!(supervisor.config_state("project").unwrap().pid, state.pid);
    supervisor.stop_all().await;
}

#[tokio::test]
async fn selection_survives_restart_and_restored_start_with_capture_enabled() {
    let fake = fake_cloudflared::fake("ok");
    let origin = format!("http://{}", common::start_origin().await);
    let (supervisor, mut spec, captured) = setup(&fake, &[origin.clone(), origin.clone(), origin]);
    spec.tunnel.inspect = true;
    let selection = RouteSelection::Only(vec![key(&spec, 0)]);
    supervisor.restore_route_selection("project", selection.clone()).unwrap();
    supervisor.restart(spec.clone());
    let state = connected(&supervisor, 1).await;
    let initial_ports = ports(&fake);
    assert!(state.inspecting);
    assert_eq!(status(initial_ports[0]).await.0, 200);
    assert_eq!(status(initial_ports[1]).await.0, 503);
    supervisor.restart(spec);
    let restarted = connected(&supervisor, 2).await;
    assert_eq!(ports(&fake), initial_ports);
    assert_eq!(supervisor.route_selection("project"), selection);
    assert_eq!(status(initial_ports[1]).await.0, 503);
    assert!(supervisor.stop_route("project", target(&state, key_from_mapping(&state)), |_| Ok(())).is_err());
    assert!(restarted.mappings[0].enabled && !restarted.mappings[1].enabled);
    assert!(*captured.lock().unwrap() > 0);
    supervisor.stop_all().await;
}

fn key_from_mapping(state: &TunnelState) -> RouteKey { state.mappings[0].route_key.clone().unwrap() }

#[tokio::test]
async fn stopping_the_only_enabled_hostname_stops_the_project_and_next_start_is_isolated() {
    let fake = fake_cloudflared::fake("ok");
    let origin = format!("http://{}", common::start_origin().await);
    let (supervisor, spec, _) = setup(&fake, &[origin.clone(), origin.clone(), origin]);
    let a = key(&spec, 0);
    supervisor.start_route(spec.clone(), RouteTarget { key: a.clone(), run: 0, config_revision: 0 }, |_| Ok(())).unwrap();
    let running = connected(&supervisor, 1).await;
    assert!(supervisor.stop_route("project", target(&running, a.clone()), |_| Err("disk full".into())).is_err());
    assert_eq!(supervisor.config_state("project").unwrap().pid, running.pid);
    assert!(supervisor.is_running("project"));
    assert_eq!(status(ports(&fake)[0]).await.0, 200);
    let mut saved = None;
    supervisor.stop_route("project", target(&running, a), |selection| { saved = Some(selection.clone()); Ok(()) }).unwrap();
    tokio::time::timeout(Duration::from_secs(3), supervisor.wait_for_stop("project", running.run)).await.unwrap();
    assert!(!supervisor.is_running("project"), "stopping the last enabled hostname must stop the project, rather than leave a green running connector");
    assert!(supervisor.running_ids().is_empty());
    assert!(supervisor.config_state("project").is_none());
    assert!(!fake.data.join("run/project.pid").exists());
    assert!(underlane_core::platform::signal_group(running.pid.unwrap(), 0).is_err(), "the connector process group must be gone");
    assert_eq!(saved, Some(RouteSelection::Only(Vec::new())));
    assert_eq!(supervisor.route_selection("project"), RouteSelection::Only(Vec::new()));
    supervisor.start_route(spec.clone(), RouteTarget { key: key(&spec, 2), run: 0, config_revision: 0 }, |_| Ok(())).unwrap();
    let restarted = connected(&supervisor, 1).await;
    assert_ne!(restarted.run, running.run);
    let ports = ports(&fake);
    assert_eq!(status(ports[0]).await.0, 503);
    assert_eq!(status(ports[1]).await.0, 503);
    assert_eq!(status(ports[2]).await.0, 200);
    supervisor.stop_all().await;
}

#[tokio::test]
async fn restored_empty_selection_remains_stopped_until_explicit_project_start() {
    let fake = fake_cloudflared::fake("ok");
    let origin = format!("http://{}", common::start_origin().await);
    let (supervisor, spec, _) = setup(&fake, &[origin.clone(), origin.clone(), origin]);
    supervisor.restore_route_selection("project", RouteSelection::Only(Vec::new())).unwrap();
    supervisor.restart(spec.clone());
    assert!(!supervisor.is_running("project"), "restoring a stopped hostname selection must not autostart an empty connector");
    assert!(!fake.data.join("run/project.pid").exists());
    supervisor.start(spec);
    let state = connected(&supervisor, 1).await;
    assert!(state.mappings.iter().filter(|mapping| mapping.route_key.is_some()).all(|mapping| mapping.enabled));
    assert_eq!(supervisor.route_selection("project"), RouteSelection::All);
    supervisor.stop_all().await;
}

#[tokio::test]
async fn stale_saved_selection_must_not_connect_a_project_with_no_enabled_current_hostnames() {
    let fake = fake_cloudflared::fake("ok");
    let origin = format!("http://{}", common::start_origin().await);
    let (supervisor, spec, _) = setup(&fake, &[origin.clone(), origin.clone(), origin]);
    let selected = key(&spec, 0);
    let path = spec.tunnel.config_path.as_ref().unwrap();
    let source = std::fs::read_to_string(path).unwrap().replace("a.example.com", "renamed.example.com");
    std::fs::write(path, &source).unwrap();
    supervisor.restore_route_selection("project", RouteSelection::Only(vec![selected])).unwrap();
    supervisor.restart(spec.clone());
    for _ in 0..200 {
        if !supervisor.is_running("project") || supervisor.config_state("project").is_some_and(|state| state.status.is_active()) { break; }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let state = supervisor.config_state("project");
    assert!(!supervisor.is_running("project"), "saved rules no longer match any current hostname, so there must not be a green empty connector: {state:?}");
    assert!(!fake.data.join("run/project.pid").exists());
    assert_eq!(std::fs::read_to_string(path).unwrap(), source);
}

#[tokio::test]
async fn shadowed_hostname_rules_reject_individual_controls_before_exposing_the_wrong_origin() {
    for (earlier, later, earlier_path, later_path) in [
        ("*.example.com", "a.example.com", None, None),
        ("a.example.com", "a.example.com", None, None),
        ("a.example.com", "a.example.com", Some(".*"), Some("^/api")),
        ("a.example.com", "*.example.com", None, None),
    ] {
        let fake = fake_cloudflared::fake("ok");
        let (supervisor, spec, _) = setup(&fake, &["http://127.0.0.1:4000".into(), "http://127.0.0.1:5000".into(), "http://127.0.0.1:6000".into()]);
        let path = spec.tunnel.config_path.as_ref().unwrap();
        let mut source = std::fs::read_to_string(path).unwrap().replace("a.example.com", &format!("\"{earlier}\"")).replace("b.example.com", &format!("\"{later}\""));
        if let Some(pattern) = earlier_path { source = source.replace("    service: http://127.0.0.1:4000", &format!("    path: \"{pattern}\"\n    service: http://127.0.0.1:4000")); }
        if let Some(pattern) = later_path { source = source.replace("    service: http://127.0.0.1:5000", &format!("    path: \"{pattern}\"\n    service: http://127.0.0.1:5000")); }
        std::fs::write(path, &source).unwrap();
        let result = supervisor.start_route(spec.clone(), RouteTarget { key: key(&spec, 1), run: 0, config_revision: 0 }, |_| Ok(()));
        assert!(result.is_err(), "overlapping ingress patterns ({earlier}, {later}) cannot safely offer independent controls");
        assert!(!supervisor.is_running("project"));
        supervisor.start(spec.clone());
        let running = connected(&supervisor, 1).await;
        assert!(running.mappings.iter().filter(|mapping| mapping.route_key.is_some()).all(|mapping| !mapping.control_supported));
        assert!(supervisor.stop_route("project", target(&running, key(&spec, 1)), |_| panic!("shadowed controls must reject before persistence")).is_err());
        assert_eq!(supervisor.config_state("project").unwrap().pid, running.pid);
        assert_eq!(supervisor.route_selection("project"), RouteSelection::All);
        supervisor.stop_all().await;
        assert_eq!(std::fs::read_to_string(path).unwrap(), source);
    }
}

#[tokio::test]
async fn source_restart_stops_if_every_selected_route_disappears_and_preserves_partial_overlap() {
    for keep_b in [false, true] {
        let fake = fake_cloudflared::fake("ok");
        let origin = format!("http://{}", common::start_origin().await);
        let (supervisor, spec, _) = setup(&fake, &[origin.clone(), origin.clone(), origin]);
        supervisor.start_route(spec.clone(), RouteTarget { key: key(&spec, 0), run: 0, config_revision: 0 }, |_| Ok(())).unwrap();
        let running = connected(&supervisor, 1).await;
        if keep_b { supervisor.start_route(spec.clone(), target(&running, key(&spec, 1)), |_| Ok(())).unwrap(); }
        let path = spec.tunnel.config_path.as_ref().unwrap();
        let source = std::fs::read_to_string(path).unwrap().replace("a.example.com", "renamed.example.com");
        std::fs::write(path, &source).unwrap();
        supervisor.restart(spec.clone());
        if keep_b {
            let reloaded = connected(&supervisor, 2).await;
            let ports = ports(&fake);
            assert_eq!(reloaded.mappings.iter().filter(|mapping| mapping.route_key.is_some() && mapping.enabled).count(), 1);
            assert!(!reloaded.mappings[0].enabled && reloaded.mappings[1].enabled && !reloaded.mappings[2].enabled);
            assert_eq!(status(ports[0]).await.0, 503);
            assert_eq!(status(ports[1]).await.0, 200);
            assert_eq!(status(ports[2]).await.0, 503);
            supervisor.stop_all().await;
        } else {
            for _ in 0..200 {
                if !supervisor.is_running("project") { break; }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            assert!(!supervisor.is_running("project"), "a source restart must stop if no selected current hostname remains: {:?}", supervisor.config_state("project"));
            assert!(!fake.data.join("run/project.pid").exists());
            supervisor.start_route(spec.clone(), RouteTarget { key: key(&spec, 2), run: 0, config_revision: 0 }, |_| Ok(())).unwrap();
            let started = connected(&supervisor, 1).await;
            assert_ne!(started.run, running.run);
            assert!(!started.mappings[0].enabled && !started.mappings[1].enabled && started.mappings[2].enabled);
            supervisor.stop_all().await;
        }
        assert_eq!(std::fs::read_to_string(path).unwrap(), source);
    }
}

#[tokio::test]
async fn state_publications_have_strict_order_across_gate_toggles_restart_and_stop() {
    let fake = fake_cloudflared::fake("ok");
    let origin = format!("http://{}", common::start_origin().await);
    let (base, spec, _) = setup(&fake, &[origin.clone(), origin.clone(), origin]);
    let states = Arc::new(Mutex::new(Vec::new()));
    let recorded = states.clone();
    let supervisor = Supervisor::new(base.config().clone(), Arc::new(move |event| {
        if let Event::State(state) = event { recorded.lock().unwrap().push(state); }
    }));
    supervisor.start_route(spec.clone(), RouteTarget { key: key(&spec, 0), run: 0, config_revision: 0 }, |_| Ok(())).unwrap();
    let first = connected(&supervisor, 1).await;
    supervisor.start_route(spec.clone(), target(&first, key(&spec, 1)), |_| Ok(())).unwrap();
    supervisor.stop_route("project", target(&first, key(&spec, 1)), |_| Ok(())).unwrap();
    assert_eq!(supervisor.config_state("project").unwrap().config_revision, first.config_revision);
    supervisor.restart(spec.clone());
    let reloaded = connected(&supervisor, 2).await;
    supervisor.stop_route("project", target(&reloaded, key(&spec, 0)), |_| Ok(())).unwrap();
    supervisor.wait_for_stop("project", reloaded.run).await;
    supervisor.stop("project").await;
    let states = states.lock().unwrap();
    assert!(states.iter().any(|state| state.status == Status::Stopped));
    for pair in states.windows(2) {
        let sequence = |state: &TunnelState| serde_json::to_value(state).unwrap().get("state_seq").and_then(serde_json::Value::as_u64).unwrap_or_default();
        assert!(pair[1].run > pair[0].run || (pair[1].run == pair[0].run && sequence(&pair[1]) > sequence(&pair[0])),
            "state publication order must not allow an older gate event to roll back a newer snapshot: previous={:?}, next={:?}", pair[0], pair[1]);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn idle_stop_and_concurrent_start_reserve_state_order_under_the_same_lock() {
    let fake = fake_cloudflared::fake("ok");
    let (base, spec, _) = setup(&fake, &vec!["http://localhost:3000".into(); 3]);
    *base.config().cloudflared.write().unwrap() = None;
    for _ in 0..200 {
        let states = Arc::new(Mutex::new(Vec::new()));
        let recorded = states.clone();
        let supervisor = Supervisor::new(base.config().clone(), Arc::new(move |event| {
            if let Event::State(state) = event { recorded.lock().unwrap().push(state); }
        }));
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let stopping = supervisor.clone();
        let stop_barrier = barrier.clone();
        let runtime = tokio::runtime::Handle::current();
        let stop = std::thread::spawn(move || { stop_barrier.wait(); runtime.block_on(stopping.stop("project")); });
        let starting = supervisor.clone();
        let next = spec.clone();
        let start = std::thread::spawn(move || { barrier.wait(); starting.start(next); });
        stop.join().unwrap();
        start.join().unwrap();
        let live = supervisor.config_state("project");
        let incorrectly_newer_stop = live.as_ref().is_some_and(|live| {
            states.lock().unwrap().iter().any(|state| state.status == Status::Stopped && state.run > live.run)
        });
        supervisor.stop_all().await;
        assert!(!incorrectly_newer_stop, "a synthetic stopped publication must never mask a concurrently reserved live run: {live:?}");
    }
}

#[tokio::test]
async fn last_hostname_stop_rejects_stale_row_starts_and_wait_does_not_stop_a_newer_run() {
    let fake = fake_cloudflared::fake("ok");
    let script = std::fs::read_to_string(&fake.bin).unwrap().replace("    os._exit(0)", "    time.sleep(0.2)\n    os._exit(0)");
    std::fs::write(&fake.bin, script).unwrap();
    let origin = format!("http://{}", common::start_origin().await);
    let (supervisor, spec, _) = setup(&fake, &[origin.clone(), origin.clone(), origin]);
    supervisor.start_route(spec.clone(), RouteTarget { key: key(&spec, 0), run: 0, config_revision: 0 }, |_| Ok(())).unwrap();
    let running = connected(&supervisor, 1).await;
    supervisor.stop_route("project", target(&running, key(&spec, 0)), |_| Ok(())).unwrap();
    assert!(supervisor.config_state("project").is_none(), "last stop marks the run stopping before releasing the action lock");
    assert!(supervisor.restart_session_ids().is_empty());
    assert!(supervisor.start_route(spec.clone(), target(&running, key(&spec, 1)), |_| panic!("a row start against a stopping run must not persist")).unwrap_err().contains("stopping"));
    supervisor.start_all_routes(spec, |_| Ok(())).unwrap();
    let ((), restarted) = tokio::join!(supervisor.wait_for_stop("project", running.run), connected(&supervisor, 1));
    assert_ne!(restarted.run, running.run);
    assert_ne!(restarted.pid, running.pid);
    supervisor.wait_for_stop("project", running.run).await;
    assert_eq!(supervisor.config_state("project").unwrap().run, restarted.run);
    assert!(supervisor.is_running("project"));
    supervisor.stop_all().await;
}

#[tokio::test]
async fn a_later_project_stop_cancels_a_project_start_deferred_by_last_hostname_stop() {
    let fake = fake_cloudflared::fake("ok");
    let script = std::fs::read_to_string(&fake.bin).unwrap().replace("    os._exit(0)", "    time.sleep(0.2)\n    os._exit(0)");
    std::fs::write(&fake.bin, script).unwrap();
    let origin = format!("http://{}", common::start_origin().await);
    let (supervisor, spec, _) = setup(&fake, &[origin.clone(), origin.clone(), origin]);
    supervisor.start_route(spec.clone(), RouteTarget { key: key(&spec, 0), run: 0, config_revision: 0 }, |_| Ok(())).unwrap();
    let running = connected(&supervisor, 1).await;
    supervisor.stop_route("project", target(&running, key(&spec, 0)), |_| Ok(())).unwrap();
    supervisor.start_all_routes(spec, |_| Ok(())).unwrap();
    supervisor.stop("project").await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!supervisor.is_running("project"), "a deferred Start must not resurrect the connector after a later explicit Stop: {:?}", supervisor.config_state("project"));
    assert!(!fake.data.join("run/project.pid").exists());
}

#[tokio::test]
async fn removed_selected_rules_do_not_keep_the_project_alive_after_its_last_current_hostname_stops() {
    let fake = fake_cloudflared::fake("ok");
    let origin = format!("http://{}", common::start_origin().await);
    let (supervisor, spec, _) = setup(&fake, &[origin.clone(), origin.clone(), origin.clone()]);
    supervisor.start_route(spec.clone(), RouteTarget { key: key(&spec, 0), run: 0, config_revision: 0 }, |_| Ok(())).unwrap();
    let running = connected(&supervisor, 1).await;
    supervisor.start_route(spec.clone(), target(&running, key(&spec, 1)), |_| Ok(())).unwrap();
    let path = spec.tunnel.config_path.as_ref().unwrap();
    let source = std::fs::read_to_string(path).unwrap().replace(&format!("  - hostname: b.example.com\n    service: {origin}\n"), "");
    std::fs::write(path, &source).unwrap();
    supervisor.restart(spec.clone());
    let restarted = connected(&supervisor, 2).await;
    assert_eq!(restarted.mappings.iter().filter(|mapping| mapping.route_key.is_some() && mapping.enabled).count(), 1);
    let selection = supervisor.stop_route("project", target(&restarted, key(&spec, 0)), |_| Ok(())).unwrap();
    assert_eq!(selection, RouteSelection::Only(Vec::new()));
    supervisor.wait_for_stop("project", restarted.run).await;
    assert!(!supervisor.is_running("project"));
    assert_eq!(std::fs::read_to_string(path).unwrap(), source);
}

#[tokio::test]
async fn proxy_setup_failure_never_falls_back_to_direct_origin_even_without_capture() {
    let fake = fake_cloudflared::fake("ok");
    let (supervisor, spec, _) = setup(&fake, &vec!["https://localhost:3000".into(); 3]);
    let path = spec.tunnel.config_path.as_ref().unwrap();
    let source = std::fs::read_to_string(path).unwrap();
    std::fs::write(path, format!("originRequest:\n  caPool: missing-ca.pem\n{source}")).unwrap();
    supervisor.start_route(spec.clone(), RouteTarget { key: key(&spec, 0), run: 0, config_revision: 0 }, |_| Ok(())).unwrap();
    for _ in 0..200 {
        if supervisor.config_state("project").is_some_and(|state| state.status == Status::Crashed) { break; }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let state = supervisor.config_state("project").unwrap();
    assert_eq!(state.status, Status::Crashed);
    assert!(state.pid.is_none());
    assert!(state.failure.as_ref().unwrap().raw.contains("hostname proxy could not start"));
    assert!(!fake.data.join("derived/project.yml").exists());
    supervisor.stop_all().await;
}

#[tokio::test]
async fn starting_a_paused_route_recovers_an_exhausted_connector_without_enabling_others() {
    let fake = fake_cloudflared::fake("ok");
    let origin = format!("http://{}", common::start_origin().await);
    let (supervisor, spec, _) = setup(&fake, &[origin.clone(), origin.clone(), origin]);
    supervisor.start_route(spec.clone(), RouteTarget { key: key(&spec, 0), run: 0, config_revision: 0 }, |_| Ok(())).unwrap();
    let first = connected(&supervisor, 1).await;
    underlane_core::platform::signal_group(first.pid.unwrap(), underlane_core::platform::SIGTERM).unwrap();
    for _ in 0..200 {
        if supervisor.config_state("project").is_some_and(|state| state.status == Status::Crashed) { break; }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let crashed = supervisor.config_state("project").unwrap();
    assert_eq!(crashed.status, Status::Crashed);
    supervisor.start_route(spec.clone(), target(&crashed, key(&spec, 2)), |_| Ok(())).unwrap();
    let recovered = connected(&supervisor, 2).await;
    assert_ne!(recovered.pid, first.pid);
    assert_eq!(recovered.run, first.run);
    let ports = ports(&fake);
    assert_eq!(status(ports[0]).await.0, 200);
    assert_eq!(status(ports[1]).await.0, 503);
    assert_eq!(status(ports[2]).await.0, 200);
    supervisor.stop_all().await;
}

#[tokio::test]
async fn disabled_requests_never_reach_origin_or_capture() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use underlane_core::derived::Upstream;
    use underlane_core::proxy::{self, ProxyContext};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let origin_requests = Arc::new(AtomicUsize::new(0));
    let count = origin_requests.clone();
    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let count = count.clone();
            tokio::spawn(async move {
                let service = hyper::service::service_fn(move |_| {
                    count.fetch_add(1, Ordering::SeqCst);
                    async { Ok::<_, Infallible>(hyper::Response::new(Full::new(Bytes::from_static(b"origin")))) }
                });
                let _ = hyper::server::conn::http1::Builder::new().serve_connection(TokioIo::new(stream), service).await;
            });
        }
    });
    let captured = Arc::new(AtomicUsize::new(0));
    let count = captured.clone();
    let ctx = ProxyContext { tunnel_id: "test".into(), options: Arc::new(RwLock::new(CaptureOptions::default())),
        sink: Arc::new(move |_| { count.fetch_add(1, Ordering::SeqCst); }) };
    let key = RouteKey { hostname: "count.example.com".into(), path: None, service: format!("http://{address}") };
    let selection = Arc::new(RwLock::new(RouteSelection::Only(Vec::new())));
    let gate = RouteGate { selection: selection.clone(), key: Some(key.clone()) };
    let proxy = proxy::start_controlled(0, Upstream::from_service(0, Some(key.hostname.clone()), &key.service).unwrap(), ctx, Some(gate), true).await.unwrap();
    assert_eq!(status(proxy.port()).await, (hyper::StatusCode::SERVICE_UNAVAILABLE, true));
    assert_eq!((origin_requests.load(Ordering::SeqCst), captured.load(Ordering::SeqCst)), (0, 0));
    *selection.write().unwrap() = RouteSelection::Only(vec![key]);
    assert_eq!(status(proxy.port()).await.0, 200);
    *selection.write().unwrap() = RouteSelection::Only(Vec::new());
    assert_eq!(status(proxy.port()).await.0, 503);
    for _ in 0..100 {
        if captured.load(Ordering::SeqCst) == 1 { break; }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!((origin_requests.load(Ordering::SeqCst), captured.load(Ordering::SeqCst)), (1, 1));
}
