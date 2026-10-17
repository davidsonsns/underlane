//! Preserve the running session for the next launch after installing an app update.

use std::collections::HashSet;

use underlane_core::db::Db;

const SESSION: &str = "update_restart_tunnels";

pub fn save(db: &Db, is_running: impl Fn(&str) -> bool) -> anyhow::Result<()> {
    let ids: Vec<_> = db.tunnels()?.into_iter().filter(|t| is_running(&t.id)).map(|t| t.id).collect();
    db.kv_set(SESSION, &serde_json::to_string(&ids)?)
}

/// An empty saved session still overrides autostart. Missing tunnels are ignored.
pub fn startup(db: &Db) -> anyhow::Result<(bool, Vec<String>)> {
    let session = db.kv_take(SESSION)?;
    let restarting = session.is_some();
    let saved: Option<HashSet<String>> = session.map(|json| serde_json::from_str(&json)).transpose()?;
    let ids = db.tunnels()?.into_iter()
        .filter(|t| saved.as_ref().map_or(t.autostart, |ids| ids.contains(&t.id)))
        .map(|t| t.id).collect();
    Ok((restarting, ids))
}

#[cfg(test)]
mod tests {
    use super::*;
    use underlane_core::model::{Tunnel, TunnelKind};

    fn tunnel(db: &Db, id: &str, autostart: bool) {
        db.upsert_tunnel(&Tunnel {
            id: id.into(), group_id: None, name: id.into(), kind: TunnelKind::Quick,
            config_path: None, origin_url: Some("http://localhost:3000".into()),
            inspect: false, autostart, loglevel: "info".into(), created_at: 0, sort: 0,
        }).unwrap();
    }

    #[test]
    fn update_restart_restores_running_session_from_disk_once_without_changing_autostart() {
        let directory = std::env::temp_dir().join(format!("underlane-restart-{}-{}", std::process::id(), underlane_core::proxy::now_ms()));
        let path = directory.join("underlane.db");
        let db = Db::open(&path).unwrap();
        tunnel(&db, "manual", false);
        tunnel(&db, "stopped", true);
        tunnel(&db, "backoff", false);
        save(&db, |id| matches!(id, "manual" | "backoff")).unwrap();
        drop(db);

        let db = Db::open(&path).unwrap();
        let (restarting, mut ids) = startup(&db).unwrap();
        ids.sort();
        assert!(restarting);
        assert_eq!(ids, ["backoff", "manual"]);
        assert!(!db.tunnel("manual").unwrap().unwrap().autostart);
        assert!(db.tunnel("stopped").unwrap().unwrap().autostart);
        assert_eq!(startup(&db).unwrap(), (false, vec!["stopped".into()]));
        drop(db);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn empty_session_overrides_autostart_and_latest_snapshot_replaces_previous_one() {
        let db = Db::open_in_memory().unwrap();
        tunnel(&db, "auto", true);
        save(&db, |_| true).unwrap();
        save(&db, |_| false).unwrap();
        assert_eq!(startup(&db).unwrap(), (true, vec![]));
        assert_eq!(startup(&db).unwrap(), (false, vec!["auto".into()]));
    }

    #[test]
    fn deleted_unknown_and_duplicate_ids_do_not_start_other_tunnels() {
        let db = Db::open_in_memory().unwrap();
        tunnel(&db, "running", false);
        tunnel(&db, "deleted", false);
        tunnel(&db, "stopped", true);
        db.kv_set(SESSION, r#"["running","deleted","missing","../invalid","running"]"#).unwrap();
        db.delete_tunnel("deleted").unwrap();
        assert_eq!(startup(&db).unwrap(), (true, vec!["running".into()]));
    }

    #[test]
    fn malformed_session_fails_without_falling_back_to_autostart_and_is_consumed() {
        let db = Db::open_in_memory().unwrap();
        tunnel(&db, "auto", true);
        db.kv_set(SESSION, r#"[42]"#).unwrap();
        assert!(startup(&db).is_err());
        assert_eq!(startup(&db).unwrap(), (false, vec!["auto".into()]));
    }
}
