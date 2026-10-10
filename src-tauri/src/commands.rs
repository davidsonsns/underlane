//! Tauri commands invoked from the UI.

use std::path::PathBuf;
use std::sync::Arc;

use base64::Engine;
use burrow_core::cloudflared::{self, Binary, Validation};
use burrow_core::config::{home_dir, scan_dir, TunnelConfig};
use burrow_core::db::{LogQuery, RequestSummary};
use burrow_core::decode::{decode_body, is_probably_text};
use burrow_core::import::{self, ImportReport};
use burrow_core::model::{LogLine, Settings, TunnelKind};
use burrow_core::proxy::capture::{BodyCapture, CapturedExchange};
use burrow_core::supervisor::normalize_origin;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime, State};

use crate::platform::{self, LoginItem};
use crate::state::{Core, Overview};
use crate::{tokens, tray};

type Res<T> = Result<T, String>;
type CoreState<'a> = State<'a, Arc<Core>>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[tauri::command]
pub fn overview(core: CoreState<'_>) -> Res<Overview> {
    core.overview().map_err(err)
}

#[derive(Serialize)]
pub struct Candidate {
    pub path: String,
    pub name: String,
    pub project: String,
    pub hostnames: Vec<String>,
    pub imported: bool,
}

/// Config files in `~/.cloudflared`, for the import sheet.
#[tauri::command]
pub fn scan_default(core: CoreState<'_>) -> Res<Vec<Candidate>> {
    let dir = home_dir().join(".cloudflared");
    let mut out = Vec::new();
    for p in scan_dir(&dir) {
        let key = p.to_string_lossy().into_owned();
        let Ok(c) = TunnelConfig::load(&p) else { continue };
        out.push(Candidate {
            imported: core.db.tunnel_by_config_path(&key).ok().flatten().is_some(),
            name: c.display_name(),
            project: import::project_name_for(&p),
            hostnames: c.ingress.iter().filter_map(|r| r.hostname.clone()).collect(),
            path: key,
        });
    }
    Ok(out)
}

#[tauri::command]
pub fn import_paths(core: CoreState<'_>, paths: Vec<String>) -> Res<ImportReport> {
    let level = core.settings.read().unwrap().default_loglevel.clone();
    let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    Ok(import::import_configs(&core.db, &paths, &level))
}

#[tauri::command]
pub fn add_quick_tunnel(core: CoreState<'_>, name: String, origin: String, project: String, inspect: bool) -> Res<String> {
    let origin = normalize_origin(&origin);
    if burrow_core::derived::split_service(&origin).is_none() {
        return Err(format!("`{origin}` isn't a local http:// or https:// address."));
    }
    let level = core.settings.read().unwrap().default_loglevel.clone();
    let name = if name.trim().is_empty() { origin.trim_start_matches("http://").trim_start_matches("https://").to_string() } else { name };
    let mut t = import::add_simple(&core.db, TunnelKind::Quick, &name, &project, Some(origin), &level).map_err(err)?;
    if inspect {
        t.inspect = true;
        core.db.upsert_tunnel(&t).map_err(err)?;
    }
    Ok(t.id)
}

#[tauri::command]
pub fn add_token_tunnel(core: CoreState<'_>, name: String, token: String, project: String) -> Res<String> {
    let token = token.trim().trim_start_matches("cloudflared tunnel run --token ").trim().to_string();
    if token.len() < 20 {
        return Err("That doesn't look like a tunnel token. Copy it from the tunnel's install command in the Cloudflare dashboard.".into());
    }
    let level = core.settings.read().unwrap().default_loglevel.clone();
    let name = if name.trim().is_empty() { "Remote tunnel".to_string() } else { name };
    let t = import::add_simple(&core.db, TunnelKind::Token, &name, &project, None, &level).map_err(err)?;
    tokens::set(&t.id, &token).map_err(|e| format!("Couldn't save the token in the keychain: {e}"))?;
    core.db.kv_set(&format!("token:{}", t.id), "1").map_err(err)?;
    Ok(t.id)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelPatch {
    pub name: Option<String>,
    pub project: Option<String>,
    pub inspect: Option<bool>,
    pub autostart: Option<bool>,
    pub loglevel: Option<String>,
    pub origin_url: Option<String>,
    pub token: Option<String>,
}

#[tauri::command]
pub fn update_tunnel(core: CoreState<'_>, id: String, patch: TunnelPatch) -> Res<()> {
    let mut t = core.db.tunnel(&id).map_err(err)?.ok_or("Tunnel not found")?;
    let mut needs_restart = false;
    if let Some(n) = patch.name.filter(|n| !n.trim().is_empty()) {
        t.name = n.trim().to_string();
    }
    if let Some(p) = patch.project.filter(|p| !p.trim().is_empty()) {
        t.project_id = core.db.ensure_project(&p).map_err(err)?.id;
    }
    if let Some(i) = patch.inspect {
        needs_restart |= i != t.inspect;
        t.inspect = i;
    }
    if let Some(a) = patch.autostart {
        t.autostart = a;
    }
    if let Some(l) = patch.loglevel.filter(|l| ["debug", "info", "warn", "error", "fatal"].contains(&l.as_str())) {
        needs_restart |= l != t.loglevel;
        t.loglevel = l;
    }
    if let Some(o) = patch.origin_url.filter(|_| t.kind == TunnelKind::Quick) {
        let o = normalize_origin(&o);
        if burrow_core::derived::split_service(&o).is_none() {
            return Err(format!("`{o}` isn't a local http:// or https:// address."));
        }
        needs_restart |= Some(&o) != t.origin_url.as_ref();
        t.origin_url = Some(o);
    }
    if let Some(tok) = patch.token.filter(|t| !t.trim().is_empty()) {
        tokens::set(&t.id, &tok).map_err(err)?;
        core.db.kv_set(&format!("token:{}", t.id), "1").map_err(err)?;
        needs_restart = true;
    }
    core.db.upsert_tunnel(&t).map_err(err)?;
    core.db.prune_empty_projects().map_err(err)?;
    if needs_restart {
        core.restart_if_running(&id).map_err(err)?;
    }
    Ok(())
}

#[tauri::command]
pub async fn delete_tunnel(core: CoreState<'_>, id: String) -> Res<()> {
    core.stop(&id).await;
    if core.db.tunnel(&id).map_err(err)?.map(|t| t.kind == TunnelKind::Token).unwrap_or(false) {
        tokens::delete(&id);
    }
    core.db.delete_tunnel(&id).map_err(err)?;
    core.states.lock().unwrap().remove(&id);
    let _ = std::fs::remove_file(core.data_dir.join("derived").join(format!("{id}.yml")));
    Ok(())
}

#[tauri::command]
pub fn rename_project(core: CoreState<'_>, id: String, name: String) -> Res<()> {
    if name.trim().is_empty() {
        return Err("A project needs a name.".into());
    }
    core.db.rename_project(&id, name.trim()).map_err(err)
}

#[tauri::command]
pub fn start_tunnel(core: CoreState<'_>, id: String) -> Res<()> {
    core.start(&id).map_err(err)
}

#[tauri::command]
pub async fn stop_tunnel(core: CoreState<'_>, id: String) -> Res<()> {
    core.stop(&id).await;
    Ok(())
}

#[tauri::command]
pub fn restart_tunnel(core: CoreState<'_>, id: String) -> Res<()> {
    core.restart(&id).map_err(err)
}

pub fn start_all_inner<R: Runtime>(app: &AppHandle<R>) {
    let core = app.state::<Arc<Core>>();
    if let Ok(ts) = core.db.tunnels() {
        for t in ts {
            if !core.state_of(&t.id).status.is_active() {
                let _ = core.start(&t.id);
            }
        }
    }
}

pub async fn stop_all_inner<R: Runtime>(app: &AppHandle<R>) {
    let core = app.state::<Arc<Core>>().inner().clone();
    core.sup.stop_all().await;
}

#[tauri::command]
pub fn start_all(app: AppHandle) {
    start_all_inner(&app);
}

#[tauri::command]
pub async fn stop_all(app: AppHandle) {
    stop_all_inner(&app).await;
}

#[tauri::command]
pub async fn validate_tunnel(core: CoreState<'_>, id: String) -> Res<Validation> {
    let t = core.db.tunnel(&id).map_err(err)?.ok_or("Tunnel not found")?;
    let path = t.config_path.ok_or("Only config-file tunnels can be validated.")?;
    let bin = core.binary.read().unwrap().clone().ok_or("cloudflared wasn't found.")?;
    Ok(cloudflared::validate(std::path::Path::new(&bin.path), std::path::Path::new(&path)).await)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogArgs {
    pub tunnel_id: Option<String>,
    #[serde(default)]
    pub levels: Vec<String>,
    pub search: Option<String>,
    pub before_id: Option<i64>,
    pub after_id: Option<i64>,
    pub limit: Option<usize>,
}

#[tauri::command]
pub fn query_logs(core: CoreState<'_>, args: LogArgs) -> Res<Vec<LogLine>> {
    core.db
        .query_logs(&LogQuery {
            tunnel_id: args.tunnel_id,
            levels: args.levels,
            search: args.search,
            before_id: args.before_id,
            after_id: args.after_id,
            limit: args.limit.unwrap_or(500),
        })
        .map_err(err)
}

#[tauri::command]
pub fn clear_logs(core: CoreState<'_>, tunnel_id: Option<String>) -> Res<()> {
    core.db.clear_logs(tunnel_id.as_deref()).map_err(err)
}

#[tauri::command]
pub fn list_requests(core: CoreState<'_>, tunnel_id: Option<String>, search: Option<String>, limit: Option<usize>) -> Res<Vec<RequestSummary>> {
    core.db.list_requests(tunnel_id.as_deref(), search.as_deref(), limit.unwrap_or(500)).map_err(err)
}

#[derive(Serialize)]
pub struct BodyView {
    pub size: u64,
    pub stored: usize,
    pub truncated: bool,
    pub content_type: Option<String>,
    pub decoded_from: Option<String>,
    pub decode_partial: bool,
    pub is_text: bool,
    /// UTF-8 text (when `is_text`).
    pub text: Option<String>,
    /// Base64 of the stored (decoded) bytes, for the hex view and copy-as-curl.
    pub base64: String,
}

fn body_view(b: &BodyCapture, headers: &[(String, String)]) -> BodyView {
    let h = |n: &str| headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(n)).map(|(_, v)| v.clone());
    let decoded = decode_body(&b.bytes, h("content-encoding").as_deref());
    let is_text = is_probably_text(&decoded.bytes);
    BodyView {
        size: b.size,
        stored: b.bytes.len(),
        truncated: b.truncated,
        content_type: h("content-type"),
        decoded_from: decoded.decoded_from.clone(),
        decode_partial: decoded.partial,
        is_text,
        text: is_text.then(|| String::from_utf8_lossy(&decoded.bytes).into_owned()),
        base64: base64::engine::general_purpose::STANDARD.encode(&decoded.bytes),
    }
}

#[derive(Serialize)]
pub struct RequestDetail {
    #[serde(flatten)]
    pub exchange: RequestHead,
    pub req: BodyView,
    pub resp: BodyView,
}

#[derive(Serialize)]
pub struct RequestHead {
    pub id: String,
    pub tunnel_id: String,
    pub ts: i64,
    pub duration_ms: Option<i64>,
    pub kind: String,
    pub method: String,
    pub host: String,
    pub path: String,
    pub status: Option<u16>,
    pub req_headers: Vec<(String, String)>,
    pub resp_headers: Vec<(String, String)>,
    pub error: Option<String>,
}

pub fn detail_of(x: CapturedExchange) -> RequestDetail {
    RequestDetail {
        req: body_view(&x.req_body, &x.req_headers),
        resp: body_view(&x.resp_body, &x.resp_headers),
        exchange: RequestHead {
            id: x.id,
            tunnel_id: x.tunnel_id,
            ts: x.started_at_ms,
            duration_ms: x.duration_ms,
            kind: x.kind.as_str().into(),
            method: x.method,
            host: x.host,
            path: x.path,
            status: x.status,
            req_headers: x.req_headers,
            resp_headers: x.resp_headers,
            error: x.error,
        },
    }
}

#[tauri::command]
pub fn get_request(core: CoreState<'_>, id: String) -> Res<Option<RequestDetail>> {
    Ok(core.db.get_request(&id).map_err(err)?.map(detail_of))
}

#[tauri::command]
pub fn clear_requests(core: CoreState<'_>, tunnel_id: Option<String>) -> Res<()> {
    core.db.clear_requests(tunnel_id.as_deref()).map_err(err)
}

#[tauri::command]
pub fn save_settings(core: CoreState<'_>, settings: Settings) -> Res<()> {
    let mut s = settings;
    s.log_ttl_days = s.log_ttl_days.clamp(1, 365);
    s.max_db_mb = s.max_db_mb.clamp(16, 10_240);
    s.max_body_kb = s.max_body_kb.min(16 * 1024);
    s.redact_headers = s.redact_headers.into_iter().map(|h| h.trim().to_string()).filter(|h| !h.is_empty()).collect();
    if !["system", "light", "dark"].contains(&s.theme.as_str()) {
        s.theme = "system".into();
    }
    core.apply_settings(s).map_err(err)
}

#[tauri::command]
pub async fn detect_cloudflared(core: CoreState<'_>) -> Res<Option<Binary>> {
    Ok(core.rediscover().await)
}

#[tauri::command]
pub fn login_item_status() -> LoginItem {
    platform::login_item()
}

#[tauri::command]
pub fn set_launch_at_login(enabled: bool) -> LoginItem {
    platform::set_login_item(enabled)
}

#[tauri::command]
pub fn open_dashboard(app: AppHandle, view: Option<String>) {
    tray::open_dashboard(&app, view);
}

#[tauri::command]
pub fn hide_popover(app: AppHandle) {
    tray::hide_popover(&app);
}

#[tauri::command]
pub fn quit(app: AppHandle) {
    app.exit(0);
}

/// The e2e harness asks the dashboard for its rendered text; the page answers here.
#[tauri::command]
pub fn e2e_report(nonce: String, text: String) {
    crate::e2e::report(nonce, text);
}
