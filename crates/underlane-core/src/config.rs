//! Reading cloudflared `config.yml` files.
//!
//! Files are only ever read. The parsed document keeps the original YAML value so a derived
//! config can be emitted later without losing keys this module doesn't model.

use std::path::{Path, PathBuf};
use std::hash::{Hash, Hasher};

use serde::Serialize;
use serde_norway::{Mapping, Value};

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("could not read {path}: {source}")]
    Io { path: PathBuf, source: std::io::Error },
    #[error("{path} is not valid YAML: {source}")]
    Yaml { path: PathBuf, source: serde_norway::Error },
    #[error("{0}")]
    Invalid(String),
}

/// One `ingress:` entry.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct IngressRule {
    pub index: usize,
    pub hostname: Option<String>,
    pub path: Option<String>,
    pub service: String,
}

/// A parsed config file.
#[derive(Debug, Clone)]
pub struct TunnelConfig {
    pub source: PathBuf,
    /// The `tunnel:` key (name or UUID), if present.
    pub tunnel: Option<String>,
    pub credentials_file: Option<PathBuf>,
    pub ingress: Vec<IngressRule>,
    /// The full original document.
    pub raw: Value,
    pub fingerprint: ConfigFingerprint,
}

/// Ephemeral source-byte identity; never retains YAML or reaches IPC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfigFingerprint(u64);

impl ConfigFingerprint {
    pub fn of(bytes: &[u8]) -> Self {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        bytes.hash(&mut hash);
        Self(hash.finish())
    }
}

impl TunnelConfig {
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::parse(path, &text)
    }

    pub fn parse(path: &Path, text: &str) -> Result<Self, ConfigError> {
        let raw: Value = if text.trim().is_empty() {
            Value::Mapping(Mapping::new())
        } else {
            serde_norway::from_str(text).map_err(|source| ConfigError::Yaml {
                path: path.to_path_buf(),
                source,
            })?
        };
        let map = raw
            .as_mapping()
            .ok_or_else(|| ConfigError::Invalid(format!("{} must be a YAML mapping at the top level", path.display())))?;

        let tunnel = map.get("tunnel").and_then(scalar_string);
        let base_dir = path.parent().unwrap_or(Path::new("."));
        let credentials_file = map
            .get("credentials-file")
            .and_then(Value::as_str)
            .map(|p| resolve_path(base_dir, p));

        let mut ingress = Vec::new();
        match map.get("ingress") {
            Some(Value::Sequence(rules)) => {
                for (index, rule) in rules.iter().enumerate() {
                    let rule = rule.as_mapping().ok_or_else(|| {
                        ConfigError::Invalid(format!("ingress rule #{} must be a mapping", index + 1))
                    })?;
                    let service = rule.get("service").and_then(scalar_string).ok_or_else(|| {
                        ConfigError::Invalid(format!("ingress rule #{} has no service", index + 1))
                    })?;
                    ingress.push(IngressRule {
                        index,
                        hostname: rule.get("hostname").and_then(scalar_string).filter(|h| !h.is_empty()),
                        path: rule.get("path").and_then(scalar_string).filter(|p| !p.is_empty()),
                        service,
                    });
                }
            }
            Some(Value::Null) | None => {
                // Single-origin configs use a top-level `url:` (or `hello-world: true`).
                if let Some(url) = map.get("url").and_then(scalar_string) {
                    ingress.push(IngressRule { index: 0, hostname: None, path: None, service: url });
                } else if map.get("hello-world").and_then(Value::as_bool) == Some(true) {
                    ingress.push(IngressRule { index: 0, hostname: None, path: None, service: "hello_world".into() });
                }
            }
            Some(_) => return Err(ConfigError::Invalid("`ingress` must be a list of rules".into())),
        }

        Ok(TunnelConfig {
            source: path.to_path_buf(),
            tunnel,
            credentials_file,
            ingress,
            raw,
            fingerprint: ConfigFingerprint::of(text.as_bytes()),
        })
    }

    /// True when the file uses a top-level `url:` instead of ingress rules.
    pub fn is_single_origin(&self) -> bool {
        self.raw
            .as_mapping()
            .map(|m| !m.contains_key("ingress") && (m.contains_key("url") || m.get("hello-world").and_then(Value::as_bool) == Some(true)))
            .unwrap_or(false)
    }

    /// A display name: the file stem, unless it is the generic `config`, in which case the
    /// parent directory name (projects often keep `.cloudflared/config.yml`).
    pub fn display_name(&self) -> String {
        let stem = self.source.file_stem().and_then(|s| s.to_str()).unwrap_or("tunnel");
        if matches!(stem, "config" | "cloudflared") {
            let parent = self.source.parent().and_then(|p| p.file_name()).and_then(|s| s.to_str());
            match parent {
                Some(".cloudflared") => self
                    .source
                    .parent()
                    .and_then(|p| p.parent())
                    .and_then(|p| p.file_name())
                    .and_then(|s| s.to_str())
                    .unwrap_or(stem)
                    .to_string(),
                Some(p) => p.to_string(),
                None => stem.to_string(),
            }
        } else {
            stem.to_string()
        }
    }
}

fn scalar_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// Resolve `~/...` and relative paths the way a user would expect from a config file.
pub fn resolve_path(base_dir: &Path, p: &str) -> PathBuf {
    let expanded = expand_tilde(p);
    if expanded.is_absolute() {
        expanded
    } else {
        base_dir.join(expanded)
    }
}

pub fn expand_tilde(p: &str) -> PathBuf {
    if p == "~" {
        return home_dir();
    }
    if let Some(rest) = p.strip_prefix("~/") {
        return home_dir().join(rest);
    }
    PathBuf::from(p)
}

pub use crate::platform::home_dir;

/// Publish a complete app-owned config atomically, including when commands start together.
pub(crate) fn neutral_config(data_dir: &Path) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(data_dir)?;
    let target = data_dir.join("neutral.yml");
    let temporary = data_dir.join(format!(".neutral-{}.yml", uuid::Uuid::new_v4()));
    let result = (|| {
        std::fs::write(&temporary, "# Generated by Underlane. Keeps cloudflared from loading a default config.\nno-autoupdate: true\n")?;
        std::fs::rename(&temporary, &target)?;
        Ok(target)
    })();
    if result.is_err() { let _ = std::fs::remove_file(&temporary); }
    result
}

/// Find candidate config files in a directory (non-recursive, plus one level of `.cloudflared`).
pub fn scan_dir(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut visit = |d: &Path| {
        if let Ok(entries) = std::fs::read_dir(d) {
            for e in entries.flatten() {
                let p = e.path();
                let is_yaml = p
                    .extension()
                    .and_then(|x| x.to_str())
                    .map(|x| x.eq_ignore_ascii_case("yml") || x.eq_ignore_ascii_case("yaml"))
                    .unwrap_or(false);
                if p.is_file() && is_yaml && looks_like_tunnel_config(&p) {
                    found.push(p);
                }
            }
        }
    };
    visit(dir);
    let nested = dir.join(".cloudflared");
    if nested.is_dir() {
        visit(&nested);
    }
    found.sort();
    found
}

/// Cheap check so a folder scan doesn't import random YAML (CI files, compose files...).
pub fn looks_like_tunnel_config(path: &Path) -> bool {
    match TunnelConfig::load(path) {
        Ok(c) => {
            let m = c.raw.as_mapping();
            c.tunnel.is_some() || m.map(|m| m.contains_key("ingress") || m.contains_key("credentials-file")).unwrap_or(false)
        }
        Err(_) => false,
    }
}
