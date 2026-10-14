//! Stop tunnels and clear local state without exiting the application.

use std::path::Path;
use std::sync::Arc;

use tauri::{Emitter, State};
use underlane_core::{db::Db, model::TunnelKind};

use crate::{platform, state::Core, tokens};

#[tauri::command]
pub async fn reset_app(core: State<'_, Arc<Core>>) -> Result<(), String> {
    reset(core.inner().clone()).await
}

pub async fn reset(core: Arc<Core>) -> Result<(), String> {
    let _account = crate::setup::ACCOUNT.try_lock().map_err(|_| "Finish the current Cloudflare operation before resetting Underlane.")?;
    let _install = crate::installer::INSTALL_LOCK.try_lock().map_err(|_| "Finish installing cloudflared before resetting Underlane.")?;
    let _saves = crate::hostnames::SAVES.try_lock().map_err(|_| "Finish saving hostnames before resetting Underlane.")?;
    let _operations = core.operations.write().await;
    core.sup.begin_reset();
    core.sup.stop_all_for_reset().await;
    let reset_core = core.clone();
    let result = tokio::task::spawn_blocking(move || {
        if platform::login_item().enabled {
            let result = platform::set_login_item(false);
            anyhow::ensure!(!result.enabled, "{}", result.message.unwrap_or_else(|| "Could not disable Launch at login.".into()));
        }
        let mut epoch = reset_core.data_epoch.write().unwrap();
        clear_local_data(&reset_core.db, &reset_core.data_dir, tokens::delete_for_reset)?;
        *epoch = epoch.wrapping_add(1);
        reset_core.reset_runtime()?;
        Ok::<_, anyhow::Error>(())
    }).await.map_err(|error| error.to_string()).and_then(|result| result.map_err(|error| underlane_core::secret::redact_text(&error.to_string())));
    core.sup.end_reset();
    result?;
    core.rediscover().await;
    drop(_operations);
    core.notify_changed();
    core.handle.emit("app-reset", ()).map_err(|error| error.to_string())?;
    Ok(())
}

fn clear_local_data(db: &Db, directory: &Path, delete_token: impl Fn(&str) -> anyhow::Result<()>) -> anyhow::Result<()> {
    for tunnel in db.tunnels()?.into_iter().filter(|t| t.kind == TunnelKind::Token) {
        delete_token(&tunnel.id)?;
    }
    // Shared project configs and cloudflared authorization can be used by other tools.
    for name in ["bin", "derived", "run", "project-setup", "separate-tunnel", "separate-cert", "neutral.yml", "reset-pending"] {
        let path = directory.join(name);
        let result = match std::fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_dir() => std::fs::remove_dir_all(&path),
            Ok(_) => std::fs::remove_file(&path),
            Err(error) => Err(error),
        };
        if let Err(error) = result {
            if error.kind() != std::io::ErrorKind::NotFound { return Err(error.into()); }
        }
    }
    db.reset()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_clears_live_database_and_is_retryable_without_touching_external_configs() {
        let directory = std::env::temp_dir().join(format!(
            "underlane-reset-{}-{}",
            std::process::id(),
            underlane_core::proxy::now_ms()
        ));
        std::fs::create_dir_all(directory.join("derived")).unwrap();
        let config = directory.join("external.yml");
        std::fs::write(&config, "keep this config").unwrap();
        let db = Db::open(&directory.join("underlane.db")).unwrap();
        db.kv_set("onboarding_complete", "1").unwrap();
        db.save_settings(&underlane_core::model::Settings {
            theme: "dark".into(),
            ..Default::default()
        })
        .unwrap();
        db.ensure_group("Local project").unwrap();
        db.upsert_tunnel(&underlane_core::model::Tunnel {
            id: "token".into(),
            group_id: None,
            name: "Token tunnel".into(),
            kind: TunnelKind::Token,
            config_path: None,
            origin_url: None,
            inspect: false,
            autostart: false,
            loglevel: "info".into(),
            created_at: 0,
            sort: 0,
        })
        .unwrap();
        db.insert_logs(&[underlane_core::model::LogLine {
            id: 0,
            tunnel_id: "token".into(),
            ts: 1,
            level: "info".into(),
            message: "Saved history".into(),
            fields: "{}".into(),
        }])
        .unwrap();
        db.upsert_request(&underlane_core::proxy::capture::CapturedExchange {
            id: "request".into(),
            tunnel_id: "token".into(),
            rule_index: 0,
            started_at_ms: 1,
            duration_ms: Some(1),
            kind: underlane_core::proxy::capture::ExchangeKind::Http,
            method: "GET".into(),
            host: "localhost".into(),
            path: "/".into(),
            status: Some(200),
            req_headers: vec![],
            resp_headers: vec![],
            req_body: Default::default(),
            resp_body: Default::default(),
            error: None,
        })
        .unwrap();
        assert!(clear_local_data(&db, &directory, |_| anyhow::bail!("Keychain locked")).is_err());
        assert!(directory.join("underlane.db").exists());
        clear_local_data(&db, &directory, |id| {
            assert_eq!(id, "token");
            Ok(())
        })
        .unwrap();
        assert!(!directory.join("derived").exists());
        let fresh = &db;
        assert!(fresh.tunnels().unwrap().is_empty());
        assert!(fresh.groups().unwrap().is_empty());
        assert!(fresh.kv_get("onboarding_complete").unwrap().is_none());
        assert_eq!(
            fresh.settings().unwrap(),
            underlane_core::model::Settings::default()
        );
        assert!(fresh.query_logs(&Default::default()).unwrap().is_empty());
        assert!(fresh.get_request("request").unwrap().is_none());
        assert_eq!(std::fs::read_to_string(config).unwrap(), "keep this config");
        drop(db);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
