//! DNS reads reuse the selected project's cloudflared login; credentials stay in the backend.

use std::collections::VecDeque;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use underlane_core::cli::{self, ConfigPolicy, Invocation, StdoutMode};
use underlane_core::config::{home_dir, resolve_path, TunnelConfig};
use underlane_core::dns::{DnsStatus as Status, HostnameDns};
use underlane_core::secret::{redact_text, Secret};
use underlane_core::tunnel_setup::{certificate_identity, CertificateIdentity};

use crate::{hostnames, setup, state::Core};

struct Source {
    config: TunnelConfig,
    cert: PathBuf,
    hostname: String,
    identity: String,
}

static NEXT_CERT: AtomicU64 = AtomicU64::new(0);

struct CertificateSnapshot(PathBuf);

impl CertificateSnapshot {
    fn new(directory: &Path, source: &Path) -> Result<Self, String> {
        let metadata = std::fs::metadata(source)
            .map_err(|_| "Could not read the Cloudflare login certificate.")?;
        if !metadata.is_file() || metadata.len() > 1024 * 1024 {
            return Err("The Cloudflare login certificate is invalid or too large.".into());
        }
        let bytes = Secret::new(
            std::fs::read(source)
                .map_err(|_| "Could not read the Cloudflare login certificate.")?,
        );
        if bytes.expose().len() > 1024 * 1024 {
            return Err("The Cloudflare login certificate is too large.".into());
        }
        for _ in 0..10 {
            let serial = NEXT_CERT.fetch_add(1, Ordering::Relaxed);
            let path = directory.join(format!(".dns-cert-{}-{serial}.pem", std::process::id()));
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = match options.open(&path) {
                Ok(file) => file,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(_) => {
                    return Err("Could not secure the login certificate for DNS creation.".into())
                }
            };
            // Both zone verification and CLI routing read this immutable app-owned snapshot.
            let snapshot = Self(path);
            file.write_all(bytes.expose())
                .map_err(|_| "Could not secure the login certificate for DNS creation.")?;
            return Ok(snapshot);
        }
        Err("Could not secure the login certificate for DNS creation. Retry.".into())
    }
}

impl Drop for CertificateSnapshot {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[derive(Deserialize)]
struct Credentials<'a> {
    #[serde(rename = "TunnelID")]
    id: &'a str,
    #[serde(rename = "AccountTag")]
    account: &'a str,
}

fn credentials(config: &TunnelConfig) -> Result<Option<(String, Secret<String>)>, String> {
    let Some(path) = &config.credentials_file else {
        return Ok(None);
    };
    let metadata =
        std::fs::metadata(path).map_err(|_| "Could not read this project's credentials file.")?;
    if !metadata.is_file() || metadata.len() > 1024 * 1024 {
        return Err("The project's credentials file is invalid or too large.".into());
    }
    let bytes = Secret::new(
        std::fs::read(path).map_err(|_| "Could not read this project's credentials file.")?,
    );
    let value: Credentials<'_> = serde_json::from_slice(bytes.expose())
        .map_err(|_| "The project's credentials file is invalid.")?;
    let id = uuid_string(value.id)?;
    if value.account.trim().is_empty() {
        return Err("The credentials have no Cloudflare account.".into());
    }
    Ok(Some((id, Secret::new(value.account.to_owned()))))
}

fn uuid_string(id: &str) -> Result<String, String> {
    underlane_core::dns::canonical_tunnel_id(id)
        .map_err(|_| "The project has no valid tunnel ID. Set up its tunnel first.".into())
}

fn source(core: &Core, id: &str, hostname: &str) -> Result<Source, String> {
    let config = TunnelConfig::load(&hostnames::source_path(core, id)?)
        .map_err(|_| "Could not load the project config. Fix it before managing DNS.")?;
    prepare(config, hostname)
}

fn prepare(config: TunnelConfig, hostname: &str) -> Result<Source, String> {
    let hostname = underlane_core::dns::canonical_hostname(hostname).map_err(|_| {
        "This hostname is invalid. Use Manage hostnames to fix it before managing DNS."
    })?;
    if !config.ingress.iter().any(|route| {
        route
            .hostname
            .as_deref()
            .is_some_and(|host| host.trim_end_matches('.').eq_ignore_ascii_case(&hostname))
    }) {
        return Err(
            "This hostname is no longer in the project. Refresh before managing DNS.".into(),
        );
    }
    let identity = config
        .tunnel
        .clone()
        .filter(|value| !value.trim().is_empty())
        .ok_or("Set up this project's tunnel before creating DNS records.")?;
    let cert = config
        .raw
        .as_mapping()
        .and_then(|map| map.get("origincert"))
        .and_then(|value| value.as_str())
        .map(|path| resolve_path(config.source.parent().unwrap_or(Path::new(".")), path))
        .unwrap_or_else(|| home_dir().join(".cloudflared/cert.pem"));
    Ok(Source {
        config,
        cert,
        hostname,
        identity,
    })
}

fn expected(source: &Source) -> Result<String, String> {
    if let Ok(id) = uuid_string(&source.identity) {
        return Ok(id);
    }
    credentials(&source.config)?.map(|(id, _)| id)
        .ok_or_else(|| "This config uses a tunnel name without a credentials file. Create DNS to resolve its tunnel ID through Cloudflare.".into())
}

fn unknown(message: String, can_create: bool) -> HostnameDns {
    let mut status = HostnameDns::unknown(&message);
    status.can_create = can_create;
    status
}

// Keep the service name stable so bundle ID changes do not orphan saved tokens.
const DNS_KEYCHAIN_SERVICE: &str = "com.davidsonsns.underlane.dns-read-token";

fn dns_key(cert: &CertificateIdentity) -> String {
    format!("{}:{}", cert.account.expose(), cert.zone_id)
}

fn dns_token(cert: &CertificateIdentity) -> Result<Option<Secret<String>>, String> {
    let entry = keyring::Entry::new(DNS_KEYCHAIN_SERVICE, &dns_key(cert))
        .map_err(|_| "The system keychain is unavailable. Unlock it before connecting DNS.")?;
    match entry.get_password() {
        Ok(token) => Ok(Some(Secret::new(token))),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(_) => {
            Err("Could not read DNS access from the system keychain. Unlock it and retry.".into())
        }
    }
}

fn account_context(selected: &Source) -> Result<CertificateIdentity, String> {
    let cert = certificate_identity(&selected.cert).map_err(|_| "A valid Cloudflare login certificate is required. Run cloudflared tunnel login for this domain, then retry.")?;
    if let Some((credential_id, credential_account)) = credentials(&selected.config)? {
        if cert.account.expose() != credential_account.expose() {
            return Err("This project's credentials and login certificate belong to different Cloudflare accounts. Use the matching certificate before creating DNS.".into());
        }
        if uuid_string(&selected.identity).is_ok_and(|id| id != credential_id) {
            return Err("The project's tunnel ID and credentials do not match. Fix the config before creating DNS.".into());
        }
    }
    Ok(cert)
}

async fn authoritative(
    core: &Core,
    selected: &Source,
    token: &Secret<String>,
    pinned: Option<&str>,
) -> Result<HostnameDns, String> {
    let cert = account_context(selected)?;
    underlane_core::dns::verify_zone(
        token,
        &cert.zone_id,
        cert.account.expose(),
        &selected.hostname,
    )
    .await
    .map_err(|e| e.to_string())?;
    let target = match pinned
        .map(str::to_owned)
        .map(Ok)
        .unwrap_or_else(|| expected(selected))
    {
        Ok(target) => target,
        Err(_) => resolve_remote(core, selected).await?,
    };
    underlane_core::dns::check_records(token, &cert.zone_id, &selected.hostname, &target)
        .await
        .map_err(|e| e.to_string())
}

/// Prefer the existing login; scoped DNS access remains an optional fallback.
async fn verified_records(
    core: &Core,
    selected: &Source,
    pinned: Option<&str>,
) -> Result<(HostnameDns, Secret<String>), String> {
    let cert = account_context(selected)?;
    match authoritative(core, selected, &cert.api_token, pinned).await {
        Ok(status) => Ok((status, cert.api_token)),
        Err(login_error) => {
            if let Some(token) = dns_token(&cert)? {
                let mut status = authoritative(core, selected, &token, pinned).await?;
                // Read-only fallback credentials never authorize repair.
                status.can_repair = false;
                Ok((status, token))
            } else {
                Err(login_error)
            }
        }
    }
}

async fn ensure_certificate(core: &Core, selected: &Source) -> Result<(), String> {
    if selected.cert.exists() {
        return Ok(());
    }
    if selected.cert != home_dir().join(".cloudflared/cert.pem") {
        return Err("The config's custom account certificate is missing. Log in with cloudflared for that certificate, then retry.".into());
    }
    let binary =
        core.binary.read().unwrap().clone().ok_or(
            "Install cloudflared or choose its executable in Settings before connecting DNS.",
        )?;
    let mut login = Invocation::new(
        vec![
            std::ffi::OsString::from("--origincert"),
            selected.cert.as_os_str().into(),
            "login".into(),
        ],
        ConfigPolicy::Neutral(core.data_dir.clone()),
    );
    login.stdout = StdoutMode::Secret;
    login.timeout = Duration::from_secs(300);
    cli::exec(Path::new(&binary.path), login)
        .await
        .map_err(|_| {
            "Cloudflare login did not finish. Retry and authorize the domain in your browser."
        })?;
    Ok(())
}

pub async fn check(core: Arc<Core>, id: String, hostname: String) -> Result<HostnameDns, String> {
    check_for(core, id, hostname, None).await
}

pub(crate) async fn check_expected(
    core: Arc<Core>,
    id: String,
    hostname: String,
    expected_id: String,
) -> Result<HostnameDns, String> {
    check_for(core, id, hostname, Some(uuid_string(&expected_id)?)).await
}

async fn check_for(
    core: Arc<Core>,
    id: String,
    hostname: String,
    pinned: Option<String>,
) -> Result<HostnameDns, String> {
    let load_core = core.clone();
    let selected = tokio::task::spawn_blocking(move || source(&load_core, &id, &hostname))
        .await
        .map_err(|_| "Could not load the DNS status.")?;
    let selected = match selected {
        Ok(selected) => selected,
        Err(message) if pinned.is_some() => {
            let mut status = HostnameDns::unknown(&message);
            status.expected_tunnel_id = pinned;
            return Ok(status);
        }
        Err(message) => return Err(message),
    };
    match verified_records(&core, &selected, pinned.as_deref()).await {
        Ok((status, _)) => return Ok(status),
        Err(message) => {
            // Public DNS is still useful when login is missing, expired, or scoped to another zone.
            let target = match pinned.map(Ok).unwrap_or_else(|| expected(&selected)) {
                Ok(target) => target,
                Err(_) => {
                    let mut status = unknown(message, false);
                    status.needs_dns_access = true;
                    return Ok(status);
                }
            };
            let mut status = underlane_core::dns::check(&selected.hostname, &target).await;
            status.can_create = false;
            status.needs_dns_access = true;
            if status.status == Status::Unknown {
                status.message = message;
            }
            Ok(status)
        }
    }
}

pub async fn connect(
    core: Arc<Core>,
    id: String,
    hostname: String,
    token: Secret<String>,
) -> Result<HostnameDns, String> {
    let _account = setup::ACCOUNT.try_lock().map_err(|_| {
        "Another Cloudflare setup or DNS operation is running. Finish it before connecting DNS."
    })?;
    if token.expose().trim().is_empty() || token.expose().len() > 4096 {
        return Err(
            "Enter a valid Cloudflare API token with Zone Read and DNS Read for this domain."
                .into(),
        );
    }
    let token = Secret::new(token.expose().trim().to_owned());
    let load_core = core.clone();
    let mut selected = tokio::task::spawn_blocking(move || source(&load_core, &id, &hostname))
        .await
        .map_err(|_| "Could not load the project config.")??;
    ensure_certificate(&core, &selected).await?;
    let snapshot = CertificateSnapshot::new(&core.data_dir, &selected.cert)?;
    selected.cert = snapshot.0.clone();
    let mut result = authoritative(&core, &selected, &token, None).await?;
    result.can_repair = false;
    let cert = account_context(&selected)?;
    keyring::Entry::new(DNS_KEYCHAIN_SERVICE, &dns_key(&cert))
        .and_then(|entry| entry.set_password(token.expose()))
        .map_err(|_| "Could not save DNS access in the system keychain. Unlock it and retry.")?;
    Ok(result)
}

#[derive(Deserialize)]
struct RemoteTunnel {
    #[serde(alias = "ID", alias = "Id")]
    id: String,
    #[serde(alias = "Name")]
    name: String,
    #[serde(default, alias = "deletedAt", alias = "DeletedAt")]
    deleted_at: Option<String>,
}

fn active(tunnel: &RemoteTunnel) -> bool {
    tunnel
        .deleted_at
        .as_deref()
        .is_none_or(|date| date.is_empty() || date.starts_with("0001-01-01T00:00:00"))
}

fn remote_identity(tunnels: &[RemoteTunnel], identity: &str) -> Result<String, String> {
    let requested_id = uuid_string(identity).ok();
    let matching = tunnels
        .iter()
        .filter(|tunnel| {
            active(tunnel)
                && match &requested_id {
                    Some(id) => uuid_string(&tunnel.id).is_ok_and(|other| *id == other),
                    None => tunnel.name == identity,
                }
        })
        .collect::<Vec<_>>();
    if matching.len() != 1 {
        return Err("Could not find this project's tunnel in the selected Cloudflare account. Check its tunnel ID and certificate.".into());
    }
    uuid_string(&matching[0].id)
}

async fn resolve_remote(core: &Core, selected: &Source) -> Result<String, String> {
    let binary =
        core.binary.read().unwrap().clone().ok_or(
            "Install cloudflared or choose its executable in Settings before managing DNS.",
        )?;
    let mut list = Invocation::new(
        vec![
            std::ffi::OsString::from("--origincert"),
            selected.cert.as_os_str().into(),
            "list".into(),
            "--output".into(),
            "json".into(),
        ],
        ConfigPolicy::Neutral(core.data_dir.clone()),
    );
    list.stdout = StdoutMode::Secret;
    list.timeout = Duration::from_secs(30);
    let tunnels: Vec<RemoteTunnel> = cli::exec(Path::new(&binary.path), list).await
        .map_err(|_| "Could not verify the tunnel in this Cloudflare account. Check your login and connection, then retry.")?
        .stdout.deserialize().map_err(|_| "Cloudflared returned an invalid tunnel list.")?;
    let target = remote_identity(&tunnels, &selected.identity)?;
    if let Some((credential_id, _)) = credentials(&selected.config)? {
        if credential_id != target {
            return Err("The project's credentials refer to a different tunnel. Fix the config before creating DNS.".into());
        }
    }
    Ok(target)
}

pub async fn create(core: Arc<Core>, id: String, hostname: String) -> Result<HostnameDns, String> {
    let _account = setup::ACCOUNT.try_lock().map_err(|_| {
        "Another Cloudflare setup or DNS operation is running. Finish it before creating DNS."
    })?;
    let load_core = core.clone();
    let load_id = id.clone();
    let mut selected = tokio::task::spawn_blocking(move || source(&load_core, &load_id, &hostname))
        .await
        .map_err(|_| "Could not load the project config.")??;
    let binary =
        core.binary.read().unwrap().clone().ok_or(
            "Install cloudflared or choose its executable in Settings before creating DNS.",
        )?;
    ensure_certificate(&core, &selected).await?;
    let snapshot = CertificateSnapshot::new(&core.data_dir, &selected.cert)?;
    selected.cert = snapshot.0.clone();
    let (_, token) = verified_records(&core, &selected, None).await?;
    let target = resolve_remote(&core, &selected).await?;
    let cert = account_context(&selected)?;
    let status =
        underlane_core::dns::check_records(&token, &cert.zone_id, &selected.hostname, &target)
            .await
            .map_err(|e| e.to_string())?;
    if status.status == Status::Conflict {
        return Err(status.message);
    }
    if status.status == Status::Connected {
        return Ok(status);
    }
    let fresh = source(&core, &id, &selected.hostname)?;
    if fresh.config.fingerprint != selected.config.fingerprint {
        return Err("The project config changed while checking DNS. Refresh and retry.".into());
    }
    underlane_core::dns::create(
        Path::new(&binary.path),
        &core.data_dir,
        &selected.cert,
        &target,
        &selected.hostname,
    )
    .await
    .map_err(|e| redact_text(&e.to_string()))
}

#[derive(Serialize)]
pub struct DnsRepairPreview {
    pub review_id: String,
    pub hostname: String,
    pub current_target: String,
    pub expected_target: String,
    pub current_tunnel_id: String,
    pub expected_tunnel_id: String,
    pub current_project_name: Option<String>,
    pub expires_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RepairBinding {
    path: PathBuf,
    config: underlane_core::config::ConfigFingerprint,
    cert_path: PathBuf,
    certificate: underlane_core::config::ConfigFingerprint,
    expected: String,
    run: u64,
    config_revision: u64,
    supervised: bool,
}

#[derive(Clone)]
struct RepairReview {
    review_id: String,
    project_id: String,
    binding: RepairBinding,
    record: underlane_core::dns::DnsRepairRecord,
    certificate: Arc<CertificateSnapshot>,
    expires_at: i64,
}

// Reviews are short lived and bounded, including their private certificate snapshots.
static REVIEWS: Mutex<VecDeque<RepairReview>> = Mutex::new(VecDeque::new());
const REVIEW_MS: i64 = 120_000;
const MAX_REVIEWS: usize = 16;

pub(crate) fn clear_repair_reviews() {
    REVIEWS.lock().unwrap().clear();
}

fn repair_binding(
    selected: &Source,
    state: &underlane_core::supervisor::TunnelState,
) -> Result<RepairBinding, String> {
    let expected = expected(selected)?;
    if state.supervised {
        let loaded = state.loaded_config.as_ref().ok_or(
            "The connector has no verified loaded config. Restart it, then review DNS repair again.")?;
        let loaded_path = std::fs::canonicalize(&loaded.source_path).map_err(|_| {
            "The loaded config is unavailable. Restart the connector before reviewing DNS repair."
        })?;
        if loaded_path != selected.config.source
            || loaded.fingerprint != selected.config.fingerprint
            || state.cloudflare_tunnel_id.as_deref() != Some(&expected)
        {
            return Err("The config on disk and the active connector differ. Restart the connector, then review DNS repair again.".into());
        }
    }
    let certificate =
        Secret::new(std::fs::read(&selected.cert).map_err(|_| {
            "Could not pin the login certificate. Log in and review DNS repair again."
        })?);
    if certificate.expose().len() > 1024 * 1024 {
        return Err("The login certificate is too large. Review your Cloudflare login.".into());
    }
    Ok(RepairBinding {
        path: selected.config.source.clone(),
        config: selected.config.fingerprint,
        cert_path: selected.cert.clone(),
        certificate: underlane_core::config::ConfigFingerprint::of(certificate.expose()),
        expected,
        run: state.run,
        config_revision: state.config_revision,
        supervised: state.supervised,
    })
}

fn repair_source(
    core: &Core,
    id: &str,
    hostname: &str,
) -> Result<(Source, RepairBinding, CertificateIdentity), String> {
    if crate::separate_tunnel::is_running(id) || crate::separate_tunnel::start_blocked(core, id)? {
        return Err("Finish separate tunnel recovery before reviewing or repairing DNS.".into());
    }
    let selected = source(core, id, hostname)?;
    let state = core
        .sup
        .config_state(id)
        .unwrap_or_else(|| core.state_of(id));
    let binding = repair_binding(&selected, &state)?;
    let cert = account_context(&selected)?;
    Ok((selected, binding, cert))
}

fn repair_unchanged(core: &Core, review: &RepairReview) -> Result<(), String> {
    if underlane_core::dns::checked_at() >= review.expires_at {
        return Err("DNS repair review expired. Review the current targets again.".into());
    }
    let (_, binding, cert) = repair_source(core, &review.project_id, &review.record.hostname)?;
    let pinned = certificate_identity(&review.certificate.0)
        .map_err(|_| "The reviewed login certificate is unavailable. Review DNS repair again.")?;
    binding_unchanged(&review.binding, &binding, &pinned, &cert)
}

fn binding_unchanged(
    reviewed: &RepairBinding,
    current: &RepairBinding,
    pinned: &CertificateIdentity,
    current_cert: &CertificateIdentity,
) -> Result<(), String> {
    if current != reviewed
        || current_cert.zone_id != pinned.zone_id
        || current_cert.account.expose() != pinned.account.expose()
    {
        return Err("The project, connector run, or Cloudflare account changed since review. Restart if needed, then review DNS repair again.".into());
    }
    Ok(())
}

fn known_project(core: &Core, current_id: &str, observed: &str) -> Option<String> {
    core.db
        .tunnels()
        .ok()?
        .into_iter()
        .filter(|tunnel| tunnel.id != current_id)
        .find_map(|tunnel| {
            let state = core
                .sup
                .config_state(&tunnel.id)
                .unwrap_or_else(|| core.state_of(&tunnel.id));
            let identity = if state.supervised {
                state.cloudflare_tunnel_id
            } else {
                core.spec(&tunnel.id)
                    .ok()
                    .and_then(|spec| match tunnel.kind {
                        underlane_core::model::TunnelKind::Config => tunnel
                            .config_path
                            .as_deref()
                            .and_then(|path| TunnelConfig::load(Path::new(path)).ok())
                            .and_then(|config| {
                                underlane_core::supervisor::config_tunnel_identity(&config)
                                    .ok()
                                    .flatten()
                            }),
                        underlane_core::model::TunnelKind::Token => spec.token.and_then(|token| {
                            underlane_core::supervisor::token_tunnel_identity(token.expose()).ok()
                        }),
                        _ => None,
                    })
            };
            (identity.as_deref() == Some(observed)).then_some(tunnel.name)
        })
}

pub async fn preview_repair(
    core: Arc<Core>,
    id: String,
    hostname: String,
) -> Result<DnsRepairPreview, String> {
    let _account = setup::ACCOUNT.try_lock().map_err(|_| {
        "Another Cloudflare operation is running. Finish it before reviewing DNS repair."
    })?;
    let _save = hostnames::SAVES.lock().await;
    let (mut selected, binding, _) = repair_source(&core, &id, &hostname)?;
    let snapshot = Arc::new(CertificateSnapshot::new(&core.data_dir, &selected.cert)?);
    let bytes = Secret::new(
        std::fs::read(&snapshot.0).map_err(|_| "Could not pin the reviewed login certificate.")?,
    );
    if underlane_core::config::ConfigFingerprint::of(bytes.expose()) != binding.certificate {
        return Err("The Cloudflare login changed during review. Review DNS repair again.".into());
    }
    let cert = certificate_identity(&snapshot.0).map_err(|_| {
        "The reviewed login certificate is invalid. Log in and review DNS repair again."
    })?;
    selected.cert = snapshot.0.clone();
    if resolve_remote(&core, &selected).await? != binding.expected {
        return Err(
            "The project's tunnel identity changed during review. Review DNS repair again.".into(),
        );
    }
    // Only the certificate's embedded credential is used, never the optional read-only token.
    underlane_core::dns::verify_zone(
        &cert.api_token,
        &cert.zone_id,
        cert.account.expose(),
        &selected.hostname,
    )
    .await
    .map_err(repair_error)?;
    let record = underlane_core::dns::review_repair(
        &cert.api_token,
        &cert.zone_id,
        &selected.hostname,
        &binding.expected,
    )
    .await
    .map_err(repair_error)?;
    let now = underlane_core::dns::checked_at();
    let review_id = format!(
        "{}-{now}-{}",
        std::process::id(),
        NEXT_CERT.fetch_add(1, Ordering::Relaxed)
    );
    let result = DnsRepairPreview {
        review_id: review_id.clone(),
        hostname: record.hostname.clone(),
        current_target: record.target.clone(),
        expected_target: format!("{}.cfargotunnel.com", binding.expected),
        current_tunnel_id: record.tunnel_id.clone(),
        expected_tunnel_id: binding.expected.clone(),
        current_project_name: known_project(&core, &id, &record.tunnel_id),
        expires_at: now + REVIEW_MS,
    };
    let review = RepairReview {
        review_id,
        project_id: id,
        binding,
        record,
        certificate: snapshot,
        expires_at: result.expires_at,
    };
    repair_unchanged(&core, &review)?;
    let mut reviews = REVIEWS.lock().unwrap();
    reviews.retain(|review| review.expires_at > now);
    while reviews.len() >= MAX_REVIEWS {
        reviews.pop_front();
    }
    let expires_review_id = review.review_id.clone();
    reviews.push_back(review);
    drop(reviews);
    // Expire the snapshot even when the user closes the dialog and makes no further requests.
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(REVIEW_MS as u64)).await;
        REVIEWS
            .lock()
            .unwrap()
            .retain(|review| review.review_id != expires_review_id);
    });
    Ok(result)
}

fn repair_error(error: anyhow::Error) -> String {
    let message = redact_text(&error.to_string());
    if message.contains("rejected") || message.contains("access") {
        format!("{message} DNS repair requires a Cloudflare login with DNS Write access. Log in with write access or review the record manually in Cloudflare.")
    } else {
        message
    }
}

pub async fn repair(core: Arc<Core>, id: String, review_id: String) -> Result<HostnameDns, String> {
    let _account = setup::ACCOUNT
        .try_lock()
        .map_err(|_| "Another Cloudflare operation is running. Finish it before repairing DNS.")?;
    let _save = hostnames::SAVES.lock().await;
    let review = {
        let mut reviews = REVIEWS.lock().unwrap();
        reviews.retain(|review| review.expires_at > underlane_core::dns::checked_at());
        reviews
            .iter()
            .find(|review| review.review_id == review_id && review.project_id == id)
            .cloned()
            .ok_or("DNS repair review is missing or expired. Review the current targets again.")?
    };
    repair_unchanged(&core, &review)?;
    let cert = certificate_identity(&review.certificate.0)
        .map_err(|_| "The reviewed login certificate is unavailable. Review DNS repair again.")?;
    underlane_core::dns::verify_zone(
        &cert.api_token,
        &cert.zone_id,
        cert.account.expose(),
        &review.record.hostname,
    )
    .await
    .map_err(repair_error)?;
    repair_unchanged(&core, &review)?;
    let mut selected = source(&core, &id, &review.record.hostname)?;
    selected.cert = review.certificate.0.clone();
    underlane_core::dns::repair_reviewed(
        &cert.api_token,
        &cert.zone_id,
        &review.record,
        &review.binding.expected,
        || async {
            repair_unchanged(&core, &review).map_err(anyhow::Error::msg)?;
            let target = resolve_remote(&core, &selected)
                .await
                .map_err(anyhow::Error::msg)?;
            if target != review.binding.expected {
                anyhow::bail!("The project's tunnel identity changed. Review DNS repair again.");
            }
            repair_unchanged(&core, &review).map_err(anyhow::Error::msg)
        },
        || repair_unchanged(&core, &review).map_err(anyhow::Error::msg),
    )
    .await
    .map_err(repair_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "00000000-0000-4000-8000-000000000001";

    fn config(text: &str) -> TunnelConfig {
        TunnelConfig::parse(Path::new("/tmp/underlane/config.yml"), text).unwrap()
    }

    #[test]
    fn only_current_hostname_rules_can_request_dns() {
        let text = format!("tunnel: {ID}\ningress:\n - hostname: APP.example.com\n   service: http://localhost:3000\n - service: http_status:404\n");
        let selected = prepare(config(&text), "app.example.com.").unwrap();
        assert_eq!(selected.hostname, "app.example.com");
        assert_eq!(expected(&selected).unwrap(), ID);
        assert!(prepare(config(&text), "other.example.com").is_err());
        assert!(prepare(config(&text), "--overwrite-dns").is_err());
        assert!(prepare(
            config("ingress:\n - hostname: app.example.com\n   service: http_status:404\n"),
            "app.example.com"
        )
        .is_err());
    }

    #[test]
    fn remote_resolution_ignores_deleted_and_ambiguous_tunnels() {
        let active =
            format!(r#"{{"id":"{ID}","name":"project","deleted_at":"0001-01-01T00:00:00Z"}}"#);
        let deleted =
            format!(r#"{{"id":"{ID}","name":"project","deleted_at":"2026-10-01T00:00:00Z"}}"#);
        let list: Vec<RemoteTunnel> =
            serde_json::from_str(&format!("[{deleted},{active}]")).unwrap();
        assert_eq!(remote_identity(&list, "project").unwrap(), ID);
        assert_eq!(remote_identity(&list, ID).unwrap(), ID);
        assert!(remote_identity(&list, "other").is_err());
        let duplicate: Vec<RemoteTunnel> =
            serde_json::from_str(&format!("[{active},{active}]")).unwrap();
        assert!(remote_identity(&duplicate, "project").is_err());
        let deleted: Vec<RemoteTunnel> = serde_json::from_str(&format!("[{deleted}]")).unwrap();
        assert!(remote_identity(&deleted, ID).is_err());
    }

    #[test]
    fn custom_certificate_paths_are_relative_to_source() {
        let selected = prepare(config(&format!("tunnel: {ID}\norigincert: account.pem\ningress:\n - hostname: app.example.com\n   service: http_status:404\n")), "app.example.com").unwrap();
        assert_eq!(selected.cert, Path::new("/tmp/underlane/account.pem"));
    }

    struct TestDir(PathBuf);
    impl TestDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "underlane-dns-test-{}-{}",
                std::process::id(),
                NEXT_CERT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn certificate(account: &str, zone: &str) -> String {
        use base64::Engine;
        let payload = serde_json::json!({"accountID":account,"zoneID":zone,"apiToken":"synthetic-certificate-token"});
        format!(
            "-----BEGIN ARGO TUNNEL TOKEN-----\n{}\n-----END ARGO TUNNEL TOKEN-----\n",
            base64::engine::general_purpose::STANDARD.encode(payload.to_string())
        )
    }

    #[test]
    fn snapshot_pins_certificate_during_external_replacement_and_cleans_up() {
        let dir = TestDir::new();
        let original = dir.0.join("cert.pem");
        std::fs::write(&original, certificate("account-a", "zone-a")).unwrap();
        let snapshot = CertificateSnapshot::new(&dir.0, &original).unwrap();
        let path = snapshot.0.clone();
        std::fs::write(&original, certificate("account-b", "zone-b")).unwrap();
        let pinned = certificate_identity(&path).unwrap();
        assert_eq!(pinned.account.expose(), "account-a");
        assert_eq!(pinned.zone_id, "zone-a");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        drop(snapshot);
        assert!(!path.exists());
        assert_eq!(certificate_identity(&original).unwrap().zone_id, "zone-b");
    }

    #[test]
    fn credential_account_and_tunnel_must_match_the_selected_project() {
        let dir = TestDir::new();
        let cert = dir.0.join("cert.pem");
        let creds = dir.0.join("credentials.json");
        std::fs::write(&cert, certificate("account-a", "zone-a")).unwrap();
        let yaml = format!("tunnel: {ID}\norigincert: {}\ncredentials-file: {}\ningress:\n - hostname: app.example.com\n   service: http_status:404\n", cert.display(), creds.display());
        let selected = prepare(config(&yaml), "app.example.com").unwrap();
        std::fs::write(&creds, serde_json::json!({"TunnelID":ID,"AccountTag":"account-b","TunnelSecret":"synthetic-secret"}).to_string()).unwrap();
        let error = account_context(&selected).err().unwrap();
        assert!(error.contains("different Cloudflare accounts"));
        assert!(!error.contains("synthetic"));
        std::fs::write(&creds, serde_json::json!({"TunnelID":"00000000-0000-4000-8000-000000000002","AccountTag":"account-a"}).to_string()).unwrap();
        assert!(account_context(&selected)
            .err()
            .unwrap()
            .contains("do not match"));
        std::fs::write(
            &creds,
            serde_json::json!({"TunnelID":ID,"AccountTag":"account-a"}).to_string(),
        )
        .unwrap();
        let context = account_context(&selected).unwrap();
        assert_eq!(context.api_token.expose(), "synthetic-certificate-token");
        assert!(!format!("{:?}", context.api_token).contains("synthetic-certificate-token"));
    }

    #[test]
    fn dns_credentials_are_keyed_by_account_and_zone() {
        let identity = |account: &str, zone: &str| CertificateIdentity {
            account: Secret::new(account.into()),
            zone_id: zone.into(),
            api_token: Secret::new("synthetic-certificate-token".into()),
        };
        let first = dns_key(&identity("account-a", "zone-a"));
        assert_ne!(first, dns_key(&identity("account-a", "zone-b")));
        assert_ne!(first, dns_key(&identity("account-b", "zone-a")));
        assert_eq!(first, dns_key(&identity("account-a", "zone-a")));
    }

    #[test]
    fn repair_review_rejects_config_drift_and_new_connector_runs() {
        use underlane_core::supervisor::{LoadedConfig, TunnelState};
        let dir = TestDir::new();
        let path = dir.0.join("config.yml");
        let cert_path = dir.0.join("cert.pem");
        let text = format!("tunnel: {ID}\norigincert: {}\ningress:\n - hostname: app.example.com\n   service: http://localhost:3000\n - service: http_status:404\n", cert_path.display());
        std::fs::write(&path, &text).unwrap();
        let path = std::fs::canonicalize(&path).unwrap();
        std::fs::write(&cert_path, certificate("account-a", "zone-a")).unwrap();
        let selected = prepare(TunnelConfig::load(&path).unwrap(), "app.example.com").unwrap();
        let mut state = TunnelState::stopped("project");
        state.supervised = true;
        state.run = 2;
        state.config_revision = 3;
        state.cloudflare_tunnel_id = Some(ID.into());
        state.loaded_config = Some(LoadedConfig {
            source_path: path.to_string_lossy().into_owned(),
            fingerprint: selected.config.fingerprint,
        });
        let reviewed = repair_binding(&selected, &state).unwrap();
        let cert = certificate_identity(&cert_path).unwrap();
        assert!(binding_unchanged(&reviewed, &reviewed, &cert, &cert).is_ok());
        state.run += 1;
        assert!(binding_unchanged(
            &reviewed,
            &repair_binding(&selected, &state).unwrap(),
            &cert,
            &cert
        )
        .is_err());
        state.run -= 1;
        state.config_revision += 1;
        assert!(binding_unchanged(
            &reviewed,
            &repair_binding(&selected, &state).unwrap(),
            &cert,
            &cert
        )
        .is_err());
        state.config_revision -= 1;
        let changed = prepare(
            TunnelConfig::parse(&path, &text.replace("3000", "4000")).unwrap(),
            "app.example.com",
        )
        .unwrap();
        assert!(repair_binding(&changed, &state)
            .unwrap_err()
            .contains("Restart"));
        state.cloudflare_tunnel_id = Some("00000000-0000-4000-8000-000000000002".into());
        assert!(repair_binding(&selected, &state).is_err());
        state.supervised = false;
        assert!(binding_unchanged(
            &reviewed,
            &repair_binding(&selected, &state).unwrap(),
            &cert,
            &cert
        )
        .is_err());
    }

    #[test]
    fn repair_review_is_bound_to_account_zone_and_certificate_bytes() {
        let dir = TestDir::new();
        let cert_path = dir.0.join("cert.pem");
        std::fs::write(&cert_path, certificate("account-a", "zone-a")).unwrap();
        let text = format!("tunnel: {ID}\norigincert: {}\ningress:\n - hostname: app.example.com\n   service: http_status:404\n", cert_path.display());
        let selected = prepare(config(&text), "app.example.com").unwrap();
        let state = underlane_core::supervisor::TunnelState::stopped("project");
        let reviewed = repair_binding(&selected, &state).unwrap();
        let cert = certificate_identity(&cert_path).unwrap();
        for (account, zone) in [("account-b", "zone-a"), ("account-a", "zone-b")] {
            std::fs::write(&cert_path, certificate(account, zone)).unwrap();
            let fresh = certificate_identity(&cert_path).unwrap();
            assert!(binding_unchanged(&reviewed, &reviewed, &cert, &fresh).is_err());
            let changed = repair_binding(&selected, &state).unwrap();
            assert!(binding_unchanged(&reviewed, &changed, &cert, &cert).is_err());
        }
        let denied = repair_error(anyhow::anyhow!("Cloudflare rejected DNS access"));
        assert!(denied.contains("DNS Write"));
        assert!(denied.contains("manually"));
    }
}
