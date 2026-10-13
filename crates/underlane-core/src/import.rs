//! Importing config files, token tunnels and quick tunnels into the database.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::config::{home_dir, scan_dir, TunnelConfig};
use crate::db::Db;
use crate::model::{Tunnel, TunnelKind};
use crate::proxy::now_ms;

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct ImportReport {
    pub added: Vec<String>,
    /// Files that were already imported.
    pub existing: Vec<String>,
    /// path -> reason
    pub failed: Vec<(String, String)>,
}

/// Group for a config file: the folder that holds it (skipping `.cloudflared`), or
/// "Personal" for files directly in `~/.cloudflared` or the home folder.
pub fn group_name_for(path: &Path) -> String {
    let mut dir = path.parent();
    if dir.and_then(|d| d.file_name()).and_then(|n| n.to_str()) == Some(".cloudflared") {
        dir = dir.and_then(Path::parent);
    }
    match dir {
        Some(d) if d == home_dir() || d.parent().is_none() => "Personal".into(),
        Some(d) => d.file_name().and_then(|n| n.to_str()).unwrap_or("Personal").to_string(),
        None => "Personal".into(),
    }
}

/// Expand a mix of files and folders into candidate config files.
pub fn expand(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for p in paths {
        if p.is_dir() {
            out.extend(scan_dir(p));
        } else {
            out.push(p.clone());
        }
    }
    out.sort();
    out.dedup();
    out
}

fn next_sort(db: &Db) -> i64 {
    db.tunnels().map(|t| t.iter().map(|t| t.sort).max().unwrap_or(-1) + 1).unwrap_or(0)
}

/// Import config files (and folders of them). Already-imported paths are skipped.
pub fn import_configs(db: &Db, paths: &[PathBuf], default_loglevel: &str) -> ImportReport {
    let mut report = ImportReport::default();
    for path in expand(paths) {
        let key = path.to_string_lossy().into_owned();
        let config = match TunnelConfig::load(&path) {
            Ok(c) => c,
            Err(e) => {
                report.failed.push((key, e.to_string()));
                continue;
            }
        };
        if config.ingress.is_empty() && config.tunnel.is_none() {
            report.failed.push((key, "This file doesn't look like a cloudflared config (no `tunnel:` or `ingress:`).".into()));
            continue;
        }
        match db.tunnel_by_config_path(&key) {
            Ok(Some(_)) => {
                report.existing.push(key);
                continue;
            }
            Ok(None) => {}
            Err(e) => {
                report.failed.push((key, e.to_string()));
                continue;
            }
        }
        let result = db.ensure_group(&group_name_for(&path)).and_then(|group| {
            let t = Tunnel {
                id: uuid::Uuid::new_v4().to_string(),
                group_id: Some(group.id),
                name: config.display_name(),
                kind: TunnelKind::Config,
                config_path: Some(key.clone()),
                origin_url: None,
                inspect: false,
                autostart: false,
                loglevel: default_loglevel.to_string(),
                created_at: now_ms(),
                sort: next_sort(db),
            };
            db.upsert_tunnel(&t)?;
            Ok(t)
        });
        match result {
            Ok(_) => report.added.push(key),
            Err(e) => report.failed.push((key, e.to_string())),
        }
    }
    report
}

/// Create a quick or token tunnel.
pub fn add_simple(db: &Db, kind: TunnelKind, name: &str, group: &str, origin_url: Option<String>, default_loglevel: &str) -> anyhow::Result<Tunnel> {
    let group = db.ensure_group(group)?;
    let t = Tunnel {
        id: uuid::Uuid::new_v4().to_string(),
        group_id: Some(group.id),
        name: name.trim().to_string(),
        kind,
        config_path: None,
        origin_url,
        inspect: false,
        autostart: false,
        loglevel: default_loglevel.to_string(),
        created_at: now_ms(),
        sort: next_sort(db),
    };
    db.upsert_tunnel(&t)?;
    Ok(t)
}
