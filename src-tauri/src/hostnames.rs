//! Explicit hostname saves; imports and ordinary starts still leave source files untouched.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use serde::Serialize;
use underlane_core::cloudflared;
use underlane_core::hostnames::{self, HostnameConfig, HostnameRoute};
use underlane_core::model::TunnelKind;
use underlane_core::secret::redact_text;

use crate::state::Core;

// Saves are infrequent; serializing them also covers tunnels sharing a config file.
static SAVES: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

#[derive(Serialize)]
pub struct SaveResult {
    pub restart_error: Option<String>,
}

fn source_path(core: &Core, id: &str) -> Result<PathBuf, String> {
    let tunnel = core
        .db
        .tunnel(id)
        .map_err(|e| redact_text(&e.to_string()))?
        .ok_or("Tunnel not found")?;
    if tunnel.kind != TunnelKind::Config {
        return Err("Hostnames can only be edited for locally managed config tunnels.".into());
    }
    let path = PathBuf::from(
        tunnel
            .config_path
            .ok_or("This tunnel has no config file.")?,
    );
    if fs::symlink_metadata(&path)
        .map_err(|e| redact_text(&format!("Could not read the config: {e}")))?
        .file_type()
        .is_symlink()
    {
        return Err(
            "This config is a symbolic link. Edit its original file with Open in Editor.".into(),
        );
    }
    fs::canonicalize(path).map_err(|e| redact_text(&format!("Could not locate the config: {e}")))
}

pub fn load(core: &Core, id: &str) -> Result<HostnameConfig, String> {
    hostnames::load(&source_path(core, id)?).map_err(|e| redact_text(&e.to_string()))
}

pub async fn save(
    core: Arc<Core>,
    id: String,
    revision: String,
    routes: Vec<HostnameRoute>,
    fallback_service: String,
) -> Result<SaveResult, String> {
    let _guard = SAVES.lock().await;
    let path = source_path(&core, &id)?;
    let prepared = hostnames::prepare(&path, &revision, &routes, &fallback_service)
        .map_err(|e| redact_text(&e.to_string()))?;
    if !prepared.changed {
        return Ok(SaveResult {
            restart_error: None,
        });
    }
    let binary = core.binary.read().unwrap().clone().ok_or(
        "Install cloudflared or select its executable in Settings before saving hostnames.",
    )?;
    let candidate = Candidate::new(&path, &prepared.text)?;
    validate(&binary.path, &candidate.path).await?;
    // A tunnel can be removed or its source changed while validation is running.
    if source_path(&core, &id)? != path {
        return Err("The tunnel config changed while saving. Reopen Manage hostnames.".into());
    }
    publish(&path, &prepared.original, &candidate.path)?;
    core.notify_changed();
    let restart_error = core
        .restart_if_running(&id)
        .err()
        .map(|e| redact_text(&e.to_string()));
    Ok(SaveResult { restart_error })
}

async fn validate(binary: &str, path: &Path) -> Result<(), String> {
    let validation = cloudflared::validate(Path::new(binary), path).await;
    if validation.ok {
        return Ok(());
    }
    Err(redact_text(&format!(
        "Could not save hostnames: {}{}",
        validation.raw,
        validation
            .hint
            .map(|hint| format!(" {hint}"))
            .unwrap_or_default()
    )))
}

struct Candidate {
    path: PathBuf,
}

impl Candidate {
    fn new(source: &Path, text: &str) -> Result<Self, String> {
        let (path, mut file) = unique_file(source, "pending")?;
        let candidate = Self { path };
        file.write_all(text.as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|e| redact_text(&format!("Could not prepare the config: {e}")))?;
        Ok(candidate)
    }
}

impl Drop for Candidate {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn unique_file(source: &Path, purpose: &str) -> Result<(PathBuf, fs::File), String> {
    let directory = source
        .parent()
        .ok_or("The config has no parent directory.")?;
    let name = source
        .file_name()
        .ok_or("The config has no file name.")?
        .to_string_lossy();
    for _ in 0..10 {
        let serial = NEXT_FILE.fetch_add(1, Ordering::Relaxed);
        let extension = if purpose == "backup" { "bak" } else { "tmp" };
        let path = directory.join(format!(
            ".{name}.underlane-{purpose}-{}-{serial}.{extension}",
            std::process::id()
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => {
                return Err(redact_text(&format!(
                    "Could not create a config {purpose} file: {e}"
                )))
            }
        }
    }
    Err("Could not create a unique config file. Try saving again.".into())
}

fn check_source(source: &Path, original: &str) -> Result<fs::Metadata, String> {
    let metadata = fs::symlink_metadata(source)
        .map_err(|e| redact_text(&format!("Could not read the config: {e}")))?;
    if !metadata.is_file() || metadata.permissions().readonly() {
        return Err("The config must be a writable regular file. Check its permissions.".into());
    }
    let current = fs::read_to_string(source)
        .map_err(|e| redact_text(&format!("Could not read the config: {e}")))?;
    if current != original {
        return Err("The config changed while this editor was open. Reload before saving; your edits have not been written.".into());
    }
    Ok(metadata)
}

fn publish(source: &Path, original: &str, candidate: &Path) -> Result<(), String> {
    let metadata = check_source(source, original)?;
    let (backup_path, mut backup) = unique_file(source, "backup")?;
    let result = (|| {
        backup
            .write_all(original.as_bytes())
            .and_then(|_| backup.sync_all())
            .map_err(|e| redact_text(&format!("Could not back up the config: {e}")))?;
        preserve_metadata(source, candidate, &metadata)?;
        // External editors don't take SAVES; compare again immediately before replacement.
        check_source(source, original)?;
        fs::rename(candidate, source)
            .map_err(|e| redact_text(&format!("Could not replace the config: {e}")))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(backup_path);
    }
    result
}

fn preserve_metadata(
    source: &Path,
    candidate: &Path,
    metadata: &fs::Metadata,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use std::os::fd::AsRawFd;
        extern "C" {
            fn fcopyfile(
                from: std::ffi::c_int,
                to: std::ffi::c_int,
                state: *mut std::ffi::c_void,
                flags: u32,
            ) -> std::ffi::c_int;
        }
        let input = fs::File::open(source).map_err(|e| redact_text(&e.to_string()))?;
        let output = OpenOptions::new()
            .write(true)
            .open(candidate)
            .map_err(|e| redact_text(&e.to_string()))?;
        // copyfile.h: ACL | STAT | XATTR, without DATA (the candidate contents stay intact).
        if unsafe {
            fcopyfile(
                input.as_raw_fd(),
                output.as_raw_fd(),
                std::ptr::null_mut(),
                0b111,
            )
        } != 0
        {
            return Err(redact_text(&format!(
                "Could not preserve config metadata: {}",
                std::io::Error::last_os_error()
            )));
        }
        output
            .set_times(fs::FileTimes::new().set_modified(std::time::SystemTime::now()))
            .map_err(|e| {
                redact_text(&format!(
                    "Could not update the config modification time: {e}"
                ))
            })?;
    }
    fs::set_permissions(candidate, metadata.permissions())
        .map_err(|e| redact_text(&format!("Could not preserve config permissions: {e}")))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let updated = fs::metadata(candidate).map_err(|e| redact_text(&e.to_string()))?;
        if updated.uid() != metadata.uid() || updated.gid() != metadata.gid() {
            return Err("Could not preserve the config's owner and group. Edit the file with Open in Editor instead.".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestDir(PathBuf);
    impl TestDir {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "underlane-hostname-save-{}-{}",
                std::process::id(),
                NEXT_FILE.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&root).unwrap();
            Self(root)
        }
    }
    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn publish_keeps_exact_backup_and_permissions() {
        let dir = TestDir::new();
        let source = dir.0.join("config.yml");
        let original = "# my comments\ningress:\n - service: http_status:404\n";
        fs::write(&source, original).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&source, fs::Permissions::from_mode(0o640)).unwrap();
        }
        let updated = "ingress:\n - service: http_status:403\n";
        let candidate = Candidate::new(&source, updated).unwrap();
        publish(&source, original, &candidate.path).unwrap();
        assert_eq!(fs::read_to_string(&source).unwrap(), updated);
        let backup = fs::read_dir(&dir.0)
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| p != &source)
            .unwrap();
        assert_eq!(fs::read_to_string(backup).unwrap(), original);
        assert_eq!(
            underlane_core::config::scan_dir(&dir.0),
            vec![source.clone()]
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&source).unwrap().permissions().mode() & 0o777,
                0o640
            );
        }
    }

    #[test]
    fn conflict_never_publishes_or_keeps_a_backup() {
        let dir = TestDir::new();
        let source = dir.0.join("config.yml");
        fs::write(&source, "external edit").unwrap();
        let candidate = Candidate::new(&source, "modal edits").unwrap();
        assert!(publish(&source, "old config", &candidate.path)
            .unwrap_err()
            .contains("config changed"));
        assert_eq!(fs::read_to_string(&source).unwrap(), "external edit");
        drop(candidate);
        assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 1);
    }

    #[tokio::test]
    async fn failed_validation_never_changes_source() {
        let dir = TestDir::new();
        let source = dir.0.join("config.yml");
        fs::write(&source, "original").unwrap();
        let candidate = Candidate::new(&source, "bad candidate").unwrap();
        assert!(validate(
            dir.0.join("missing-cloudflared").to_str().unwrap(),
            &candidate.path
        )
        .await
        .is_err());
        assert_eq!(fs::read_to_string(&source).unwrap(), "original");
        drop(candidate);
        assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 1);
    }

    #[test]
    fn readonly_source_and_symlinks_are_not_replaced() {
        let dir = TestDir::new();
        let source = dir.0.join("config.yml");
        fs::write(&source, "original").unwrap();
        let candidate = Candidate::new(&source, "updated").unwrap();
        let mut permissions = fs::metadata(&source).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&source, permissions).unwrap();
        assert!(publish(&source, "original", &candidate.path).is_err());
        assert_eq!(fs::read_to_string(&source).unwrap(), "original");
        #[cfg(unix)]
        {
            let link = dir.0.join("linked.yml");
            std::os::unix::fs::symlink(&source, &link).unwrap();
            assert!(publish(&link, "original", &candidate.path).is_err());
            assert!(fs::symlink_metadata(link).unwrap().file_type().is_symlink());
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn publish_preserves_group_and_acl() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let dir = TestDir::new();
        let source = dir.0.join("config.yml");
        fs::write(&source, "original").unwrap();
        let chgrp = std::process::Command::new("chgrp")
            .args(["everyone"])
            .arg(&source)
            .status()
            .unwrap();
        assert!(chgrp.success());
        fs::set_permissions(&source, fs::Permissions::from_mode(0o640)).unwrap();
        fs::File::open(&source)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(std::time::UNIX_EPOCH))
            .unwrap();
        let acl = std::process::Command::new("chmod")
            .args(["+a", "everyone allow read"])
            .arg(&source)
            .status()
            .unwrap();
        assert!(acl.success());
        let before = fs::metadata(&source).unwrap();
        let original_acl = std::process::Command::new("ls")
            .arg("-le")
            .arg(&source)
            .output()
            .unwrap();
        let candidate = Candidate::new(&source, "updated").unwrap();
        publish(&source, "original", &candidate.path).unwrap();
        assert_eq!(fs::read_to_string(&source).unwrap(), "updated");
        let after = fs::metadata(&source).unwrap();
        assert_eq!((after.uid(), after.gid()), (before.uid(), before.gid()));
        assert!(after.modified().unwrap() > before.modified().unwrap());
        assert_eq!(after.permissions().mode() & 0o777, 0o640);
        let updated_acl = std::process::Command::new("ls")
            .arg("-le")
            .arg(&source)
            .output()
            .unwrap();
        let acl_lines = |output: Vec<u8>| {
            String::from_utf8(output)
                .unwrap()
                .lines()
                .skip(1)
                .collect::<Vec<_>>()
                .join("\n")
        };
        assert_eq!(
            acl_lines(updated_acl.stdout),
            acl_lines(original_acl.stdout)
        );
    }
}
