//! Explicitly move one project's routes to a fresh locally managed tunnel.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::Emitter;
use underlane_core::cli::Cancellation;
use underlane_core::config::{home_dir, resolve_path, TunnelConfig};
use underlane_core::model::TunnelKind;
use underlane_core::secret::{redact_text, Secret};
use underlane_core::separate_tunnel::{self as recovery, Intent};
use underlane_core::tunnel_setup::{self, CertificateIdentity};

use crate::{hostnames, setup, state::Core};

static ACTIVE: Mutex<Option<String>> = Mutex::new(None);
static NEXT_CERT: AtomicU64 = AtomicU64::new(0);

pub fn is_running(id: &str) -> bool {
    ACTIVE.lock().unwrap().as_deref() == Some(id)
}

/// A partial DNS move must not revive the old connector, including after an app restart.
pub fn start_blocked(core: &Core, id: &str) -> Result<bool, String> {
    let Some(intent) = recovery::read_intent(&core.data_dir, id).map_err(safe)? else {
        return Ok(false);
    };
    let selected = core
        .db
        .tunnel(id)
        .map_err(safe)?
        .ok_or("Project not found.")?;
    Ok(selected.config_path.as_deref().map(Path::new)
        != Some(intent.config_path(&home_dir()).as_path()))
}

struct Guard {
    core: Arc<Core>,
}
impl Drop for Guard {
    fn drop(&mut self) {
        *ACTIVE.lock().unwrap() = None;
        self.core.notify_changed();
    }
}

#[derive(Serialize)]
pub struct Preview {
    pub revision: String,
    pub old_tunnel_id: String,
    pub hostnames: Vec<String>,
    pub config_path: String,
    pub resuming: bool,
}

#[derive(Serialize)]
pub struct ResultView {
    pub tunnel_id: String,
    pub config_path: String,
    pub moved_hostnames: Vec<String>,
    pub start_error: Option<String>,
}

#[derive(Clone, Serialize)]
struct Progress<'a> {
    id: &'a str,
    request_id: &'a str,
    stage: &'a str,
}

struct Source {
    config: TunnelConfig,
    original: String,
    selected_path: String,
    old_id: String,
    hostnames: Vec<String>,
    cert: PathBuf,
    intent: Option<Intent>,
    revision: String,
}

fn safe(error: impl std::fmt::Display) -> String {
    redact_text(&error.to_string())
}

fn check_runtime_ownership(
    kind: TunnelKind,
    name: &str,
    state: &underlane_core::supervisor::TunnelState,
    old_id: &str,
    hostnames: &[String],
) -> Result<(), String> {
    use underlane_core::supervisor::Status;
    if !state.supervised
        && !matches!(
            state.status,
            Status::Starting | Status::Connected | Status::Degraded
        )
    {
        return Ok(());
    }
    if kind == TunnelKind::Token && state.cloudflare_tunnel_id.as_deref() == Some(old_id) {
        return Err(format!("{name} is running this tunnel with remotely managed routes. Stop it and separate its routes in Cloudflare first."));
    }
    for route in &state.mappings {
        if let Some(hostname) = &route.hostname {
            let hostname = underlane_core::dns::canonical_hostname(hostname)
                .map_err(|_| format!("{name} has an invalid running hostname. Stop it before separating this tunnel."))?;
            if let Some(shared) = hostnames
                .iter()
                .find(|selected| recovery::overlaps(selected, &hostname))
            {
                return Err(format!("{shared} is still matched by the running config in {name}. Apply its hostname changes before separating this tunnel."));
            }
        } else if route.path.is_some() && state.cloudflare_tunnel_id.as_deref() == Some(old_id) {
            return Err(format!("{name} still has path-only rules running on this tunnel. Stop it or give those rules explicit hostnames first."));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use underlane_core::supervisor::{RouteMapping, Status, TunnelState};

    #[test]
    fn loaded_routes_retain_ownership_until_the_connector_stops() {
        let mut state = TunnelState::stopped("other");
        state.status = Status::Connected;
        state.supervised = true;
        state.cloudflare_tunnel_id = Some("old-identity".into());
        state.mappings = vec![RouteMapping {
            hostname: Some("*.example.com".into()),
            path: None,
            service: "http://localhost:3000".into(),
            inspected: false,
            note: None,
            route_key: None, control_supported: false, enabled: true,
        }];
        let names = vec!["app.example.com".into()];
        assert!(check_runtime_ownership(
            TunnelKind::Config,
            "other",
            &state,
            "old-identity",
            &names
        )
        .is_err());
        state.status = Status::Crashed; // A supervised retry still owns its loaded routes.
        assert!(check_runtime_ownership(
            TunnelKind::Config,
            "other",
            &state,
            "old-identity",
            &names
        )
        .is_err());
        state.supervised = false;
        assert!(check_runtime_ownership(
            TunnelKind::Config,
            "other",
            &state,
            "old-identity",
            &names
        )
        .is_ok());
    }

    #[test]
    fn running_token_identity_blocks_a_move_even_without_visible_routes() {
        let mut state = TunnelState::stopped("remote");
        state.supervised = true;
        state.cloudflare_tunnel_id = Some("old-identity".into());
        assert!(
            check_runtime_ownership(TunnelKind::Token, "remote", &state, "old-identity", &[])
                .is_err()
        );
        assert!(check_runtime_ownership(
            TunnelKind::Token,
            "remote",
            &state,
            "different-identity",
            &[]
        )
        .is_ok());
    }
}

fn source(core: &Core, id: &str) -> Result<Source, String> {
    let tunnel = core
        .db
        .tunnel(id)
        .map_err(safe)?
        .ok_or("Project not found.")?;
    if tunnel.kind != TunnelKind::Config {
        return Err("Only locally managed config projects can create a separate tunnel.".into());
    }
    let selected_path = tunnel
        .config_path
        .ok_or("This project has no config file.")?;
    let selected = hostnames::source_path(core, id)?;
    // A crash can leave a journal after the DB already switched to the completed owned config.
    // Clear it before reading the historical source, which may since have changed or disappeared.
    recovery::clear_completed_intent(&core.data_dir, &home_dir(), id, &selected).map_err(safe)?;
    let intent = recovery::read_intent(&core.data_dir, id).map_err(safe)?;
    let path = if let Some(intent) = &intent {
        if selected != intent.source_path && selected != intent.config_path(&home_dir()) {
            return Err("This project changed config during an unfinished tunnel recovery. Restore its original config before retrying.".into());
        }
        intent.source_path.clone()
    } else {
        selected
    };
    let original = fs::read_to_string(&path).map_err(|_| {
        "Could not read the original project config. Restore it before retrying recovery."
    })?;
    if intent
        .as_ref()
        .is_some_and(|intent| recovery::revision(original.as_bytes()) != intent.source_revision)
    {
        return Err("The original config changed during tunnel recovery. Restore its reviewed contents before retrying; saved credentials and DNS changes were preserved.".into());
    }
    let config = TunnelConfig::parse(&path, &original).map_err(safe)?;
    let hostnames = recovery::hostnames(&config).map_err(safe)?;
    let old_id = underlane_core::supervisor::config_tunnel_identity(&config)?
        .ok_or("Set up this project's locally managed tunnel before separating it.")?;
    if intent
        .as_ref()
        .is_some_and(|intent| intent.old_tunnel_id != old_id || intent.hostnames != hostnames)
    {
        return Err("The original project identity or hostnames changed during recovery. Restore them before retrying.".into());
    }
    let default_cert = home_dir().join(".cloudflared/cert.pem");
    let cert = config
        .raw
        .as_mapping()
        .and_then(|root| root.get("origincert"))
        .and_then(|value| value.as_str())
        .map(|cert| resolve_path(path.parent().unwrap_or(Path::new(".")), cert))
        .unwrap_or_else(|| default_cert.clone());
    let same = cert == default_cert
        || fs::canonicalize(&cert)
            .ok()
            .zip(fs::canonicalize(&default_cert).ok())
            .is_some_and(|(a, b)| a == b);
    if !same {
        return Err("This project uses a custom account certificate. Separate its tunnel with cloudflared using that account, or select the matching default certificate.".into());
    }
    let account = tunnel_setup::certificate_identity(&cert).map_err(|_| "A valid default Cloudflare login is required. Connect the correct account before creating a separate tunnel.")?;
    let credentials = config.credentials_file.as_ref().ok_or(
        "This config needs a credentials-file before its tunnel can be separated automatically.",
    )?;
    let old = recovery::saved_identity(credentials, &account.account)
        .map_err(safe)?
        .ok_or("The project's original credentials are missing. Restore them before recovery.")?;
    if old.tunnel_id != old_id {
        return Err(
            "The config and credentials name different tunnels. Fix them before recovery.".into(),
        );
    }
    // Bind the review to other imported configs as well: one hostname cannot belong to two rows.
    let mut reviewed = original.as_bytes().to_vec();
    reviewed.extend_from_slice(selected_path.as_bytes());
    if let Some(intent) = &intent {
        reviewed.extend_from_slice(intent.attempt_id.as_bytes());
    }
    for other in core
        .db
        .tunnels()
        .map_err(safe)?
        .into_iter()
        .filter(|other| other.id != id)
    {
        let running = core
            .sup
            .config_state(&other.id)
            .unwrap_or_else(|| core.state_of(&other.id));
        check_runtime_ownership(other.kind, &other.name, &running, &old_id, &hostnames)?;
        if other.kind == TunnelKind::Token {
            let remote_id = core
                .spec(&other.id)
                .map_err(safe)?
                .token
                .and_then(|token| {
                    underlane_core::supervisor::token_tunnel_identity(token.expose()).ok()
                })
                .or_else(|| core.state_of(&other.id).cloudflare_tunnel_id);
            if remote_id.as_deref() == Some(old_id.as_str()) {
                return Err(format!("{} uses this tunnel with remotely managed routes. Their hostname ownership cannot be checked safely; separate those routes in Cloudflare first.", other.name));
            }
            reviewed.extend_from_slice(other.id.as_bytes());
            if let Some(remote_id) = remote_id {
                reviewed.extend_from_slice(remote_id.as_bytes());
            }
            continue;
        }
        if other.kind != TunnelKind::Config {
            continue;
        }
        let Some(other_path) = other.config_path else {
            continue;
        };
        let other_text = fs::read_to_string(&other_path).map_err(|_| format!("Could not check hostname ownership in {}. Fix its config before separating this tunnel.", other.name))?;
        let other_config = TunnelConfig::parse(Path::new(&other_path), &other_text).map_err(|_| format!("Could not check hostname ownership in {}. Fix its config before separating this tunnel.", other.name))?;
        for route in &other_config.ingress {
            if let Some(hostname) = route.hostname.as_deref() {
                let hostname = underlane_core::dns::canonical_hostname(hostname).map_err(|_| {
                    format!(
                        "{} has an invalid hostname. Fix its config before separating this tunnel.",
                        other.name
                    )
                })?;
                if let Some(shared) = hostnames
                    .iter()
                    .find(|selected| recovery::overlaps(selected, &hostname))
                {
                    return Err(format!("{shared} is also matched by {}. Give each project distinct hostnames before separating their tunnels.", other.name));
                }
            } else if route.path.is_some()
                && underlane_core::supervisor::config_tunnel_identity(&other_config)
                    .ok()
                    .flatten()
                    .as_deref()
                    == Some(&old_id)
            {
                return Err(format!("{} has path-only rules on this tunnel. Give those rules explicit hostnames before separating it.", other.name));
            }
        }
        reviewed.extend_from_slice(other.id.as_bytes());
        reviewed.extend_from_slice(other_path.as_bytes());
        reviewed.extend_from_slice(other_text.as_bytes());
    }
    let revision = recovery::revision(&reviewed);
    Ok(Source {
        config,
        original,
        selected_path,
        old_id,
        hostnames,
        cert,
        intent,
        revision,
    })
}

pub fn preview(core: &Core, id: &str) -> Result<Preview, String> {
    if is_running(id) {
        return Err("A separate tunnel is already being created for this project.".into());
    }
    let source = source(core, id)?;
    Ok(Preview {
        revision: source.revision,
        old_tunnel_id: source.old_id,
        hostnames: source.hostnames,
        config_path: source.config.source.to_string_lossy().into_owned(),
        resuming: source.intent.is_some(),
    })
}

struct CertificateSnapshot {
    path: PathBuf,
    identity: CertificateIdentity,
}
impl CertificateSnapshot {
    fn new(core: &Core, source: &Path) -> Result<Self, String> {
        let bytes = Secret::new(
            fs::read(source)
                .map_err(|_| "Could not pin the Cloudflare certificate for recovery.")?,
        );
        if bytes.expose().len() > 1024 * 1024 {
            return Err("The Cloudflare certificate is too large.".into());
        }
        let serial = NEXT_CERT.fetch_add(1, Ordering::SeqCst);
        let path = core
            .data_dir
            .join("separate-cert")
            .join(format!("{}-{serial}.pem", std::process::id()));
        recovery::publish_new(&path, bytes.expose()).map_err(safe)?;
        let identity = match tunnel_setup::certificate_identity(&path) {
            Ok(identity) => identity,
            Err(error) => {
                let _ = fs::remove_file(&path);
                return Err(safe(error));
            }
        };
        Ok(Self { path, identity })
    }
}
impl Drop for CertificateSnapshot {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn unchanged(core: &Core, id: &str, expected: &Source, intent: &Intent) -> Result<(), String> {
    let fresh = source(core, id)?;
    if fresh.config.source != expected.config.source
        || fresh.original != expected.original
        || fresh.selected_path != expected.selected_path
        || fresh.old_id != expected.old_id
        || fresh.hostnames != intent.hostnames
    {
        return Err("The project changed during tunnel recovery. Retry with its reviewed config; any created tunnel and DNS changes were preserved.".into());
    }
    // Other projects' hostnames are rechecked by source() before every DNS mutation.
    if core.sup.is_running(id) {
        return Err("The project started during tunnel recovery. Stop it and retry.".into());
    }
    Ok(())
}

pub async fn run(
    core: Arc<Core>,
    id: String,
    revision: String,
    request: String,
) -> Result<ResultView, String> {
    tokio::time::timeout(std::time::Duration::from_secs(300), run_inner(core, id, revision, request)).await
        .map_err(|_| "Separate tunnel recovery timed out after five minutes. Retry Create separate tunnel to verify and resume any saved tunnel or DNS changes.".to_string())?
}

async fn run_inner(
    core: Arc<Core>,
    id: String,
    revision: String,
    request: String,
) -> Result<ResultView, String> {
    let _account = setup::ACCOUNT.try_lock().map_err(|_| {
        "Another Cloudflare operation is running. Finish it before creating a separate tunnel."
    })?;
    if request.is_empty()
        || request.len() > 80
        || !request
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err("Invalid recovery request. Reopen the project and try again.".into());
    }
    {
        let mut active = ACTIVE.lock().unwrap();
        if active.is_some() {
            return Err("Another separate tunnel recovery is running.".into());
        }
        *active = Some(id.clone());
    }
    let guard = Guard { core: core.clone() };
    core.notify_changed();
    let progress = |stage| {
        let _ = core.handle.emit(
            "project-separate-progress",
            Progress {
                id: &id,
                request_id: &request,
                stage,
            },
        );
    };
    progress("checking");
    // Holding SAVES prevents another in-app hostname edit from changing the reviewed source.
    let _save = hostnames::SAVES.lock().await;
    let source = source(&core, &id)?;
    if source.revision != revision {
        return Err("The project or hostname ownership changed since review. Review Create separate tunnel again.".into());
    }
    let binary =
        core.binary.read().unwrap().clone().ok_or(
            "Install cloudflared or select its executable before creating a separate tunnel.",
        )?;
    let validation =
        underlane_core::cloudflared::validate(Path::new(&binary.path), &source.config.source).await;
    if !validation.ok {
        return Err(safe(format!(
            "The project config is not valid. {}",
            validation.raw
        )));
    }
    let snapshot = CertificateSnapshot::new(&core, &source.cert)?;
    let original_credentials = source
        .config
        .credentials_file
        .as_ref()
        .ok_or("The project's original credentials are missing.")?;
    let original_identity =
        recovery::saved_identity(original_credentials, &snapshot.identity.account)
            .map_err(safe)?
            .ok_or("The project's original credentials are missing.")?;
    if original_identity.tunnel_id != source.old_id {
        return Err("The original credentials changed while preparing recovery. Retry with the matching account.".into());
    }
    let intent = match &source.intent {
        Some(intent) => intent.clone(),
        None => Intent::new(&id, &source.config, &source.original, &source.old_id).map_err(safe)?,
    };
    let saved = recovery::saved_identity(
        &intent.credentials_path(&home_dir()),
        &snapshot.identity.account,
    )
    .map_err(safe)?;
    // Verify every candidate before creating a remote resource. Missing names are created later.
    for hostname in &intent.hostnames {
        underlane_core::dns::verify_zone(
            &snapshot.identity.api_token,
            &snapshot.identity.zone_id,
            snapshot.identity.account.expose(),
            hostname,
        )
        .await
        .map_err(safe)?;
        recovery::check_dns(
            &snapshot.identity.api_token,
            &snapshot.identity.zone_id,
            hostname,
            &intent.old_tunnel_id,
            saved.as_ref().map(|identity| identity.tunnel_id.as_str()),
        )
        .await
        .map_err(|error| format!("{hostname}: {}", safe(error)))?;
    }
    // Save the exact source/hostnames before the first remote create or DNS write.
    if source.intent.is_none() {
        intent.save(&core.data_dir).map_err(safe)?;
    }
    core.stop(&id).await;
    unchanged(&core, &id, &source, &intent)?;
    let identity = tunnel_setup::setup_separate(
        Path::new(&binary.path),
        &core.data_dir,
        &home_dir(),
        &intent.attempt_id,
        &intent.name(),
        &snapshot.path,
        Cancellation::new(),
        |stage| {
            if stage == tunnel_setup::Stage::Creating {
                progress("creating")
            }
        },
    )
    .await
    .map_err(|error| {
        format!(
            "{} Retry Create separate tunnel to resume the saved recovery.",
            safe(error)
        )
    })?;
    if identity.tunnel_id == intent.old_tunnel_id {
        return Err("The recovered credentials still belong to the old tunnel. They were preserved; restore the separate tunnel credentials before retrying.".into());
    }
    unchanged(&core, &id, &source, &intent)?;
    let text = recovery::config_text(&source.config, &identity).map_err(safe)?;
    let target = intent.config_path(&home_dir());
    recovery::publish_new(&target, text.as_bytes()).map_err(safe)?;
    let validation = underlane_core::cloudflared::validate(Path::new(&binary.path), &target).await;
    if !validation.ok {
        return Err(safe(format!(
            "The separate tunnel config is not valid. {} Saved files were preserved for retry.",
            validation.raw
        )));
    }
    let mut moved = Vec::new();
    progress("dns");
    for hostname in &intent.hostnames {
        unchanged(&core, &id, &source, &intent)?;
        match recovery::move_dns(&snapshot.identity.api_token, &snapshot.identity.zone_id, hostname, &intent.old_tunnel_id, &identity.tunnel_id).await {
            Ok(_) => moved.push(hostname.clone()),
            Err(error) => return Err(format!("{hostname}: {} Recovery is saved. Retry Create separate tunnel to finish; hostnames already moved will not be moved again.", safe(error))),
        }
    }
    progress("linking");
    unchanged(&core, &id, &source, &intent)?;
    if fs::read(&target).ok().as_deref() != Some(text.as_bytes()) {
        return Err("The app-owned config changed during recovery. It was preserved; restore it before retrying.".into());
    }
    let mut tunnel = core
        .db
        .tunnel(&id)
        .map_err(safe)?
        .ok_or("The project was removed during recovery.")?;
    if tunnel.config_path.as_deref() != Some(&source.selected_path) {
        return Err(
            "The project changed config during recovery. Retry to review its current config."
                .into(),
        );
    }
    tunnel.config_path = Some(target.to_string_lossy().into_owned());
    core.db.upsert_tunnel(&tunnel).map_err(safe)?;
    recovery::clear_completed_intent(&core.data_dir, &home_dir(), &id, &target).map_err(safe)?;
    core.notify_changed();
    progress("starting");
    // Release the start guard only after every DNS move and the config pointer are committed.
    drop(guard);
    let start_error = core.start(&id).err().map(safe);
    Ok(ResultView {
        tunnel_id: identity.tunnel_id,
        config_path: target.to_string_lossy().into_owned(),
        moved_hostnames: moved,
        start_error,
    })
}
