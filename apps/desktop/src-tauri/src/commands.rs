//! Tauri commands invoked from the UI.

use std::path::PathBuf;
use std::sync::Arc;

use base64::Engine;
use underlane_core::cloudflared::{self, Binary, Validation};
use underlane_core::config::{home_dir, TunnelConfig};
use underlane_core::db::{LogQuery, RequestSummary};
use underlane_core::decode::{decode_body, is_probably_text};
use underlane_core::import::{self, ImportReport};
use underlane_core::model::{LogLine, Settings, TunnelKind};
use underlane_core::proxy::capture::{BodyCapture, CapturedExchange};
use underlane_core::supervisor::{normalize_origin, ConfigChange};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime, State};
use tauri_plugin_opener::OpenerExt;

use crate::platform::{self, LoginItem};
use crate::state::{Core, Overview};
use crate::{tokens, tray};

type Res<T> = Result<T, String>;
type CoreState<'a> = State<'a, Arc<Core>>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Run database work off the main thread (sync Tauri commands run on it) and off the async
/// workers.
async fn blocking<T: Send + 'static>(core: &Arc<Core>, f: impl FnOnce(&Core) -> Res<T> + Send + 'static) -> Res<T> {
    let _operation = core.operations.try_read().map_err(|_| "App reset is in progress. Try again when setup opens.")?;
    let core = core.clone();
    tokio::task::spawn_blocking(move || f(&core)).await.map_err(err)?
}

#[tauri::command]
pub async fn overview(core: CoreState<'_>) -> Res<Overview> {
    blocking(&core, |c| c.overview().map_err(err)).await
}

#[tauri::command]
pub async fn check_config_change(core: CoreState<'_>, id: String) -> Res<Option<ConfigChange>> {
    blocking(&core, move |c| c.check_config_change(&id).map_err(err)).await
}

#[tauri::command]
pub async fn load_hostnames(core: CoreState<'_>, id: String) -> Res<underlane_core::hostnames::HostnameConfig> {
    blocking(&core, move |c| crate::hostnames::load(c, &id)).await
}

#[tauri::command]
pub async fn save_hostnames(
    core: CoreState<'_>, id: String, revision: String,
    routes: Vec<underlane_core::hostnames::HostnameRoute>, fallback_service: String,
) -> Res<crate::hostnames::SaveResult> {
    let result = crate::hostnames::save(core.inner().clone(), id.clone(), revision, routes, fallback_service).await;
    core.connections.invalidate(&id);
    result
}

#[tauri::command]
pub async fn setup_project_tunnel(core: CoreState<'_>, id: String, request_id: String) -> Res<crate::setup::SetupResult> {
    crate::setup::run(core.inner().clone(), id, request_id).await
}

#[tauri::command]
pub async fn preview_separate_tunnel(core: CoreState<'_>, id: String) -> Res<crate::separate_tunnel::Preview> {
    blocking(&core, move |c| crate::separate_tunnel::preview(c, &id)).await
}

#[tauri::command]
pub async fn create_separate_tunnel(core: CoreState<'_>, id: String, revision: String, request_id: String) -> Res<crate::separate_tunnel::ResultView> {
    crate::separate_tunnel::run(core.inner().clone(), id, revision, request_id).await
}

#[tauri::command]
pub async fn check_hostname_dns(core: CoreState<'_>, id: String, hostname: String) -> Res<underlane_core::dns::HostnameDns> {
    crate::dns::check(core.inner().clone(), id, hostname).await
}

#[tauri::command]
pub async fn create_hostname_dns(core: CoreState<'_>, id: String, hostname: String) -> Res<underlane_core::dns::HostnameDns> {
    core.connections.clear();
    let result = crate::dns::create(core.inner().clone(), id, hostname).await;
    core.connections.clear();
    core.notify_changed();
    result
}

#[tauri::command]
pub async fn connect_hostname_dns(core: CoreState<'_>, id: String, hostname: String, token: String) -> Res<underlane_core::dns::HostnameDns> {
    let result = crate::dns::connect(core.inner().clone(), id, hostname, underlane_core::secret::Secret::new(token)).await;
    core.connections.clear();
    core.notify_changed();
    result
}

#[tauri::command]
pub async fn check_connection(
    core: CoreState<'_>, id: String, hostname: Option<String>, rule_index: Option<usize>, test_path: Option<String>,
) -> Res<crate::diagnostics::TunnelConnection> {
    crate::diagnostics::check(core.inner().clone(), id, hostname, rule_index, test_path).await
}

#[tauri::command]
pub async fn preview_dns_repair(core: CoreState<'_>, id: String, hostname: String) -> Res<crate::dns::DnsRepairPreview> {
    crate::dns::preview_repair(core.inner().clone(), id, hostname).await
}

#[tauri::command]
pub async fn repair_hostname_dns(core: CoreState<'_>, id: String, review_id: String) -> Res<underlane_core::dns::HostnameDns> {
    core.connections.clear();
    let result = crate::dns::repair(core.inner().clone(), id, review_id).await;
    core.connections.clear();
    core.notify_changed();
    result
}

#[tauri::command]
pub fn cancel_project_setup(id: String, request_id: String) {
    crate::setup::cancel(&id, &request_id);
}

#[tauri::command]
pub async fn open_config_in_editor(core: CoreState<'_>, app: AppHandle, id: String) -> Res<()> {
    blocking(&core, move |c| {
        let tunnel = c.db.tunnel(&id).map_err(err)?.ok_or("Tunnel not found")?;
        if tunnel.kind != TunnelKind::Config { return Err("This tunnel has no imported config file.".into()); }
        let path = tunnel.config_path.ok_or("This tunnel has no config file path.")?;
        readable_config_file(&path)?;
        app.opener().open_path(path, None::<&str>)
            .map_err(|e| underlane_core::secret::redact_text(&format!("Could not open the config in its registered editor: {e}")))
    }).await
}

fn readable_config_file(path: &str) -> Res<()> {
    let file = std::fs::File::open(path).map_err(|e| underlane_core::secret::redact_text(&format!("Could not read the config file: {e}")))?;
    if !file.metadata().map_err(err)?.is_file() { return Err("The config path is not a file.".into()); }
    Ok(())
}

#[derive(Serialize)]
pub struct Candidate {
    pub path: String,
    pub name: String,
    pub group: String,
    pub hostnames: Vec<String>,
    pub imported: bool,
}

/// Config files in `~/.cloudflared`, for the import sheet.
#[tauri::command]
pub async fn scan_default(core: CoreState<'_>) -> Res<Vec<Candidate>> {
    blocking(&core, scan_default_inner).await
}

fn scan_default_inner(core: &Core) -> Res<Vec<Candidate>> {
    let dir = home_dir().join(".cloudflared");
    let mut out = Vec::new();
    for c in underlane_core::projects::available_configs(&core.db, &dir).map_err(err)? {
        let p = c.source.clone();
        let key = p.to_string_lossy().into_owned();
        out.push(Candidate {
            imported: false,
            name: c.display_name(),
            group: import::group_name_for(&p),
            hostnames: c.ingress.iter().filter_map(|r| r.hostname.as_deref().map(underlane_core::secret::redact_text)).collect(),
            path: key,
        });
    }
    Ok(out)
}

#[tauri::command]
pub async fn import_paths(core: CoreState<'_>, paths: Vec<String>) -> Res<ImportReport> {
    blocking(&core, move |core| {
        let level = core.settings.read().unwrap().default_loglevel.clone();
        let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
        let report = import::import_configs(&core.db, &paths, &level);
        core.notify_changed();
        Ok(report)
    })
    .await
}

#[tauri::command]
pub fn project_directory() -> String {
    underlane_core::projects::default_directory().to_string_lossy().into_owned()
}

#[tauri::command]
pub async fn preview_project_config(core: CoreState<'_>, path: String) -> Res<underlane_core::projects::ProjectConfigPreview> {
    blocking(&core, move |_| underlane_core::projects::preview(std::path::Path::new(&path)).map_err(err)).await
}

#[tauri::command]
pub async fn create_project(core: CoreState<'_>, name: String, group: String, config_path: Option<String>, directory: Option<String>) -> Res<String> {
    blocking(&core, move |core| {
        let level = core.settings.read().unwrap().default_loglevel.clone();
        let tunnel = underlane_core::projects::create(
            &core.db, &name, &group, config_path.as_deref().map(std::path::Path::new),
            directory.as_deref().map(std::path::Path::new), &level,
        ).map_err(|e| underlane_core::secret::redact_text(&e.to_string()))?;
        core.notify_changed();
        Ok(tunnel.id)
    }).await
}

#[tauri::command]
pub fn add_quick_tunnel(core: CoreState<'_>, name: String, origin: String, group: String, inspect: bool) -> Res<String> {
    let _operation = core.operations.try_read().map_err(|_| "App reset is in progress.")?;
    let origin = normalize_origin(&origin);
    if underlane_core::derived::split_service(&origin).is_none() {
        return Err(format!("`{origin}` isn't a local http:// or https:// address."));
    }
    let level = core.settings.read().unwrap().default_loglevel.clone();
    let name = if name.trim().is_empty() { origin.trim_start_matches("http://").trim_start_matches("https://").to_string() } else { name };
    let mut t = import::add_simple(&core.db, TunnelKind::Quick, &name, &group, Some(origin), &level).map_err(err)?;
    if inspect {
        t.inspect = true;
        core.db.upsert_tunnel(&t).map_err(err)?;
    }
    core.notify_changed();
    Ok(t.id)
}

#[tauri::command]
pub fn add_token_tunnel(core: CoreState<'_>, name: String, token: String, group: String) -> Res<String> {
    let _operation = core.operations.try_read().map_err(|_| "App reset is in progress.")?;
    let token = token.trim().trim_start_matches("cloudflared tunnel run --token ").trim().to_string();
    if token.len() < 20 {
        return Err("That doesn't look like a tunnel token. Copy it from the tunnel's install command in the Cloudflare dashboard.".into());
    }
    let level = core.settings.read().unwrap().default_loglevel.clone();
    let name = if name.trim().is_empty() { "Remote tunnel".to_string() } else { name };
    let t = import::add_simple(&core.db, TunnelKind::Token, &name, &group, None, &level).map_err(err)?;
    tokens::set(&t.id, &token).map_err(|e| format!("Couldn't save the token in the keychain: {e}"))?;
    core.db.kv_set(&format!("token:{}", t.id), "1").map_err(err)?;
    core.notify_changed();
    Ok(t.id)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelPatch {
    pub name: Option<String>,
    #[serde(default, deserialize_with = "deserialize_group_patch")]
    pub group: Option<Option<String>>,
    pub inspect: Option<bool>,
    pub autostart: Option<bool>,
    pub loglevel: Option<String>,
    pub origin_url: Option<String>,
    pub token: Option<String>,
}

// A missing group leaves membership unchanged; explicit null removes it.
fn deserialize_group_patch<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Option<Option<String>>, D::Error> {
    Option::<String>::deserialize(deserializer).map(Some)
}

#[tauri::command]
pub async fn create_group(core: CoreState<'_>, name: String) -> Res<String> {
    blocking(&core, move |c| {
        let group = c.db.ensure_group(&name).map_err(err)?;
        c.notify_changed();
        Ok(group.id)
    }).await
}

#[tauri::command]
pub async fn rename_group(core: CoreState<'_>, id: String, name: String) -> Res<()> {
    blocking(&core, move |c| {
        c.db.rename_group(&id, &name).map_err(err)?;
        c.notify_changed();
        Ok(())
    }).await
}

#[tauri::command]
pub async fn delete_group(core: CoreState<'_>, id: String) -> Res<()> {
    blocking(&core, move |c| {
        c.db.delete_empty_group(&id).map_err(err)?;
        c.notify_changed();
        Ok(())
    }).await
}

#[tauri::command]
pub async fn update_tunnel(core: CoreState<'_>, id: String, patch: TunnelPatch) -> Res<()> {
    blocking(&core, move |c| update_tunnel_inner(c, id, patch)).await
}

fn update_tunnel_inner(core: &Core, id: String, patch: TunnelPatch) -> Res<()> {
    if crate::separate_tunnel::is_running(&id) {
        return Err("Finish creating the separate tunnel before changing this project.".into());
    }
    let mut t = core.db.tunnel(&id).map_err(err)?.ok_or("Tunnel not found")?;
    let mut needs_restart = false;
    if let Some(name) = patch.name {
        t.name = underlane_core::db::normalize_name(&name).map_err(err)?;
    }
    if let Some(group) = patch.group {
        match group {
            None => t.group_id = None,
            Some(name) if !name.trim().is_empty() => t.group_id = Some(core.db.ensure_group(&name).map_err(err)?.id),
            Some(_) => {}
        }
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
        if underlane_core::derived::split_service(&o).is_none() {
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
    if needs_restart {
        core.restart_if_running(&id).map_err(err)?;
    }
    core.notify_changed();
    Ok(())
}

#[tauri::command]
pub async fn delete_tunnel(core: CoreState<'_>, id: String, delete_config: Option<bool>) -> Res<()> {
    let _operation = core.operations.try_read().map_err(|_| "App reset is in progress. Try again when setup opens.")?;
    if crate::separate_tunnel::is_running(&id) {
        return Err("Finish creating the separate tunnel before removing this project.".into());
    }
    let tunnel = core.db.tunnel(&id).map_err(err)?;
    core.stop(&id).await;
    if let Some(tunnel) = tunnel {
        if tunnel.kind == TunnelKind::Config && delete_config.unwrap_or(false) {
            if let Some(path) = tunnel.config_path.as_deref() {
                delete_config_file(path)?;
            }
        }
        if tunnel.kind == TunnelKind::Token {
            tokens::delete(&id);
        }
    }
    core.db.delete_tunnel(&id).map_err(err)?;
    core.sup.clear_route_selection(&id);
    core.states.lock().unwrap().remove(&id);
    core.connections.invalidate(&id);
    let _ = std::fs::remove_file(core.data_dir.join("derived").join(format!("{id}.yml")));
    core.notify_changed();
    Ok(())
}

fn delete_config_file(path: &str) -> Res<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("Could not delete config file `{path}`: {e}")),
    }
}

#[tauri::command]
pub async fn start_tunnel(core: CoreState<'_>, id: String) -> Res<()> {
    blocking(&core, move |c| c.start(&id).map_err(err)).await
}

#[tauri::command]
pub async fn start_hostname(core: CoreState<'_>, id: String, target: underlane_core::route_control::RouteTarget) -> Res<()> {
    blocking(&core, move |c| c.start_hostname(&id, target).map_err(err)).await
}

#[tauri::command]
pub async fn stop_hostname(core: CoreState<'_>, id: String, target: underlane_core::route_control::RouteTarget) -> Res<()> {
    let run = target.run;
    let target_id = id.clone();
    let last_hostname = blocking(&core, move |c| c.stop_hostname(&target_id, target).map_err(err)).await?;
    if last_hostname {
        core.sup.wait_for_stop(&id, run).await;
    }
    Ok(())
}

#[tauri::command]
pub async fn stop_tunnel(core: CoreState<'_>, id: String) -> Res<()> {
    core.stop(&id).await;
    Ok(())
}

#[tauri::command]
pub async fn restart_tunnel(core: CoreState<'_>, id: String) -> Res<()> {
    blocking(&core, move |c| c.restart(&id).map_err(err)).await
}

/// Restart every tunnel that is currently running.
pub fn restart_all_inner<R: Runtime>(app: &AppHandle<R>) {
    let core = app.state::<Arc<Core>>();
    for id in core.sup.running_ids() {
        let _ = core.restart(&id);
    }
}

#[tauri::command]
pub fn restart_all(app: AppHandle) {
    restart_all_inner(&app);
}

pub fn start_all_inner<R: Runtime>(app: &AppHandle<R>) {
    let core = app.state::<Arc<Core>>();
    if let Ok(ts) = core.db.tunnels() {
        for t in ts {
            if t.kind == TunnelKind::Config && t.config_path.as_deref()
                .and_then(|path| TunnelConfig::load(std::path::Path::new(path)).ok())
                .is_some_and(|config| underlane_core::projects::needs_setup(&config)) {
                continue;
            }
            let _ = core.start(&t.id);
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

/// For each route service, whether something accepts TCP connections at its address. None
/// when the service has no host:port to probe (`http_status:404`, `hello_world`, unix sockets).
#[tauri::command]
pub async fn probe_services(services: Vec<String>) -> Vec<Option<bool>> {
    let probes: Vec<_> = services.into_iter().map(|s| tokio::spawn(probe_service(s))).collect();
    let mut out = Vec::with_capacity(probes.len());
    for p in probes {
        out.push(p.await.ok().flatten());
    }
    out
}

async fn probe_service(service: String) -> Option<bool> {
    use underlane_core::connection::{probe_service, ServiceStatus};
    match probe_service(&service).await.status {
        ServiceStatus::Reachable => Some(true),
        ServiceStatus::Unreachable => Some(false),
        ServiceStatus::Unsupported => None,
    }
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
pub async fn query_logs(core: CoreState<'_>, args: LogArgs) -> Res<Vec<LogLine>> {
    blocking(&core, move |c| {
        c.db.query_logs(&LogQuery {
            tunnel_id: args.tunnel_id,
            levels: args.levels,
            search: args.search,
            before_id: args.before_id,
            after_id: args.after_id,
            limit: args.limit.unwrap_or(500),
        })
        .map_err(err)
    })
    .await
}

#[tauri::command]
pub async fn clear_logs(core: CoreState<'_>, tunnel_id: Option<String>) -> Res<()> {
    blocking(&core, move |c| c.db.clear_logs(tunnel_id.as_deref()).map_err(err)).await
}

#[tauri::command]
pub async fn list_requests(
    core: CoreState<'_>, tunnel_id: Option<String>, search: Option<String>, limit: Option<usize>,
    statuses: Option<Vec<u16>>, methods: Option<Vec<String>>,
) -> Res<Vec<RequestSummary>> {
    blocking(&core, move |c| c.db.list_requests_filtered(
        tunnel_id.as_deref(), search.as_deref(), &statuses.unwrap_or_default(), &methods.unwrap_or_default(), limit.unwrap_or(500),
    ).map_err(err)).await
}

#[tauri::command]
pub async fn request_capture_paused(core: CoreState<'_>, tunnel_id: String) -> Res<bool> {
    blocking(&core, move |c| c.request_capture_paused(&tunnel_id).map_err(err)).await
}

#[tauri::command]
pub async fn set_request_capture_paused(core: CoreState<'_>, tunnel_id: String, paused: bool) -> Res<()> {
    blocking(&core, move |c| c.set_request_capture_paused(&tunnel_id, paused).map_err(err)).await
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
        // The hex view only needs binary bodies; text goes over once.
        base64: if is_text { String::new() } else { base64::engine::general_purpose::STANDARD.encode(&decoded.bytes[..decoded.bytes.len().min(64 * 1024)]) },
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
pub async fn get_request(core: CoreState<'_>, id: String) -> Res<Option<RequestDetail>> {
    blocking(&core, move |c| Ok(c.db.get_request(&id).map_err(err)?.map(detail_of))).await
}

#[tauri::command]
pub async fn clear_requests(core: CoreState<'_>, tunnel_id: Option<String>) -> Res<()> {
    blocking(&core, move |c| c.db.clear_requests(tunnel_id.as_deref()).map_err(err)).await
}

#[tauri::command]
pub async fn save_settings(core: CoreState<'_>, settings: Settings) -> Res<()> {
    blocking(&core, move |c| save_settings_inner(c, settings)).await
}

fn save_settings_inner(core: &Core, settings: Settings) -> Res<()> {
    let mut s = settings;
    s.log_ttl_days = s.log_ttl_days.clamp(1, 365);
    s.max_db_mb = s.max_db_mb.clamp(16, 10_240);
    s.max_body_kb = s.max_body_kb.min(16 * 1024);
    s.background_opacity = s.background_opacity.clamp(50, 100);
    s.redact_headers = s.redact_headers.into_iter().map(|h| h.trim().to_string()).filter(|h| !h.is_empty()).collect();
    s.redact_query = s.redact_query.into_iter().map(|h| h.trim().to_string()).filter(|h| !h.is_empty()).collect();
    if !["system", "light", "dark"].contains(&s.theme.as_str()) {
        s.theme = "system".into();
    }
    core.apply_settings(s).map_err(err)
}

#[tauri::command]
pub async fn detect_cloudflared(core: CoreState<'_>) -> Res<Option<Binary>> {
    let _operation = core.operations.try_read().map_err(|_| "App reset is in progress.")?;
    Ok(core.rediscover().await)
}

#[tauri::command]
pub async fn install_cloudflared(core: CoreState<'_>) -> Res<Binary> {
    let installed = crate::installer::install(&core.data_dir).await?;
    // Installation is an explicit choice: select the managed copy even if a stale custom path exists.
    crate::onboarding::select_cloudflared(core, Some(installed.path)).await?
        .ok_or_else(|| "cloudflared was installed but could not be selected. Check again or choose its executable.".into())
}

// async = off the main thread: SMAppService calls are blocking XPC round-trips, and a
// blocked main thread freezes the webview's animations (the Switch thumb jumps).
#[tauri::command]
pub async fn login_item_status() -> LoginItem {
    platform::login_item()
}

#[tauri::command]
pub async fn set_launch_at_login(core: CoreState<'_>, enabled: bool) -> Res<LoginItem> {
    let _operation = core.operations.try_read().map_err(|_| "App reset is in progress.")?;
    Ok(platform::set_login_item(enabled))
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

#[tauri::command]
pub fn restart_app(app: AppHandle, core: CoreState<'_>) -> Res<()> {
    let _operation = core.operations.try_read().map_err(|_| "App reset is in progress. Try again when setup opens.")?;
    let running: std::collections::HashSet<_> = core.sup.restart_session_ids().into_iter().collect();
    crate::restart::save(&core.db, |id| running.contains(id))
        .map_err(|e| format!("Could not save running tunnels before restarting: {e}"))?;
    // Request an exit so RunEvent::Exit stops tunnels before the new app launches.
    app.request_restart();
    Ok(())
}

/// The e2e harness asks the dashboard for its rendered text; the page answers here.
#[tauri::command]
pub fn e2e_report(nonce: String, text: String) {
    crate::e2e::report(nonce, text);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_deletion_removes_only_the_file_and_reports_failures() {
        let root = std::env::temp_dir().join(format!("underlane-delete-config-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("config.yml");
        let credentials = root.join("credentials.json");
        std::fs::write(&path, "url: http://localhost:1\n").unwrap();
        std::fs::write(&credentials, "{}").unwrap();

        delete_config_file(path.to_str().unwrap()).unwrap();
        assert!(!path.exists());
        assert!(credentials.exists());
        delete_config_file(path.to_str().unwrap()).unwrap();
        let error = delete_config_file(root.to_str().unwrap()).unwrap_err();
        assert!(error.contains("Could not delete config file"));
        assert!(root.is_dir());

        std::fs::remove_file(credentials).unwrap();
        std::fs::remove_dir(root).unwrap();
    }

    #[test]
    fn group_patch_distinguishes_omitted_null_and_name() {
        let omitted: TunnelPatch = serde_json::from_str(r#"{"name":"Renamed","autostart":true}"#).unwrap();
        assert_eq!(omitted.group, None);
        assert_eq!(omitted.name.as_deref(), Some("Renamed"));
        assert_eq!(omitted.autostart, Some(true));
        let removed: TunnelPatch = serde_json::from_str(r#"{"group":null}"#).unwrap();
        assert_eq!(removed.group, Some(None));
        let assigned: TunnelPatch = serde_json::from_str(r#"{"group":"Personal"}"#).unwrap();
        assert_eq!(assigned.group, Some(Some("Personal".into())));
        assert!(serde_json::from_str::<TunnelPatch>(r#"{"group":false}"#).is_err());
    }

    #[test]
    fn editor_source_path_requires_a_readable_regular_file() {
        let root = std::env::temp_dir().join(format!("underlane-editor-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("disposable config.yml");
        std::fs::write(&path, "url: http://localhost:1\n").unwrap();
        assert!(readable_config_file(path.to_str().unwrap()).is_ok());
        assert!(readable_config_file(root.to_str().unwrap()).unwrap_err().contains("not a file"));
        std::fs::remove_file(&path).unwrap();
        assert!(readable_config_file(path.to_str().unwrap()).unwrap_err().contains("Could not read"));
        std::fs::remove_dir(&root).unwrap();
    }

    #[tokio::test]
    async fn probes_listening_and_closed_ports() {
        let open = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let closed = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let closed_port = closed.local_addr().unwrap().port();
        drop(closed);
        let got = probe_services(vec![
            format!("http://localhost:{}", open.local_addr().unwrap().port()),
            format!("http://127.0.0.1:{closed_port}"),
            "http_status:404".into(),
            format!("tcp://127.0.0.1:{}", open.local_addr().unwrap().port()),
        ])
        .await;
        assert_eq!(got, vec![Some(true), Some(false), None, Some(true)]);
    }
}
