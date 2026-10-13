//! End-to-end control server. Compiled in only with `--features e2e` and started only when
//! `UNDERLANE_E2E_PORT` is set; it listens on loopback and drives the real app the same way the UI
//! does, plus it can read the actual menu bar title and the dashboard's rendered text.

use std::collections::HashMap;
use std::sync::Mutex;

use tokio::sync::oneshot;

static PENDING: Mutex<Option<HashMap<String, oneshot::Sender<String>>>> = Mutex::new(None);

/// Called by the dashboard (through a command) with its rendered text.
pub fn report(nonce: String, text: String) {
    if let Some(tx) = PENDING.lock().unwrap().as_mut().and_then(|m| m.remove(&nonce)) {
        let _ = tx.send(text);
    }
}

// Debug builds only: even `--features e2e` on a release build doesn't include the server.
#[cfg(all(feature = "e2e", debug_assertions))]
pub use server::start;

#[cfg(all(feature = "e2e", debug_assertions))]
mod server {
    use std::collections::HashMap;
    use std::convert::Infallible;
    use std::sync::Arc;
    use std::time::Duration;

    use bytes::Bytes;
    use http_body_util::Full;
    use hyper::{Request, Response};
    use hyper_util::rt::TokioIo;
    use serde_json::{json, Value};
    use tauri::{AppHandle, Emitter, Manager};

    use super::PENDING;
    use crate::state::Core;
    use crate::tray;

    pub fn start(app: AppHandle, port: u16) {
        tauri::async_runtime::spawn(async move {
            let listener = match tokio::net::TcpListener::bind(("127.0.0.1", port)).await {
                Ok(l) => l,
                Err(e) => {
                    tracing::error!("e2e control server: {e}");
                    return;
                }
            };
            tracing::info!("e2e control server on 127.0.0.1:{port}");
            loop {
                let Ok((stream, _)) = listener.accept().await else { continue };
                let app = app.clone();
                tokio::spawn(async move {
                    let svc = hyper::service::service_fn(move |req| {
                        let app = app.clone();
                        async move { Ok::<_, Infallible>(handle(app, req).await) }
                    });
                    let _ = hyper::server::conn::http1::Builder::new().serve_connection(TokioIo::new(stream), svc).await;
                });
            }
        });
    }

    fn query(req: &Request<hyper::body::Incoming>) -> HashMap<String, String> {
        req.uri()
            .query()
            .unwrap_or("")
            .split('&')
            .filter_map(|kv| kv.split_once('='))
            .map(|(k, v)| (k.to_string(), percent_decode(v)))
            .collect()
    }

    fn percent_decode(s: &str) -> String {
        let b = s.as_bytes();
        let mut out = Vec::new();
        let mut i = 0;
        while i < b.len() {
            if b[i] == b'%' && i + 2 < b.len() {
                if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                    out.push(v);
                    i += 3;
                    continue;
                }
            }
            out.push(if b[i] == b'+' { b' ' } else { b[i] });
            i += 1;
        }
        String::from_utf8_lossy(&out).into_owned()
    }

    fn reply(status: u16, v: Value) -> Response<Full<Bytes>> {
        let mut r = Response::new(Full::new(Bytes::from(v.to_string())));
        *r.status_mut() = hyper::StatusCode::from_u16(status).unwrap();
        r.headers_mut().insert("content-type", "application/json".parse().unwrap());
        r
    }

    async fn handle(app: AppHandle, req: Request<hyper::body::Incoming>) -> Response<Full<Bytes>> {
        let core = app.state::<Arc<Core>>().inner().clone();
        let q = query(&req);
        let id = q.get("id").cloned().unwrap_or_default();
        match (req.method().as_str(), req.uri().path()) {
            ("GET", "/health") => reply(200, json!({"ok": true})),
            ("POST", "/quick") => {
                let level = core.settings.read().unwrap().default_loglevel.clone();
                let origin = q.get("origin").cloned().unwrap_or_default();
                match underlane_core::import::add_simple(
                    &core.db,
                    underlane_core::model::TunnelKind::Quick,
                    q.get("name").map(String::as_str).unwrap_or("e2e"),
                    "E2E",
                    Some(underlane_core::supervisor::normalize_origin(&origin)),
                    &level,
                ) {
                    Ok(mut t) => {
                        t.inspect = q.get("inspect").map(|v| v == "1").unwrap_or(false);
                        let _ = core.db.upsert_tunnel(&t);
                        core.notify_changed();
                        reply(200, json!({"id": t.id}))
                    }
                    Err(e) => reply(500, json!({"error": e.to_string()})),
                }
            }
            ("POST", "/start") => match core.start(&id) {
                Ok(()) => reply(200, json!({"ok": true})),
                Err(e) => reply(400, json!({"error": e.to_string()})),
            },
            ("POST", "/stop") => {
                core.stop(&id).await;
                reply(200, json!({"ok": true}))
            }
            ("POST", "/restart") => match core.restart(&id) {
                Ok(()) => reply(200, json!({"ok": true})),
                Err(e) => reply(400, json!({"error": e.to_string()})),
            },
            ("POST", "/ingress-rule") => match core.test_ingress_rule(id, q.get("url").cloned().unwrap_or_default(), q.get("requestId").cloned().unwrap_or_default()).await {
                Ok(result) => reply(200, json!(result)),
                Err(error) => reply(400, json!({"error": error})),
            },
            ("POST", "/cancel-ingress") => match core.cancel_ingress_test(q.get("requestId").map(String::as_str).unwrap_or_default()) {
                Ok(()) => reply(200, json!({"ok": true})),
                Err(error) => reply(400, json!({"error": error})),
            },
            ("GET", "/config-change") => {
                match tokio::task::spawn_blocking(move || core.check_config_change(&id)).await {
                    Ok(Ok(result)) => reply(200, json!(result)),
                    _ => reply(500, json!({"error": "Config check failed"})),
                }
            }
            ("GET", "/state") => {
                let s = core.state_of(&id);
                let title = tray::status_item_title(&app);
                reply(
                    200,
                    json!({
                        "status": s.status,
                        "public_url": s.public_url,
                        "detail": s.detail,
                        "pid": s.pid,
                        "run": s.run,
                        "config_revision": s.config_revision,
                        "inspecting": s.inspecting,
                        "active_count": core.active_count(),
                        "menu_bar_title": title,
                        "internal_title": tray::TITLE.lock().unwrap().clone(),
                    }),
                )
            }
            ("GET", "/requests") => match core.db.list_requests(Some(&id), None, 100) {
                Ok(r) => reply(200, json!(r)),
                Err(e) => reply(500, json!({"error": e.to_string()})),
            },
            ("GET", "/request") => match core.db.get_request(q.get("rid").map(String::as_str).unwrap_or("")) {
                Ok(Some(x)) => reply(200, serde_json::to_value(crate::commands::detail_of(x)).unwrap()),
                Ok(None) => reply(404, json!({"error": "not found"})),
                Err(e) => reply(500, json!({"error": e.to_string()})),
            },
            ("GET", "/logs") => {
                let logs = core.db.query_logs(&underlane_core::db::LogQuery { tunnel_id: Some(id), limit: 1000, ..Default::default() });
                match logs {
                    Ok(l) => reply(200, json!(l)),
                    Err(e) => reply(500, json!({"error": e.to_string()})),
                }
            }
            ("GET", "/ui") => {
                // Open the dashboard on a view, let it render, then ask it for its text.
                let view = q.get("view").cloned().unwrap_or_else(|| "tunnels".into());
                tray::open_dashboard(&app, Some(view.clone()));
                tokio::time::sleep(Duration::from_millis(1500)).await;
                let nonce = uuid::Uuid::new_v4().to_string();
                let (tx, rx) = tokio::sync::oneshot::channel();
                PENDING.lock().unwrap().get_or_insert_with(HashMap::new).insert(nonce.clone(), tx);
                let _ = app.emit_to(
                    tray::DASHBOARD,
                    "e2e-snapshot",
                    json!({"nonce": nonce, "view": view, "select": q.get("select"), "tab": q.get("tab")}),
                );
                match tokio::time::timeout(Duration::from_secs(10), rx).await {
                    Ok(Ok(text)) => reply(200, json!({"text": text})),
                    _ => reply(504, json!({"error": "the dashboard did not answer"})),
                }
            }
            ("GET", "/tray-rect") => {
                let rect = app.tray_by_id(tray::TRAY_ID).and_then(|t| t.rect().ok().flatten());
                match rect {
                    Some(r) => {
                        let scale = app.primary_monitor().ok().flatten().map(|m| m.scale_factor()).unwrap_or(2.0);
                        let p = r.position.to_logical::<f64>(scale);
                        let sz = r.size.to_logical::<f64>(scale);
                        reply(200, json!({"x": p.x, "y": p.y, "width": sz.width, "height": sz.height}))
                    }
                    None => reply(404, json!({"error": "no tray rect"})),
                }
            }
            ("POST", "/popover") => {
                let app2 = app.clone();
                let _ = app.run_on_main_thread(move || tray::toggle_popover(&app2));
                reply(200, json!({"ok": true}))
            }
            ("POST", "/import") => {
                let level = core.settings.read().unwrap().default_loglevel.clone();
                let path = q.get("path").cloned().unwrap_or_default();
                let r = underlane_core::import::import_configs(&core.db, &[std::path::PathBuf::from(path)], &level);
                core.notify_changed();
                reply(200, json!(r))
            }
            ("POST", "/settings") => {
                let mut s = core.settings.read().unwrap().clone();
                if let Some(t) = q.get("theme") {
                    s.theme = t.clone();
                }
                match core.apply_settings(s) {
                    Ok(()) => reply(200, json!({"ok": true})),
                    Err(e) => reply(500, json!({"error": e.to_string()})),
                }
            }
            ("POST", "/tunnel") => {
                // Toggle flags: /tunnel?id=..&autostart=1&inspect=0
                match core.db.tunnel(&id) {
                    Ok(Some(mut t)) => {
                        if let Some(v) = q.get("autostart") {
                            t.autostart = v == "1";
                        }
                        if let Some(v) = q.get("inspect") {
                            t.inspect = v == "1";
                        }
                        let _ = core.db.upsert_tunnel(&t);
                        core.notify_changed();
                        reply(200, json!(t))
                    }
                    _ => reply(404, json!({"error": "no such tunnel"})),
                }
            }
            ("GET", "/overview") => match core.overview() {
                Ok(o) => reply(200, serde_json::to_value(o).unwrap()),
                Err(e) => reply(500, json!({"error": e.to_string()})),
            },
            ("POST", "/navigate") => {
                tray::open_dashboard(&app, q.get("view").cloned());
                reply(200, json!({"ok": true}))
            }
            ("POST", "/quit") => {
                app.exit(0);
                reply(200, json!({"ok": true}))
            }
            _ => reply(404, json!({"error": "unknown route"})),
        }
    }
}
