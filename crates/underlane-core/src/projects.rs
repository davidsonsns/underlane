//! Register existing configs or create stopped, app-owned project drafts.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::config::{expand_tilde, ConfigError, TunnelConfig};
use crate::db::{normalize_name, Db};
use crate::model::{Tunnel, TunnelKind};

const DRAFT: &str = "# Managed by Underlane\n# Use Set up tunnel in the project to create a tunnel and link its credentials.\n# Add hostname/service rules before the final fallback rule.\ningress:\n  - service: http_status:404\n";
pub const DRAFT_DETAIL: &str = "This project needs a Cloudflare tunnel ID before it can start.";
pub const DRAFT_HINT: &str = "Use Set up tunnel in the project to log in, create a tunnel, and link its credentials, then configure hostnames.";

pub fn needs_setup(config: &TunnelConfig) -> bool {
    let has_identity = config.tunnel.as_deref().is_some_and(|identity| !identity.trim().is_empty())
        || config.raw.as_mapping().is_some_and(|map| ["token", "token-file"].iter()
            .any(|field| map.get(*field).and_then(serde_norway::Value::as_str).is_some_and(|value| !value.trim().is_empty())));
    !config.is_single_origin() && !has_identity
}

#[derive(Debug, Serialize, PartialEq)]
pub struct ProjectRoute {
    pub hostname: Option<String>,
    pub path: Option<String>,
    pub service: String,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct ProjectConfigPreview {
    pub path: String,
    pub routes: Vec<ProjectRoute>,
}

/// Merely calculate the location; opening the New Project modal never creates directories.
pub fn default_directory() -> PathBuf {
    crate::config::home_dir().join(".cloudflared/underlane")
}

fn load(path: &Path) -> anyhow::Result<TunnelConfig> {
    // Keep the selected symlink's parent: relative credentials use the selected config location.
    let path = std::path::absolute(expand_tilde(&path.to_string_lossy()))
        .map_err(|e| anyhow::anyhow!("Could not locate the config file: {e}"))?;
    let metadata = fs::metadata(&path).map_err(|e| anyhow::anyhow!("Could not locate the config file: {e}"))?;
    anyhow::ensure!(metadata.is_file(), "The config path must be a regular file.");
    anyhow::ensure!(path.extension().and_then(|s| s.to_str()).is_some_and(|s| s.eq_ignore_ascii_case("yml") || s.eq_ignore_ascii_case("yaml")),
        "Choose a .yml or .yaml config file.");
    let config = TunnelConfig::load(&path).map_err(|e| match e {
        // YAML errors can quote secret source values. Retain only their location.
        ConfigError::Yaml { source, .. } => match source.location() {
            Some(location) => anyhow::anyhow!("The config is not valid YAML at line {}, column {}.", location.line(), location.column()),
            None => anyhow::anyhow!("The config is not valid YAML."),
        },
        ConfigError::Io { source, .. } => anyhow::anyhow!("Could not read the config file: {source}"),
        ConfigError::Invalid(message) => anyhow::anyhow!(crate::secret::redact_text(&message)),
    })?;
    let map = config.raw.as_mapping().expect("parsed config mapping");
    for field in ["tunnel", "credentials-file", "url"] {
        anyhow::ensure!(map.get(field).is_none_or(|value| value.is_null() || value.as_str().is_some()),
            "The config `{field}` field must be a string.");
    }
    if let Some(rules) = map.get("ingress").and_then(serde_norway::Value::as_sequence) {
        for (index, rule) in rules.iter().enumerate() {
            let rule = rule.as_mapping().expect("parsed ingress mapping");
            for field in ["hostname", "path", "service"] {
                anyhow::ensure!(rule.get(field).is_none_or(|value| value.is_null() || value.as_str().is_some()),
                    "Ingress rule #{} `{field}` must be a string.", index + 1);
            }
        }
    }
    anyhow::ensure!(!config.ingress.is_empty() || config.tunnel.as_deref().is_some_and(|s| !s.trim().is_empty()),
        "This file does not look like a cloudflared config: add tunnel, ingress, or url.");
    for rule in &config.ingress {
        anyhow::ensure!(!rule.service.trim().is_empty(), "Ingress rule #{} has an empty service.", rule.index + 1);
    }
    Ok(config)
}

pub fn preview(path: &Path) -> anyhow::Result<ProjectConfigPreview> {
    let config = load(path)?;
    Ok(ProjectConfigPreview {
        path: config.source.to_string_lossy().into_owned(),
        routes: config.ingress.into_iter().map(|rule| ProjectRoute {
            hostname: rule.hostname.map(|s| crate::secret::redact_text(&s)),
            path: rule.path.map(|s| crate::secret::redact_text(&s)),
            service: crate::secret::redact_text(&rule.service),
        }).collect(),
    })
}

/// Discover valid, unregistered configs, without following directory symlinks.
pub fn available_configs(db: &Db, directory: &Path) -> anyhow::Result<Vec<TunnelConfig>> {
    let used: Vec<PathBuf> = db.tunnels()?.into_iter().filter_map(|t| t.config_path)
        .map(|path| expand_tilde(&path))
        .map(|path| fs::canonicalize(&path).unwrap_or(path)).collect();
    let directories_root = directory.to_path_buf();
    let mut directories = vec![directories_root.clone()];
    let mut configs = Vec::new();
    let mut seen = std::collections::HashSet::new();
    while let Some(directory) = directories.pop() {
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) if directory != directories_root => continue,
            Err(e) => return Err(e.into()),
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                directories.push(path);
                continue;
            }
            if let Ok(config) = load(&path) {
                let Ok(canonical) = fs::canonicalize(&path) else { continue };
                if !used.contains(&canonical) && seen.insert(canonical) {
                    configs.push(config);
                }
            }
        }
    }
    configs.sort_by(|a, b| a.source.cmp(&b.source));
    Ok(configs)
}

fn filename(name: &str) -> String {
    let mut safe = String::new();
    for character in name.chars().flat_map(char::to_lowercase) {
        if character.is_alphanumeric() {
            if safe.len() + character.len_utf8() > 120 { break; }
            safe.push(character);
        } else if !safe.is_empty() && !safe.ends_with('-') && safe.len() < 120 {
            safe.push('-');
        }
    }
    let safe = safe.trim_end_matches('-');
    let safe = if safe.is_empty() { "project" } else { safe };
    let reserved = matches!(safe, "con" | "prn" | "aux" | "nul")
        || (safe.len() == 4 && (safe.starts_with("com") || safe.starts_with("lpt")) && matches!(safe.as_bytes()[3], b'1'..=b'9'));
    format!("{}{safe}.yml", if reserved { "project-" } else { "" })
}

fn publish_draft(directory: &Path, name: &str) -> anyhow::Result<(PathBuf, File)> {
    let path = directory.join(filename(name));
    let temporary = directory.join(format!(".underlane-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)] {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(DRAFT.as_bytes())?;
        file.sync_all()?;
        // A hard link publishes complete bytes atomically and never replaces an existing path.
        fs::hard_link(&temporary, &path)?;
        Ok((path, file))
    })();
    let _ = fs::remove_file(&temporary);
    result.map_err(|e: std::io::Error| if e.kind() == std::io::ErrorKind::AlreadyExists {
        anyhow::anyhow!("A config with this project filename already exists. Choose another name or load that file.")
    } else { anyhow::anyhow!("Could not create the project draft: {e}") })
}

pub fn create(db: &Db, name: &str, group: &str, config_path: Option<&Path>, directory: Option<&Path>, loglevel: &str) -> anyhow::Result<Tunnel> {
    let name = normalize_name(name)?;
    let group = if group.trim().is_empty() { None } else { Some(normalize_name(group)?) };
    let (path, created) = if let Some(path) = config_path {
        (load(path)?.source, None)
    } else {
        let directory = match directory {
            Some(path) => {
                let path = fs::canonicalize(expand_tilde(&path.to_string_lossy()))?;
                anyhow::ensure!(path.is_dir(), "Choose an existing directory for the project draft.");
                path
            }
            None => {
                let path = default_directory();
                let mut builder = fs::DirBuilder::new();
                builder.recursive(true);
                #[cfg(unix)] {
                    use std::os::unix::fs::DirBuilderExt;
                    builder.mode(0o700);
                }
                builder.create(&path)?;
                fs::canonicalize(path)?
            }
        };
        let (path, file) = publish_draft(&directory, &name)?;
        (path, Some(file))
    };
    let tunnel = Tunnel {
        id: uuid::Uuid::new_v4().to_string(), group_id: None, name, kind: TunnelKind::Config,
        config_path: Some(path.to_string_lossy().into_owned()), origin_url: None,
        inspect: false, autostart: false, loglevel: loglevel.into(),
        created_at: crate::proxy::now_ms(), sort: 0,
    };
    let result = db.register_project(tunnel, group.as_deref());
    if result.is_err() {
        if let Some(file) = created {
            // Do not remove a replacement or an externally edited draft on database failure.
            let unchanged = fs::read(&path).ok().as_deref() == Some(DRAFT.as_bytes());
            #[cfg(unix)]
            let unchanged = {
                use std::os::unix::fs::MetadataExt;
                unchanged && file.metadata().ok().zip(fs::symlink_metadata(&path).ok())
                    .is_some_and(|(original, current)| original.dev() == current.dev() && original.ino() == current.ino())
            };
            if unchanged { let _ = fs::remove_file(&path); }
        }
    }
    result
}
