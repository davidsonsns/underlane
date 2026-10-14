//! First-run setup never creates a tunnel or changes DNS.

use std::ffi::OsString;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::{Emitter, State};
use underlane_core::cli::{self, Cancellation, ConfigPolicy, Invocation, StdoutMode};
use underlane_core::cloudflared::Binary;
use underlane_core::config::home_dir;
use underlane_core::secret::redact_text;
use underlane_core::tunnel_setup::certificate_account;

use crate::state::Core;

pub(crate) const COMPLETE_KEY: &str = "onboarding_complete";

#[derive(Serialize)]
pub struct OnboardingStatus {
    pub complete: bool,
    /// Local certificate availability only; connect_cloudflare verifies current account access.
    pub account_connected: bool,
}

fn is_complete(db: &underlane_core::db::Db) -> Result<bool, String> {
    Ok(db
        .kv_get(COMPLETE_KEY)
        .map_err(|e| redact_text(&e.to_string()))?
        .as_deref()
        == Some("1")
        || !db
            .tunnels()
            .map_err(|e| redact_text(&e.to_string()))?
            .is_empty())
}

fn mark_complete(db: &underlane_core::db::Db) -> Result<(), String> {
    db.kv_set(COMPLETE_KEY, "1")
        .map_err(|e| redact_text(&e.to_string()))
}

#[tauri::command]
pub async fn onboarding_status(core: State<'_, Arc<Core>>) -> Result<OnboardingStatus, String> {
    let core = core.inner().clone();
    tokio::task::spawn_blocking(move || {
        Ok(OnboardingStatus {
            complete: is_complete(&core.db)?,
            account_connected: certificate_account(&home_dir().join(".cloudflared/cert.pem"))
                .is_ok(),
        })
    })
    .await
    .map_err(|e| redact_text(&e.to_string()))?
}

#[tauri::command]
pub async fn complete_onboarding(core: State<'_, Arc<Core>>) -> Result<(), String> {
    let core = core.inner().clone();
    tokio::task::spawn_blocking(move || mark_complete(&core.db))
        .await
        .map_err(|e| redact_text(&e.to_string()))?
}

#[tauri::command]
pub async fn select_cloudflared(
    core: State<'_, Arc<Core>>,
    path: Option<String>,
) -> Result<Option<Binary>, String> {
    let path = path
        .map(|path| path.trim().to_string())
        .filter(|path| !path.is_empty());
    if let Some(path) = &path {
        let expanded = underlane_core::config::expand_tilde(path);
        crate::installer::validate(&expanded).await?;
    }
    let save_core = core.inner().clone();
    tokio::task::spawn_blocking(move || {
        // Merge just this field while holding the settings lock; the UI's snapshot may be stale.
        let updated = {
            let mut settings = save_core.settings.write().unwrap();
            let mut updated = settings.clone();
            updated.cloudflared_path = path;
            save_core
                .db
                .save_settings(&updated)
                .map_err(|e| redact_text(&e.to_string()))?;
            *settings = updated.clone();
            updated
        };
        let _ = save_core.handle.emit("settings-changed", updated);
        Ok::<_, String>(())
    })
    .await
    .map_err(|e| redact_text(&e.to_string()))??;
    Ok(core.rediscover().await)
}

struct ActiveLogin {
    request: String,
    cancel: Cancellation,
    cancelled: Arc<AtomicBool>,
}

#[derive(Default)]
struct LoginRequests {
    active: Option<ActiveLogin>,
    // Covers cancellation delivered before its connect command starts, without an unbounded registry.
    last_cancelled: Option<String>,
}

static LOGIN: Mutex<LoginRequests> = Mutex::new(LoginRequests {
    active: None,
    last_cancelled: None,
});

struct LoginGuard(String);

impl Drop for LoginGuard {
    fn drop(&mut self) {
        let mut requests = LOGIN.lock().unwrap();
        if requests
            .active
            .as_ref()
            .is_some_and(|active| active.request == self.0)
        {
            requests.active = None;
        }
    }
}

fn valid_request(request: &str) -> bool {
    !request.is_empty()
        && request.len() <= 80
        && request
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn cancel_login(request: &str) {
    if !valid_request(request) {
        return;
    }
    let mut requests = LOGIN.lock().unwrap();
    if let Some(active) = requests
        .active
        .as_ref()
        .filter(|active| active.request == request)
    {
        active.cancelled.store(true, Ordering::SeqCst);
        active.cancel.cancel();
    }
    requests.last_cancelled = Some(request.into());
}

#[tauri::command]
pub fn cancel_onboarding_login(request_id: String) {
    cancel_login(&request_id);
}

async fn authorize(
    binary: &Path,
    data: &Path,
    home: &Path,
    cancel: Cancellation,
) -> Result<(), String> {
    let certificate = home.join(".cloudflared/cert.pem");
    match std::fs::symlink_metadata(&certificate) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let mut login = Invocation::new(
                [OsString::from("--origincert"), certificate.as_os_str().into(), "login".into()],
                ConfigPolicy::Neutral(data.into()),
            );
            login.stdout = StdoutMode::Secret;
            login.timeout = Duration::from_secs(300);
            login.cancel = cancel.clone();
            cli::exec(binary, login).await.map_err(|e| redact_text(&format!("Cloudflare login did not finish. {}", e)))?;
        }
        Err(_) => return Err("Could not inspect the existing Cloudflare certificate. Check its permissions and retry.".into()),
    }
    // Invalid existing certificates are preserved, and a zero exit without a certificate is failure.
    let _account = certificate_account(&certificate).map_err(|e| redact_text(&e.to_string()))?;
    let mut list = Invocation::new(
        [
            OsString::from("--origincert"),
            certificate.as_os_str().into(),
            "list".into(),
            "--output".into(),
            "json".into(),
        ],
        ConfigPolicy::Neutral(data.into()),
    );
    list.stdout = StdoutMode::Secret;
    list.cancel = cancel;
    let output = cli::exec(binary, list).await.map_err(|_| "Could not verify Cloudflare access. Check your connection and the account certificate, then retry.".to_string())?;
    // cloudflared encodes a nil tunnel slice as null for an account with no tunnels.
    let _: Option<Vec<serde_json::Value>> = output.stdout.deserialize().map_err(|_| "Cloudflare returned an unexpected tunnel list. Check the installed cloudflared version and retry.".to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn connect_cloudflare(
    core: State<'_, Arc<Core>>,
    request_id: String,
) -> Result<(), String> {
    if !valid_request(&request_id) {
        return Err("Invalid login request. Reopen onboarding and retry.".into());
    }
    let _account = crate::setup::ACCOUNT.try_lock().map_err(|_| {
        "Another Cloudflare setup or DNS operation is running. Finish it before connecting."
            .to_string()
    })?;
    let cancel = Cancellation::new();
    let cancelled = Arc::new(AtomicBool::new(false));
    {
        let mut requests = LOGIN.lock().unwrap();
        if requests.last_cancelled.as_deref() == Some(&request_id) {
            return Err("Cloudflare login was cancelled.".into());
        }
        if requests.active.is_some() {
            return Err("Another Cloudflare login is running. Finish or cancel it first.".into());
        }
        requests.active = Some(ActiveLogin {
            request: request_id.clone(),
            cancel: cancel.clone(),
            cancelled: cancelled.clone(),
        });
    }
    let _guard = LoginGuard(request_id);
    let binary = core
        .binary
        .read()
        .unwrap()
        .clone()
        .ok_or("Install cloudflared before connecting Cloudflare.")?;
    let result = tokio::time::timeout(
        Duration::from_secs(300),
        authorize(Path::new(&binary.path), &core.data_dir, &home_dir(), cancel),
    )
    .await
    .map_err(|_| {
        "Cloudflare login timed out after 5 minutes. Retry and finish authorization in the browser."
            .to_string()
    })?;
    if cancelled.load(Ordering::SeqCst) {
        return Err("Cloudflare login was cancelled.".into());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_install_stays_incomplete_until_explicit_completion() {
        let db = underlane_core::db::Db::open_in_memory().unwrap();
        assert!(!is_complete(&db).unwrap());
        assert!(
            db.kv_get(COMPLETE_KEY).unwrap().is_none(),
            "status must not write completion"
        );
        mark_complete(&db).unwrap();
        assert!(is_complete(&db).unwrap());
        assert_eq!(db.kv_get(COMPLETE_KEY).unwrap().as_deref(), Some("1"));
    }

    #[test]
    fn existing_tunnels_skip_onboarding_without_status_mutating_the_database() {
        let db = underlane_core::db::Db::open_in_memory().unwrap();
        db.upsert_tunnel(&underlane_core::model::Tunnel {
            id: "legacy-quick".into(),
            group_id: None,
            name: "Existing quick tunnel".into(),
            kind: underlane_core::model::TunnelKind::Quick,
            config_path: None,
            origin_url: Some("http://localhost:3000".into()),
            inspect: false,
            autostart: false,
            loglevel: "info".into(),
            created_at: 0,
            sort: 0,
        })
        .unwrap();
        assert!(is_complete(&db).unwrap());
        assert!(db.kv_get(COMPLETE_KEY).unwrap().is_none());
    }

    #[test]
    fn cancellation_is_scoped_and_remembers_an_early_cancel() {
        let cancelled = Arc::new(AtomicBool::new(false));
        LOGIN.lock().unwrap().active = Some(ActiveLogin {
            request: "current".into(),
            cancel: Cancellation::new(),
            cancelled: cancelled.clone(),
        });
        cancel_login("older");
        assert!(!cancelled.load(Ordering::SeqCst));
        cancel_login("current");
        assert!(cancelled.load(Ordering::SeqCst));
        LOGIN.lock().unwrap().active = None;
        cancel_login("not-started-yet");
        assert_eq!(
            LOGIN.lock().unwrap().last_cancelled.as_deref(),
            Some("not-started-yet")
        );
        LOGIN.lock().unwrap().last_cancelled = None;
    }

    #[cfg(unix)]
    struct Fixture(std::path::PathBuf);

    #[cfg(unix)]
    impl Fixture {
        fn new(mode: &str) -> Self {
            use base64::Engine;
            use std::os::unix::fs::PermissionsExt;
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = std::env::temp_dir().join(format!(
                "underlane-onboarding-{}-{nonce}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(root.join("home/.cloudflared")).unwrap();
            let cert = base64::engine::general_purpose::STANDARD.encode(r#"{"accountID":"synthetic-account","zoneID":"synthetic-zone","apiToken":"synthetic-secret"}"#);
            std::fs::write(
                root.join("certificate"),
                format!(
                    "-----BEGIN ARGO TUNNEL TOKEN-----\n{cert}\n-----END ARGO TUNNEL TOKEN-----\n"
                ),
            )
            .unwrap();
            std::fs::write(root.join("mode"), mode).unwrap();
            std::fs::write(root.join("cloudflared"), r#"#!/bin/sh
root=$(dirname "$0")
mode=$(cat "$root/mode")
if [ "$1" = '--version' ]; then
  if [ "$mode" = 'invalid-version' ]; then echo 'other version 2026.9.1'; else echo 'cloudflared version 2026.9.1'; fi
  exit 0
fi
printf '%s\n' "$@" >> "$root/args"
previous=''
for arg in "$@"; do
  if [ "$previous" = '--origincert' ]; then certificate=$arg; fi
  previous=$arg
done
case "$*" in
  *login*)
    touch "$root/login"
    if [ "$mode" = 'slow-login' ]; then touch "$root/started"; sleep 30; fi
    if [ "$mode" != 'missing-certificate' ]; then cp "$root/certificate" "$certificate"; fi
    ;;
  *list*)
    if [ "$mode" = 'rejected' ]; then echo 'unauthorized' >&2; exit 1; fi
    if [ "$mode" = 'invalid-list' ]; then echo '{}'; elif [ "$mode" = 'malformed-list' ]; then echo 'invalid json'; elif [ "$mode" = 'null-list' ]; then echo 'null'; else echo '[]'; fi
    ;;
esac
"#).unwrap();
            std::fs::set_permissions(
                root.join("cloudflared"),
                std::fs::Permissions::from_mode(0o755),
            )
            .unwrap();
            Self(root)
        }

        fn certificate(&self) -> std::path::PathBuf {
            self.0.join("home/.cloudflared/cert.pem")
        }

        async fn authorize(&self, cancel: Cancellation) -> Result<(), String> {
            authorize(
                &self.0.join("cloudflared"),
                &self.0.join("data"),
                &self.0.join("home"),
                cancel,
            )
            .await
        }
    }

    #[cfg(unix)]
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn login_requires_certificate_and_live_verification_without_provisioning() {
        let fixture = Fixture::new("success");
        fixture.authorize(Cancellation::new()).await.unwrap();
        let args = std::fs::read_to_string(fixture.0.join("args")).unwrap();
        assert!(args.contains("login\n") && args.contains("list\n"));
        assert!(args.contains("--config\n") && args.contains("neutral.yml\n"));
        assert!(!args.contains("create\n") && !args.contains("route\n"));
        for mode in [
            "missing-certificate",
            "rejected",
            "invalid-list",
            "malformed-list",
        ] {
            assert!(
                Fixture::new(mode)
                    .authorize(Cancellation::new())
                    .await
                    .is_err(),
                "{mode} must fail"
            );
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_new_account_with_no_tunnels_can_finish_login() {
        Fixture::new("null-list")
            .authorize(Cancellation::new())
            .await
            .unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn existing_certificates_are_preserved_and_never_trigger_login() {
        let fixture = Fixture::new("success");
        std::fs::copy(fixture.0.join("certificate"), fixture.certificate()).unwrap();
        let original = std::fs::read(fixture.certificate()).unwrap();
        fixture.authorize(Cancellation::new()).await.unwrap();
        assert!(!fixture.0.join("login").exists());
        assert_eq!(std::fs::read(fixture.certificate()).unwrap(), original);
        std::fs::write(fixture.certificate(), "invalid certificate").unwrap();
        assert!(fixture.authorize(Cancellation::new()).await.is_err());
        assert!(!fixture.0.join("login").exists());
        assert_eq!(
            std::fs::read_to_string(fixture.certificate()).unwrap(),
            "invalid certificate"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn cancelled_login_does_not_report_connection_or_leave_a_running_child() {
        let fixture = Fixture::new("slow-login");
        let cancel = Cancellation::new();
        let pending = fixture.authorize(cancel.clone());
        let stop = async {
            tokio::time::timeout(Duration::from_secs(3), async {
                while !fixture.0.join("started").exists() {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            cancel.cancel();
        };
        let (result, ()) = tokio::join!(pending, stop);
        assert!(result.is_err());
        assert!(!fixture.certificate().exists());
        assert!(!std::fs::read_to_string(fixture.0.join("args"))
            .unwrap()
            .contains("list\n"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn executable_selection_rejects_an_unrelated_version_command() {
        let fixture = Fixture::new("invalid-version");
        assert!(crate::installer::validate(&fixture.0.join("cloudflared"))
            .await
            .is_err());
    }
}
