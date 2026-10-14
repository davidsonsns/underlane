//! Explicit CLI provisioning; successful credentials survive a failed config link for retry.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::Emitter;
use underlane_core::cli::Cancellation;
use underlane_core::config::{home_dir, resolve_path, TunnelConfig};
use underlane_core::model::TunnelKind;
use underlane_core::secret::redact_text;
use underlane_core::tunnel_setup::{self, Stage};

use crate::hostnames;
use crate::state::Core;

struct Active {
    id: String,
    request: String,
    cancel: Cancellation,
    cancelled: Arc<AtomicBool>,
}

// Login writes one default account certificate, so provisioning is account-wide, not per row.
static ACTIVE: Mutex<Option<Active>> = Mutex::new(None);
// DNS creation and project setup may both launch login and write the default certificate.
pub(crate) static ACCOUNT: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

struct Guard {
    id: String,
    request: String,
}
impl Drop for Guard {
    fn drop(&mut self) {
        let mut active = ACTIVE.lock().unwrap();
        if active
            .as_ref()
            .is_some_and(|job| job.id == self.id && job.request == self.request)
        {
            *active = None;
        }
    }
}

#[derive(Clone, Serialize)]
struct Progress<'a> {
    id: &'a str,
    request_id: &'a str,
    stage: Stage,
}

#[derive(Serialize)]
pub struct SetupResult {
    pub tunnel_id: String,
    pub credentials_file: String,
}

struct Source {
    path: PathBuf,
    original: String,
    config: TunnelConfig,
    name: String,
}

fn source(core: &Core, id: &str) -> Result<Source, String> {
    let tunnel = core
        .db
        .tunnel(id)
        .map_err(|e| redact_text(&e.to_string()))?
        .ok_or("Project not found.")?;
    if tunnel.kind != TunnelKind::Config {
        return Err("Only config projects can set up a locally managed tunnel.".into());
    }
    if core.state_of(id).supervised || core.state_of(id).status.is_active() {
        return Err("Stop this project before setting up its tunnel.".into());
    }
    let path = hostnames::source_path(core, id)?;
    let metadata = fs::metadata(&path).map_err(|e| redact_text(&e.to_string()))?;
    if metadata.permissions().readonly() {
        return Err("The project config is read-only. Make it writable before setup.".into());
    }
    fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .map_err(|_| "The project config is not writable. Check its permissions before setup.")?;
    let original = fs::read_to_string(&path).map_err(|e| redact_text(&e.to_string()))?;
    let config = TunnelConfig::parse(&path, &original)
        .map_err(|_| "The project config is invalid YAML. Fix it before setting up a tunnel.")?;
    if !underlane_core::projects::needs_setup(&config) {
        return Err(
            "This project already has a tunnel. Refresh the project to see its latest setup."
                .into(),
        );
    }
    if config.ingress.is_empty() {
        return Err("Add hostname rules or a fallback service to this config before setup.".into());
    }
    if let Some(cert) = config
        .raw
        .as_mapping()
        .and_then(|map| map.get("origincert"))
        .and_then(|value| value.as_str())
    {
        let selected = resolve_path(path.parent().unwrap(), cert);
        let default = home_dir().join(".cloudflared/cert.pem");
        let same = selected == default
            || fs::canonicalize(&selected)
                .ok()
                .zip(fs::canonicalize(&default).ok())
                .is_some_and(|(a, b)| a == b);
        if !same {
            return Err("This config uses a different Cloudflare account certificate. Set up that tunnel with cloudflared directly, or use the default account certificate.".into());
        }
    }
    Ok(Source {
        path,
        original,
        config,
        name: tunnel.name,
    })
}

pub fn cancel(id: &str, request: &str) {
    if let Some(job) = ACTIVE
        .lock()
        .unwrap()
        .as_ref()
        .filter(|job| job.id == id && job.request == request)
    {
        job.cancelled.store(true, Ordering::SeqCst);
        job.cancel.cancel();
    }
}

pub async fn run(core: Arc<Core>, id: String, request: String) -> Result<SetupResult, String> {
    if crate::separate_tunnel::is_running(&id) {
        return Err("Finish creating the separate tunnel before setting up this project.".into());
    }
    let _account = ACCOUNT.try_lock().map_err(|_| "Another Cloudflare setup or DNS operation is running. Finish it before starting this one.")?;
    if request.is_empty()
        || request.len() > 80
        || !request
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err("Invalid setup request. Reopen the project and try again.".into());
    }
    let cancel = Cancellation::new();
    let cancelled = Arc::new(AtomicBool::new(false));
    {
        let mut active = ACTIVE.lock().unwrap();
        if active.is_some() {
            return Err(
                "Another tunnel is being set up. Finish or cancel it before starting this one."
                    .into(),
            );
        }
        *active = Some(Active {
            id: id.clone(),
            request: request.clone(),
            cancel: cancel.clone(),
            cancelled: cancelled.clone(),
        });
    }
    let _guard = Guard {
        id: id.clone(),
        request: request.clone(),
    };
    let binary = core.binary.read().unwrap().clone().ok_or(
        "Install cloudflared or choose its executable in Settings before setting up a tunnel.",
    )?;
    let load_core = core.clone();
    let load_id = id.clone();
    let source = tokio::task::spawn_blocking(move || source(&load_core, &load_id))
        .await
        .map_err(|e| redact_text(&e.to_string()))??;
    let validation = underlane_core::cloudflared::validate_cancellable(
        std::path::Path::new(&binary.path),
        &source.path,
        cancel.clone(),
    )
    .await;
    if cancelled.load(Ordering::SeqCst) {
        return Err("Tunnel setup was cancelled.".into());
    }
    if !validation.ok {
        return Err(redact_text(&format!(
            "The project config is not valid. {} {}",
            validation.raw,
            validation.hint.unwrap_or_default()
        )));
    }
    if tokio::fs::read_to_string(&source.path)
        .await
        .ok()
        .as_deref()
        != Some(source.original.as_str())
    {
        return Err(
            "The project config changed before setup. Retry with the current config.".into(),
        );
    }
    let identity = tunnel_setup::setup(
        std::path::Path::new(&binary.path), &core.data_dir, &home_dir(), &id, &source.name, cancel,
        |stage| { let _ = core.handle.emit("project-setup-progress", Progress { id: &id, request_id: &request, stage }); },
    ).await.map_err(|e| if cancelled.load(Ordering::SeqCst) {
        "Tunnel setup was cancelled. If a tunnel was already created, retry Set up tunnel to finish linking its credentials.".to_string()
    } else { redact_text(&e.to_string()) })?;
    // Share the source-write lock with hostname saves, but never hold it during browser login.
    let _save = hostnames::SAVES.lock().await;
    if cancelled.load(Ordering::SeqCst) {
        return Err("Setup was cancelled. If the tunnel was already created, Set up tunnel will finish linking it on retry.".into());
    }
    let linked_core = core.clone();
    let linked_id = id.clone();
    let result = SetupResult {
        tunnel_id: identity.tunnel_id.clone(),
        credentials_file: identity.credentials_file.to_string_lossy().into_owned(),
    };
    tokio::task::spawn_blocking(move || {
        if cancelled.load(Ordering::SeqCst) { return Err("Tunnel setup was cancelled before linking. Retry to finish linking the created tunnel.".to_string()); }
        if hostnames::source_path(&linked_core, &linked_id)? != source.path {
            return Err("The project config changed during setup. Retry to link the created tunnel to the current config.".to_string());
        }
        if linked_core.state_of(&linked_id).supervised || linked_core.state_of(&linked_id).status.is_active() {
            return Err("The project started during setup. Stop it and retry to link the created tunnel.".into());
        }
        let text = tunnel_setup::linked_config(&source.config, &identity).map_err(|e| redact_text(&e.to_string()))?;
        let candidate = hostnames::Candidate::new(&source.path, &text)?;
        if cancelled.load(Ordering::SeqCst) { return Err("Tunnel setup was cancelled before linking. Retry to finish linking the created tunnel.".to_string()); }
        hostnames::publish(&source.path, &source.original, &candidate.path)
            .map_err(|e| format!("The tunnel was created, but its config could not be linked. {e} Retry Set up tunnel to finish linking it."))?;
        linked_core.notify_changed();
        Ok::<_, String>(())
    }).await.map_err(|e| redact_text(&e.to_string()))??;
    Ok(result)
}
