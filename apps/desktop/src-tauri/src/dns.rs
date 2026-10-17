//! DNS reads reuse the selected project's cloudflared login; credentials stay in the backend.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
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
    HostnameDns {
        status: Status::Unknown,
        message,
        can_create,
        needs_dns_access: false,
    }
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
    let target = match expected(selected) {
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
) -> Result<(HostnameDns, Secret<String>), String> {
    let cert = account_context(selected)?;
    match authoritative(core, selected, &cert.api_token).await {
        Ok(status) => Ok((status, cert.api_token)),
        Err(login_error) => {
            if let Some(token) = dns_token(&cert)? {
                let status = authoritative(core, selected, &token).await?;
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
    let load_core = core.clone();
    let selected = tokio::task::spawn_blocking(move || source(&load_core, &id, &hostname))
        .await
        .map_err(|_| "Could not load the DNS status.")??;
    match verified_records(&core, &selected).await {
        Ok((status, _)) => return Ok(status),
        Err(message) => {
            // Public DNS is still useful when login is missing, expired, or scoped to another zone.
            let target = match expected(&selected) {
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
    let result = authoritative(&core, &selected, &token).await?;
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
    let (_, token) = verified_records(&core, &selected).await?;
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
}
