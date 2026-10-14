//! A verified, app-owned cloudflared installation. System installations are never changed.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use underlane_core::cloudflared::Binary;

const RELEASE_URL: &str = "https://api.github.com/repos/cloudflare/cloudflared/releases/latest";
const MAX_ARCHIVE: usize = 80 * 1024 * 1024;
const MAX_BINARY: usize = 160 * 1024 * 1024;
static INSTALL_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static STAGING_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub fn managed_path(data_dir: &Path) -> PathBuf {
    data_dir.join("bin/cloudflared")
}

/// Readiness requires an executable that successfully identifies itself as cloudflared.
pub async fn validate(path: &Path) -> Result<Binary, String> {
    let metadata =
        fs::metadata(path).map_err(|_| "cloudflared was not found at this path.".to_string())?;
    if !metadata.is_file() {
        return Err("The cloudflared path must point to an executable file.".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err("The cloudflared file does not have executable permission.".into());
        }
    }
    let mut command = Command::new(path);
    command.arg("--version").env_clear();
    let output = run_bounded(command, 16 * 1024, Duration::from_secs(5))
        .await
        .map_err(|e| format!("cloudflared could not pass its version check: {e}"))?;
    let text = String::from_utf8(output)
        .map_err(|_| "cloudflared returned an invalid version response.".to_string())?;
    let version = text
        .lines()
        .find_map(|line| {
            let version = line
                .trim()
                .strip_prefix("cloudflared version ")?
                .split_whitespace()
                .next()?;
            let parts: Vec<_> = version.split('.').collect();
            (parts.len() == 3
                && parts
                    .iter()
                    .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())))
            .then(|| version.to_string())
        })
        .ok_or_else(|| "The executable did not identify itself as cloudflared.".to_string())?;
    Ok(Binary {
        path: path.to_string_lossy().into_owned(),
        version: Some(version),
        source: "managed".into(),
    })
}

pub async fn install(data_dir: &Path) -> Result<Binary, String> {
    let asset_name = platform_asset(std::env::consts::OS, std::env::consts::ARCH)?;
    let _install = INSTALL_LOCK
        .try_lock()
        .map_err(|_| "cloudflared installation is already in progress.".to_string())?;
    let destination = managed_path(data_dir);
    if let Ok(binary) = validate(&destination).await {
        return Ok(binary);
    }
    let metadata = download(RELEASE_URL, 2 * 1024 * 1024).await?;
    let release: Release = serde_json::from_slice(&metadata)
        .map_err(|_| "Cloudflare release metadata could not be read.".to_string())?;
    let asset = release_asset(&release, asset_name)?;
    let archive = download(&asset.browser_download_url, MAX_ARCHIVE).await?;
    verify_archive(&archive, asset)?;

    let staging = Staging::create(destination.parent().expect("managed binary has parent"))?;
    let archive_path = staging.0.join("cloudflared.tgz");
    write_private(&archive_path, &archive)?;
    let executable = extract(&archive_path).await?;
    let staged_binary = staging.0.join("cloudflared");
    write_private(&staged_binary, &executable)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&staged_binary, fs::Permissions::from_mode(0o700)).map_err(io_error)?;
    }
    let mut binary = validate(&staged_binary).await?;
    if binary.version.as_deref() != Some(release.tag_name.as_str()) {
        return Err(
            "The downloaded cloudflared version does not match its release. Retry installation."
                .into(),
        );
    }
    promote(&staged_binary, &destination)?;
    binary.path = destination.to_string_lossy().into_owned();
    Ok(binary)
}

fn platform_asset(os: &str, arch: &str) -> Result<&'static str, String> {
    match (os, arch) {
        ("macos", "aarch64") => Ok("cloudflared-darwin-arm64.tgz"),
        ("macos", "x86_64") => Ok("cloudflared-darwin-amd64.tgz"),
        _ => Err("In-app installation supports Apple Silicon and Intel Macs. Install cloudflared using the installation guide on this platform.".into()),
    }
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    size: usize,
    digest: Option<String>,
    browser_download_url: String,
}

fn release_asset<'a>(release: &'a Release, name: &str) -> Result<&'a Asset, String> {
    if release.tag_name.is_empty()
        || !release
            .tag_name
            .bytes()
            .all(|b| b.is_ascii_digit() || b == b'.')
    {
        return Err("Cloudflare returned an unsupported release version.".into());
    }
    let mut matches = release.assets.iter().filter(|a| a.name == name);
    let asset = matches
        .next()
        .ok_or_else(|| "This Cloudflare release has no download for this Mac.".to_string())?;
    if matches.next().is_some() || asset.size == 0 || asset.size > MAX_ARCHIVE {
        return Err("Cloudflare returned invalid download metadata.".into());
    }
    let expected = format!(
        "https://github.com/cloudflare/cloudflared/releases/download/{}/{name}",
        release.tag_name
    );
    if asset.browser_download_url != expected {
        return Err(
            "The cloudflared download does not point to an official Cloudflare release.".into(),
        );
    }
    checksum(asset)?;
    Ok(asset)
}

fn checksum(asset: &Asset) -> Result<&str, String> {
    let hash = asset
        .digest
        .as_deref()
        .and_then(|d| d.strip_prefix("sha256:"))
        .filter(|d| d.len() == 64 && d.bytes().all(|b| b.is_ascii_hexdigit()));
    hash.ok_or_else(|| {
        "Cloudflare did not provide a valid SHA-256 checksum. Install manually or retry later."
            .into()
    })
}

fn verify_archive(bytes: &[u8], asset: &Asset) -> Result<(), String> {
    if bytes.len() != asset.size || bytes.len() > MAX_ARCHIVE {
        return Err(
            "The cloudflared download was incomplete or too large. Retry installation.".into(),
        );
    }
    let actual = format!("{:x}", Sha256::digest(bytes));
    if !actual.eq_ignore_ascii_case(checksum(asset)?) {
        return Err(
            "The cloudflared download failed SHA-256 verification. Retry installation.".into(),
        );
    }
    Ok(())
}

async fn download(url: &str, limit: usize) -> Result<Vec<u8>, String> {
    let mut command = Command::new("/usr/bin/curl");
    // Ignore user curl configuration; redirects may only use TLS, including GitHub's asset CDN.
    command.env_clear().args([
        "-q",
        "--fail",
        "--silent",
        "--show-error",
        "--location",
        "--max-redirs",
        "5",
        "--proto",
        "=https",
        "--proto-redir",
        "=https",
        "--connect-timeout",
        "15",
        "--max-time",
        "120",
        "--user-agent",
        "Underlane-cloudflared-installer",
        "--header",
        "Accept: application/vnd.github+json",
        url,
    ]);
    run_bounded(command, limit, Duration::from_secs(125))
        .await
        .map_err(|e| format!("Could not download cloudflared: {e}"))
}

async fn extract(archive: &Path) -> Result<Vec<u8>, String> {
    let mut list = Command::new("/usr/bin/tar");
    list.env_clear().arg("-tzf").arg(archive);
    let names = run_bounded(list, 16 * 1024, Duration::from_secs(15)).await?;
    // No archive path is ever used as a filesystem destination, even for a malicious tar entry.
    if names != b"cloudflared\n" && names != b"./cloudflared\n" {
        return Err("The cloudflared archive must contain only the expected executable.".into());
    }
    let member = if names.starts_with(b"./") {
        "./cloudflared"
    } else {
        "cloudflared"
    };
    let mut command = Command::new("/usr/bin/tar");
    command.env_clear().arg("-xOzf").arg(archive).arg(member);
    let bytes = run_bounded(command, MAX_BINARY, Duration::from_secs(15)).await?;
    if bytes.is_empty() {
        return Err("The cloudflared archive did not contain a regular executable.".into());
    }
    Ok(bytes)
}

async fn read_bounded(reader: impl AsyncRead + Unpin, limit: usize) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    reader
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(io_error)?;
    if bytes.len() > limit {
        return Err("The response exceeded its size limit.".into());
    }
    Ok(bytes)
}

async fn run_bounded(
    mut command: Command,
    limit: usize,
    timeout: Duration,
) -> Result<Vec<u8>, String> {
    #[cfg(unix)]
    underlane_core::platform::configure_group(&mut command).map_err(io_error)?;
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(io_error)?;
    #[cfg(unix)]
    let _group = ProcessGroup(child.id().expect("spawned child has pid"));
    let stdout = child.stdout.take().expect("stdout is piped");
    let stderr = child.stderr.take().expect("stderr is piped");
    let result = tokio::time::timeout(timeout, async {
        let (stdout, stderr, status) = tokio::try_join!(
            read_bounded(stdout, limit),
            read_bounded(stderr, 16 * 1024),
            async { child.wait().await.map_err(io_error) }
        )?;
        if !status.success() {
            let diagnostic =
                underlane_core::cloudflared::safe_diagnostic(&String::from_utf8_lossy(&stderr));
            return Err(format!(
                "The command failed: {}",
                diagnostic.chars().take(1024).collect::<String>().trim()
            ));
        }
        Ok(stdout)
    })
    .await;
    match result {
        Ok(Ok(bytes)) => Ok(bytes),
        failure => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            failure.unwrap_or_else(|_| Err("The command timed out. Retry installation.".into()))
        }
    }
}

#[cfg(unix)]
struct ProcessGroup(u32);

#[cfg(unix)]
impl Drop for ProcessGroup {
    fn drop(&mut self) {
        let _ = underlane_core::platform::signal_group(self.0, underlane_core::platform::SIGKILL);
    }
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(io_error)?;
    file.write_all(bytes).map_err(io_error)?;
    file.sync_all().map_err(io_error)
}

fn promote(staged: &Path, destination: &Path) -> Result<(), String> {
    fs::rename(staged, destination).map_err(io_error)
}

struct Staging(PathBuf);

impl Staging {
    fn create(bin: &Path) -> Result<Self, String> {
        fs::create_dir_all(bin).map_err(io_error)?;
        if fs::symlink_metadata(bin)
            .map_err(io_error)?
            .file_type()
            .is_symlink()
        {
            return Err("The app's binary directory must not be a symbolic link.".into());
        }
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(io_error)?
            .as_nanos();
        let sequence = STAGING_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = bin.join(format!(
            ".install-{}-{nonce}-{sequence}",
            std::process::id()
        ));
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&path).map_err(io_error)?;
        Ok(Self(path))
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn io_error(error: impl std::fmt::Display) -> String {
    format!("cloudflared installation could not finish: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(bytes: &[u8]) -> Release {
        let name = "cloudflared-darwin-arm64.tgz";
        Release {
            tag_name: "2026.10.0".into(),
            assets: vec![Asset {
                name: name.into(),
                size: bytes.len(),
                digest: Some(format!("sha256:{:x}", Sha256::digest(bytes))),
                browser_download_url: format!(
                    "https://github.com/cloudflare/cloudflared/releases/download/2026.10.0/{name}"
                ),
            }],
        }
    }

    fn temporary() -> Staging {
        Staging::create(&std::env::temp_dir().join("underlane-installer-tests")).unwrap()
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    #[ignore = "downloads and executes an official Cloudflare release in a disposable directory"]
    async fn live_installs_verified_official_release() {
        let directory = temporary();
        let data_dir = directory.0.clone();
        let installed = install(&data_dir)
            .await
            .expect("official release installation");
        let destination = managed_path(&data_dir);
        assert_eq!(installed.path, destination.to_string_lossy());
        assert_eq!(installed.source, "managed");
        assert!(installed.version.is_some());
        assert_eq!(validate(&destination).await.unwrap(), installed);
        let remaining: Vec<_> = fs::read_dir(data_dir.join("bin"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(remaining, vec![std::ffi::OsString::from("cloudflared")]);
        println!(
            "Verified official cloudflared {} in disposable app data",
            installed.version.unwrap()
        );
        drop(directory);
        assert!(!data_dir.exists(), "disposable installation was removed");
    }

    #[test]
    fn selects_only_supported_platforms() {
        assert_eq!(
            platform_asset("macos", "aarch64").unwrap(),
            "cloudflared-darwin-arm64.tgz"
        );
        assert_eq!(
            platform_asset("macos", "x86_64").unwrap(),
            "cloudflared-darwin-amd64.tgz"
        );
        assert!(platform_asset("linux", "aarch64").is_err());
        assert!(platform_asset("macos", "unknown").is_err());
    }

    #[test]
    fn verifies_exact_asset_size_and_digest() {
        let metadata = release(b"archive");
        let asset = release_asset(&metadata, "cloudflared-darwin-arm64.tgz").unwrap();
        assert!(verify_archive(b"archive", asset).is_ok());
        assert!(verify_archive(b"Archive", asset)
            .unwrap_err()
            .contains("SHA-256"));
        assert!(verify_archive(b"archiv", asset).is_err());
    }

    #[test]
    fn rejects_untrusted_metadata_and_missing_checksums() {
        let name = "cloudflared-darwin-arm64.tgz";
        let mut metadata = release(b"archive");
        metadata.assets[0].browser_download_url = "http://github.com/cloudflare/cloudflared/releases/download/2026.10.0/cloudflared-darwin-arm64.tgz".into();
        assert!(release_asset(&metadata, name).is_err());
        metadata = release(b"archive");
        metadata.assets[0].browser_download_url = "https://github.com/attacker/cloudflared/releases/download/2026.10.0/cloudflared-darwin-arm64.tgz".into();
        assert!(release_asset(&metadata, name).is_err());
        metadata = release(b"archive");
        metadata.assets[0].digest = None;
        assert!(release_asset(&metadata, name).is_err());
        metadata.assets[0].digest = Some("sha256:invalid".into());
        assert!(release_asset(&metadata, name).is_err());
        metadata = release(b"archive");
        metadata.tag_name = "../escape".into();
        assert!(release_asset(&metadata, name).is_err());
    }

    #[test]
    fn promotion_preserves_previous_binary_on_failure_and_cleans_staging() {
        let directory = temporary();
        let destination = directory.0.join("cloudflared");
        fs::write(&destination, b"old").unwrap();
        let staging = Staging::create(&directory.0).unwrap();
        let stage_path = staging.0.clone();
        let staged_binary = stage_path.join("cloudflared");
        assert!(promote(&staged_binary, &destination).is_err());
        assert_eq!(fs::read(&destination).unwrap(), b"old");
        write_private(&staged_binary, b"new").unwrap();
        promote(&staged_binary, &destination).unwrap();
        assert_eq!(fs::read(&destination).unwrap(), b"new");
        drop(staging);
        assert!(!stage_path.exists());
    }

    #[tokio::test]
    async fn rejects_responses_over_limit() {
        assert_eq!(read_bounded(&b"abc"[..], 3).await.unwrap(), b"abc");
        assert!(read_bounded(&b"abcd"[..], 3).await.is_err());
    }

    #[cfg(unix)]
    fn fixture_executable(directory: &Path, script: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = directory.join("cloudflared");
        fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        path
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn readiness_requires_success_and_cloudflared_identity() {
        let directory = temporary();
        let path = fixture_executable(
            &directory.0,
            "echo 'cloudflared version 2026.10.0 (built test)'",
        );
        assert_eq!(
            validate(&path).await.unwrap().version.as_deref(),
            Some("2026.10.0")
        );
        fixture_executable(&directory.0, "echo 'cloudflared version 2026.10.0'; exit 1");
        assert!(validate(&path).await.is_err());
        fixture_executable(&directory.0, "echo 'some other tool version 2026.10.0'");
        assert!(validate(&path).await.is_err());
        fixture_executable(&directory.0, "echo 'cloudflared version arbitrary'");
        assert!(validate(&path).await.is_err());
        fs::remove_file(&path).unwrap();
        assert!(validate(&path).await.is_err());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn command_output_and_runtime_are_bounded() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "printf abcd"]);
        assert!(run_bounded(command, 3, Duration::from_secs(1))
            .await
            .is_err());
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "exec /bin/sleep 2"]);
        assert!(run_bounded(command, 3, Duration::from_millis(25))
            .await
            .unwrap_err()
            .contains("timed out"));
    }

    #[cfg(unix)]
    async fn fixture_archive(directory: &Path, members: &[&str]) -> PathBuf {
        let archive = directory.join("fixture.tgz");
        let mut command = Command::new("/usr/bin/tar");
        command
            .current_dir(directory)
            .arg("-czf")
            .arg(&archive)
            .args(members);
        run_bounded(command, 1024, Duration::from_secs(5))
            .await
            .unwrap();
        archive
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn extracts_only_single_expected_file_without_using_archive_paths() {
        let directory = temporary();
        fs::write(directory.0.join("cloudflared"), b"fixture executable").unwrap();
        let archive = fixture_archive(&directory.0, &["cloudflared"]).await;
        assert_eq!(extract(&archive).await.unwrap(), b"fixture executable");
        let archive = fixture_archive(&directory.0, &["cloudflared", "cloudflared"]).await;
        assert!(extract(&archive).await.is_err());
        fs::write(directory.0.join("unexpected"), b"not cloudflared").unwrap();
        let archive = fixture_archive(&directory.0, &["unexpected"]).await;
        assert!(extract(&archive).await.is_err());
        fs::remove_file(directory.0.join("cloudflared")).unwrap();
        std::os::unix::fs::symlink("unexpected", directory.0.join("cloudflared")).unwrap();
        let archive = fixture_archive(&directory.0, &["cloudflared"]).await;
        assert!(extract(&archive).await.is_err());
        assert_eq!(
            fs::read(directory.0.join("unexpected")).unwrap(),
            b"not cloudflared"
        );
    }
}
