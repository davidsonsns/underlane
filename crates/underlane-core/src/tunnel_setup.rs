//! Create or recover an app-owned tunnel identity. Source config publication belongs to the caller.

use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, ensure, Context};
use base64::Engine;
use serde::{Deserialize, Serialize};

use crate::cli::{self, Cancellation, ConfigPolicy, Invocation, StdoutMode};
use crate::secret::Secret;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Stage {
    Checking,
    Login,
    Creating,
    Linking,
}

pub struct SetupIdentity {
    pub tunnel_id: String,
    pub credentials_file: PathBuf,
}

/// Build a candidate without writing the source; the caller validates and publishes it.
pub fn linked_config(
    config: &crate::config::TunnelConfig,
    identity: &SetupIdentity,
) -> anyhow::Result<String> {
    ensure!(
        crate::projects::needs_setup(config),
        "This config already has a tunnel identity or uses a single origin."
    );
    reidentified_config(config, identity)
}

/// Retain every route/option while changing a locally managed tunnel's credentials.
/// Callers moving the file must first resolve paths against its original directory.
pub fn reidentified_config(
    config: &crate::config::TunnelConfig,
    identity: &SetupIdentity,
) -> anyhow::Result<String> {
    ensure!(!config.is_single_origin(), "Single-origin configs cannot be separated automatically.");
    let root = config.raw.as_mapping().context("The source config must be a YAML mapping.")?;
    ensure!(!root.contains_key("token") && !root.contains_key("token-file"), "Token-authenticated configs cannot be separated automatically.");
    let id =
        uuid::Uuid::parse_str(&identity.tunnel_id).context("The setup tunnel ID is invalid.")?;
    ensure!(!id.is_nil(), "The setup tunnel ID is invalid.");
    ensure!(
        identity.credentials_file.is_absolute(),
        "The setup credentials path must be absolute."
    );
    let path = identity
        .credentials_file
        .to_str()
        .context("The setup credentials path must be valid Unicode.")?;
    let mut raw = config.raw.clone();
    let map = raw
        .as_mapping_mut()
        .context("The source config must be a YAML mapping.")?;
    // cloudflared accepts credential aliases/content too; they must not retain the old identity.
    map.remove("credentials-contents");
    map.remove("cred-file");
    map.insert("tunnel".into(), id.to_string().into());
    map.insert("credentials-file".into(), path.into());
    serde_norway::to_string(&raw)
        .map_err(|_| anyhow::anyhow!("Could not serialize the linked config."))
}

#[derive(Deserialize)]
struct Credentials<'a> {
    #[serde(rename = "AccountTag")]
    account: &'a str,
    #[serde(rename = "TunnelSecret")]
    secret: &'a str,
    #[serde(rename = "TunnelID")]
    id: &'a str,
}

#[derive(Deserialize)]
struct Certificate<'a> {
    #[serde(rename = "accountID")]
    account: &'a str,
    #[serde(rename = "zoneID")]
    zone: &'a str,
    #[serde(rename = "apiToken")]
    token: &'a str,
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

#[derive(Deserialize, Serialize)]
struct SetupIntent {
    name: String,
    account_id: String,
}

fn read_intent(path: &Path, account: &Secret<String>) -> anyhow::Result<Option<SetupIntent>> {
    let directory = path
        .parent()
        .context("The setup intent directory is invalid.")?;
    match std::fs::symlink_metadata(directory) {
        Ok(metadata) => ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "The setup intent directory must be a regular directory."
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => bail!("Could not inspect the saved setup intent directory."),
    }
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => bail!("Could not inspect the saved setup intent."),
    };
    ensure!(metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() <= 4096, "The saved setup intent is invalid. It was preserved; restore a valid setup intent before retrying.");
    let bytes = std::fs::read(path).context("Could not read the saved setup intent.")?;
    let intent: SetupIntent = serde_json::from_slice(&bytes).map_err(|_| anyhow::anyhow!("The saved setup intent is invalid. It was preserved; restore a valid setup intent before retrying."))?;
    ensure!(crate::db::normalize_name(&intent.name).is_ok_and(|name| name == intent.name) && !intent.account_id.is_empty(), "The saved setup intent is invalid. It was preserved; restore a valid setup intent before retrying.");
    ensure!(intent.account_id == *account.expose(), "This project has an unfinished setup for a different Cloudflare account. Restore the original default certificate before retrying; its setup intent was preserved.");
    Ok(Some(intent))
}

fn write_intent(path: &Path, intent: &SetupIntent) -> anyhow::Result<()> {
    let directory = path
        .parent()
        .context("The setup intent directory is invalid.")?;
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(directory)
        .context("Could not create the setup intent directory.")?;
    let metadata = std::fs::symlink_metadata(directory)
        .context("Could not inspect the setup intent directory.")?;
    ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "The setup intent directory must be a regular directory."
    );
    let temporary = directory.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temporary)
            .context("Could not save the setup intent.")?;
        let bytes = serde_json::to_vec(intent)
            .map_err(|_| anyhow::anyhow!("Could not serialize the setup intent."))?;
        ensure!(
            bytes.len() <= 4096,
            "The setup intent is too large to save safely."
        );
        file.write_all(&bytes)
            .context("Could not save the setup intent.")?;
        file.sync_all()
            .context("Could not save the setup intent.")?;
        // Hard linking publishes a complete file atomically without overwriting another attempt.
        std::fs::hard_link(&temporary, path).context("Could not publish the setup intent. Retry setup to check for an existing intent before creating a tunnel.")?;
        #[cfg(unix)]
        std::fs::File::open(directory)
            .and_then(|directory| directory.sync_all())
            .context("Could not commit the setup intent before creating a tunnel.")?;
        Ok(())
    })();
    let _ = std::fs::remove_file(&temporary);
    result
}

impl RemoteTunnel {
    fn active(&self) -> bool {
        self.deleted_at
            .as_deref()
            .is_none_or(|date| date.is_empty() || date.starts_with("0001-01-01T00:00:00"))
    }
}

fn invalid_credentials() -> anyhow::Error {
    anyhow::anyhow!("The app-owned tunnel credentials are invalid. They were preserved; restore valid credentials before retrying.")
}

fn read_secret(path: &Path) -> anyhow::Result<Secret<Vec<u8>>> {
    let metadata = std::fs::metadata(path).context("Could not read the setup credential file.")?;
    ensure!(
        metadata.is_file() && metadata.len() <= 1024 * 1024,
        "The setup credential file is not a regular file or is too large."
    );
    Ok(Secret::new(
        std::fs::read(path).context("Could not read the setup credential file.")?,
    ))
}

/// Validate a login certificate without exposing its API token to callers or diagnostics.
pub fn certificate_account(path: &Path) -> anyhow::Result<Secret<String>> {
    Ok(certificate_identity(path)?.account)
}

pub struct CertificateIdentity {
    pub account: Secret<String>,
    pub zone_id: String,
    pub api_token: Secret<String>,
}

/// Keep the login token secret while exposing its account and zone binding for DNS access.
pub fn certificate_identity(path: &Path) -> anyhow::Result<CertificateIdentity> {
    let invalid = || {
        anyhow::anyhow!("The default Cloudflare certificate is invalid. Restore a valid cert.pem for the intended account before retrying; the existing certificate was preserved.")
    };
    let bytes = read_secret(path)?;
    let text = std::str::from_utf8(bytes.expose()).map_err(|_| invalid())?;
    let begin = "-----BEGIN ARGO TUNNEL TOKEN-----";
    let end = "-----END ARGO TUNNEL TOKEN-----";
    for block in text.split("-----BEGIN ").skip(1) {
        let kind = block
            .split_once("-----")
            .map(|(kind, _)| kind)
            .ok_or_else(invalid)?;
        ensure!(
            matches!(kind, "ARGO TUNNEL TOKEN" | "PRIVATE KEY" | "CERTIFICATE"),
            "{}",
            invalid()
        );
    }
    ensure!(
        text.matches(begin).count() == 1 && text.matches(end).count() == 1,
        "{}",
        invalid()
    );
    let body = text
        .split_once(begin)
        .and_then(|(_, rest)| rest.split_once(end))
        .map(|(body, _)| body)
        .ok_or_else(invalid)?;
    let encoded = Secret::new(
        body.bytes()
            .filter(|byte| !byte.is_ascii_whitespace())
            .collect::<Vec<_>>(),
    );
    let payload = Secret::new(
        base64::engine::general_purpose::STANDARD
            .decode(encoded.expose())
            .map_err(|_| invalid())?,
    );
    let cert: Certificate<'_> = serde_json::from_slice(payload.expose()).map_err(|_| invalid())?;
    ensure!(
        !cert.account.trim().is_empty()
            && !cert.zone.trim().is_empty()
            && !cert.token.trim().is_empty(),
        "{}",
        invalid()
    );
    Ok(CertificateIdentity {
        account: Secret::new(cert.account.to_owned()),
        zone_id: cert.zone.to_owned(),
        api_token: Secret::new(cert.token.to_owned()),
    })
}

pub(crate) fn credential_identity(path: &Path, account: &Secret<String>) -> anyhow::Result<SetupIdentity> {
    let metadata = std::fs::symlink_metadata(path)
        .context("Could not inspect app-owned tunnel credentials.")?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "The app-owned credentials path must be a regular file. The existing path was preserved."
    );
    let bytes = read_secret(path)?;
    let credentials: Credentials<'_> =
        serde_json::from_slice(bytes.expose()).map_err(|_| invalid_credentials())?;
    let id = uuid::Uuid::parse_str(credentials.id).map_err(|_| invalid_credentials())?;
    ensure!(
        !id.is_nil() && !credentials.account.trim().is_empty(),
        "{}",
        invalid_credentials()
    );
    let secret = Secret::new(
        base64::engine::general_purpose::STANDARD
            .decode(credentials.secret)
            .map_err(|_| invalid_credentials())?,
    );
    ensure!(secret.expose().len() >= 32, "{}", invalid_credentials());
    ensure!(credentials.account == account.expose(), "The app-owned tunnel credentials belong to a different Cloudflare account. They were preserved; restore the matching default certificate before retrying.");
    Ok(SetupIdentity {
        tunnel_id: id.to_string(),
        credentials_file: path.into(),
    })
}

fn invocation(
    args: Vec<OsString>,
    data: &Path,
    cancel: &Cancellation,
    timeout: Duration,
) -> Invocation {
    let mut invocation = Invocation::new(args, ConfigPolicy::Neutral(data.into()));
    invocation.stdout = StdoutMode::Secret;
    invocation.timeout = timeout;
    invocation.cancel = cancel.clone();
    invocation
}

async fn list(
    binary: &Path,
    data: &Path,
    cert: &Path,
    cancel: &Cancellation,
) -> anyhow::Result<Vec<RemoteTunnel>> {
    let args = vec![
        "--origincert".into(),
        cert.as_os_str().into(),
        "list".into(),
        "--output".into(),
        "json".into(),
    ];
    let output = cli::exec(binary, invocation(args, data, cancel, Duration::from_secs(30))).await
        .context("Could not verify tunnels with the default Cloudflare certificate. Check its account access and your connection, then retry.")?;
    Ok(output.stdout.deserialize()?)
}

fn verify_remote(identity: &SetupIdentity, tunnels: &[RemoteTunnel]) -> anyhow::Result<()> {
    ensure!(tunnels.iter().any(|tunnel| tunnel.active() && uuid::Uuid::parse_str(&tunnel.id).is_ok_and(|id| id.to_string() == identity.tunnel_id)),
        "The saved tunnel no longer exists in the certificate's Cloudflare account. Its credentials were preserved; restore the matching tunnel before retrying.");
    Ok(())
}

/// Caller must serialize account setup and check the selected source config before publishing.
/// Cancellation and publication failures preserve successful credentials for the next retry.
/// Production callers pass the native default home; login itself uses cloudflared's default home.
pub async fn setup(
    binary: &Path,
    data_dir: &Path,
    home_dir: &Path,
    project_id: &str,
    name: &str,
    cancel: Cancellation,
    progress: impl Fn(Stage),
) -> anyhow::Result<SetupIdentity> {
    setup_inner(binary, data_dir, home_dir, project_id, name, None, false, cancel, progress).await
}

/// Recovery uses a separate durable setup namespace and a pinned account certificate.
/// An ambiguous create is never repeated, even if a subsequent list has not caught up.
// Keep the existing setup call shape; recovery adds only its pinned certificate.
#[allow(clippy::too_many_arguments)]
pub async fn setup_separate(
    binary: &Path,
    data_dir: &Path,
    home_dir: &Path,
    attempt_id: &str,
    name: &str,
    certificate: &Path,
    cancel: Cancellation,
    progress: impl Fn(Stage),
) -> anyhow::Result<SetupIdentity> {
    setup_inner(binary, data_dir, home_dir, attempt_id, name, Some(certificate), true, cancel, progress).await
}

// Share the provisioning implementation without introducing a public options type.
#[allow(clippy::too_many_arguments)]
async fn setup_inner(
    binary: &Path,
    data_dir: &Path,
    home_dir: &Path,
    project_id: &str,
    name: &str,
    certificate: Option<&Path>,
    never_repeat_create: bool,
    cancel: Cancellation,
    progress: impl Fn(Stage),
) -> anyhow::Result<SetupIdentity> {
    ensure!(
        home_dir.is_absolute(),
        "The default home directory must be absolute."
    );
    let project_id = uuid::Uuid::parse_str(project_id).context("The project ID is invalid.")?;
    ensure!(!project_id.is_nil(), "The project ID is invalid.");
    let name = crate::db::normalize_name(name)?;
    let cert = certificate.map(Path::to_path_buf).unwrap_or_else(|| home_dir.join(".cloudflared/cert.pem"));
    let directory = home_dir.join(".cloudflared/underlane");
    let credentials = directory.join(format!("{project_id}.json"));
    progress(Stage::Checking);
    match std::fs::symlink_metadata(&cert) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            ensure!(certificate.is_none(), "The pinned Cloudflare certificate is missing. Retry recovery.");
            progress(Stage::Login);
            let args = vec!["--origincert".into(), cert.as_os_str().into(), "login".into()];
            cli::exec(binary, invocation(args, data_dir, &cancel, Duration::from_secs(300))).await
                .context("Cloudflare login did not complete. Retry setup to authorize the intended account.")?;
            ensure!(cert.is_file(), "Cloudflare login did not save cert.pem. Retry setup and finish authorization in the browser.");
        }
        Err(_) => bail!("Could not inspect the default Cloudflare certificate. Check its permissions, then retry."),
    }
    let account = certificate_account(&cert)?;
    let intent_path = data_dir
        .join("project-setup")
        .join(format!("{project_id}.json"));
    let intent = read_intent(&intent_path, &account)?;
    let create_name = intent
        .as_ref()
        .map_or(name.as_str(), |intent| intent.name.as_str());
    match std::fs::symlink_metadata(&directory) {
        Ok(metadata) => ensure!(metadata.is_dir() && !metadata.file_type().is_symlink(), "The app-owned credentials directory must be a regular directory."),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => bail!("Could not inspect the app-owned credentials directory. Check its permissions, then retry."),
    }
    let pending = match std::fs::symlink_metadata(&credentials) {
        Ok(_) => Some(credential_identity(&credentials, &account)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => bail!(
            "Could not inspect app-owned tunnel credentials. Check their permissions, then retry."
        ),
    };
    let tunnels = list(binary, data_dir, &cert, &cancel).await?;
    if let Some(identity) = pending {
        verify_remote(&identity, &tunnels)?;
        progress(Stage::Linking);
        return Ok(identity);
    }
    if tunnels
        .iter()
        .any(|tunnel| tunnel.active() && tunnel.name == create_name)
    {
        if intent.is_some() {
            bail!("A Cloudflare tunnel for this project's original setup name already exists, but its app-owned credentials are missing. Restore those credentials before retrying; renaming the project cannot recover an unfinished setup.");
        }
        bail!("A Cloudflare tunnel with this project name already exists, but this project has no app-owned credentials. Choose a different project name or restore its credentials before retrying.");
    }
    ensure!(!never_repeat_create || intent.is_none(), "A separate tunnel creation was already attempted, but its credentials are missing. Restore the saved credentials or check the tunnel in Cloudflare before retrying; another tunnel was not created.");
    if intent.is_none() {
        write_intent(
            &intent_path,
            &SetupIntent {
                name: name.clone(),
                account_id: account.expose().clone(),
            },
        )?;
    }
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(&directory)
        .context("Could not create the app-owned credentials directory.")?;
    progress(Stage::Creating);
    let args = vec![
        "--origincert".into(),
        cert.as_os_str().into(),
        "create".into(),
        "--credentials-file".into(),
        credentials.as_os_str().into(),
        "--output".into(),
        "json".into(),
        "--".into(),
        create_name.into(),
    ];
    // Never issue a second create here: failures may occur after Cloudflare accepted the first.
    cli::exec(binary, invocation(args, data_dir, &cancel, Duration::from_secs(30))).await
        .context("Tunnel creation did not complete. Retry setup to check for saved credentials; no second tunnel was created by this attempt.")?;
    let identity = credential_identity(&credentials, &account)?;
    let tunnels = list(binary, data_dir, &cert, &cancel).await?;
    verify_remote(&identity, &tunnels)?;
    progress(Stage::Linking);
    Ok(identity)
}
