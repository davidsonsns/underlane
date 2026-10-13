//! Editable hostname routes, preserving the rest of the source document.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_norway::{Mapping, Value};

use crate::config::{ConfigError, TunnelConfig};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostnameRoute {
    pub index: Option<usize>,
    pub hostname: String,
    pub service: String,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostnameConfig {
    pub revision: String,
    pub routes: Vec<HostnameRoute>,
    pub fallback_service: String,
}

#[derive(Debug)]
pub struct PreparedHostnames {
    pub config: TunnelConfig,
    pub text: String,
    pub original: String,
    pub revision: String,
    pub changed: bool,
}

/// Read an editor snapshot. Path-only rules remain hidden and are preserved on save.
pub fn load(path: &Path) -> Result<HostnameConfig, ConfigError> {
    let (config, text) = read(path)?;
    snapshot(&config, revision(path, &text))
}

/// Produce a candidate without writing anything. The backend validates and publishes it.
pub fn prepare(
    path: &Path,
    expected_revision: &str,
    routes: &[HostnameRoute],
    fallback_service: &str,
) -> Result<PreparedHostnames, ConfigError> {
    let (mut config, original) = read(path)?;
    let revision = revision(path, &original);
    if revision != expected_revision {
        return Err(invalid(
            "The config changed while this editor was open. Reopen Manage hostnames and try again.",
        ));
    }
    let current = snapshot(&config, revision.clone())?;
    let originals: HashMap<_, _> = current
        .routes
        .iter()
        .map(|route| (route.index.unwrap(), route))
        .collect();
    let mut edits = HashMap::new();
    let mut additions = Vec::new();
    let mut previous = None;
    let mut identities: HashMap<(String, Option<String>), bool> = HashMap::new();
    for route in routes {
        // Existing spelling keeps no-op bytes and cloudflared's literal matching intact.
        let hostname = match route.index.and_then(|index| originals.get(&index)) {
            Some(old) if route.hostname == old.hostname => old.hostname.clone(),
            _ => route.hostname.to_ascii_lowercase(),
        };
        validate_hostname(&hostname)?;
        let service = match route.index.and_then(|index| originals.get(&index)) {
            Some(old) if route.service == old.service => old.service.clone(),
            _ => normalize_service(&route.service)?,
        };
        let modified = if let Some(index) = route.index {
            let old = originals.get(&index).ok_or_else(|| {
                invalid("A hostname rule no longer exists. Reopen Manage hostnames.")
            })?;
            if edits.contains_key(&index) {
                return Err(invalid(
                    "A hostname rule was submitted twice. Reopen Manage hostnames.",
                ));
            }
            if previous.is_some_and(|previous| index <= previous) {
                return Err(invalid(
                    "Existing hostname rules cannot be reordered in this editor.",
                ));
            }
            previous = Some(index);
            if route.path != old.path {
                return Err(invalid(
                    "Path filters cannot be changed in this editor. Edit the config file instead.",
                ));
            }
            edits.insert(index, (hostname.clone(), service.clone()));
            hostname != old.hostname || service != old.service
        } else {
            if route.path.is_some() {
                return Err(invalid("New path filters cannot be added in this editor. Edit the config file instead."));
            }
            additions.push((hostname.clone(), service.clone()));
            true
        };
        let identity = (hostname.to_ascii_lowercase(), route.path.clone());
        if let Some(earlier_modified) = identities.insert(identity, modified) {
            if modified || earlier_modified {
                return Err(invalid("Two hostname rules have the same hostname and path. Use a different hostname or remove the duplicate."));
            }
        }
    }
    // Check effective order: additions follow all retained rules.
    let candidates = current
        .routes
        .iter()
        .filter_map(|old| {
            edits
                .get(&old.index.unwrap())
                .map(|(hostname, _)| (Some(old), hostname.as_str(), old.path.is_none()))
        })
        .chain(
            additions
                .iter()
                .map(|(hostname, _)| (None, hostname.as_str(), true)),
        );
    let mut earlier_hostnames: Vec<&str> = Vec::new();
    for (old, hostname, no_path) in candidates {
        if let Some(earlier) = earlier_hostnames
            .iter()
            .find(|earlier| fully_shadows(earlier, hostname))
        {
            let already_shadowed = old.is_some_and(|old| {
                hostname == old.hostname
                    && current
                        .routes
                        .iter()
                        .take_while(|before| before.index < old.index)
                        .any(|before| {
                            before.path.is_none() && fully_shadows(&before.hostname, &old.hostname)
                        })
            });
            if !already_shadowed {
                return Err(invalid(format!(
                    "The hostname `{hostname}` is already matched by the earlier `{earlier}` rule. Edit the config file to place this rule before it, or change or remove the earlier rule."
                )));
            }
        }
        if no_path {
            earlier_hostnames.push(hostname);
        }
    }
    let fallback = if fallback_service == current.fallback_service {
        current.fallback_service
    } else {
        normalize_service(fallback_service)?
    };
    let mut raw = config.raw.clone();
    let rules = raw
        .as_mapping_mut()
        .unwrap()
        .get_mut("ingress")
        .unwrap()
        .as_sequence_mut()
        .unwrap();
    let last = rules.len() - 1;
    let mut updated = Vec::with_capacity(rules.len() + additions.len());
    for (index, mut rule) in std::mem::take(rules).into_iter().enumerate() {
        if index == last {
            for (hostname, service) in &additions {
                let mut added = Mapping::new();
                added.insert("hostname".into(), hostname.clone().into());
                added.insert("service".into(), service.clone().into());
                updated.push(Value::Mapping(added));
            }
            patch(rule.as_mapping_mut().unwrap(), "service", &fallback);
        } else if originals.contains_key(&index) {
            let Some((hostname, service)) = edits.get(&index) else {
                continue;
            };
            let mapping = rule.as_mapping_mut().unwrap();
            patch(mapping, "hostname", hostname);
            patch(mapping, "service", service);
        }
        updated.push(rule);
    }
    *rules = updated;
    let changed = raw != config.raw;
    let text = if changed {
        serde_norway::to_string(&raw).map_err(|source| ConfigError::Yaml {
            path: path.to_path_buf(),
            source,
        })?
    } else {
        original.clone()
    };
    if changed {
        config = TunnelConfig::parse(path, &text)?;
    }
    Ok(PreparedHostnames {
        config,
        text,
        original,
        revision,
        changed,
    })
}

fn read(path: &Path) -> Result<(TunnelConfig, String), ConfigError> {
    let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Ok((TunnelConfig::parse(path, &text)?, text))
}

fn snapshot(config: &TunnelConfig, revision: String) -> Result<HostnameConfig, ConfigError> {
    let rules = config.raw.as_mapping().and_then(|root| root.get("ingress")).and_then(Value::as_sequence)
        .filter(|rules| !rules.is_empty())
        .ok_or_else(|| invalid("This config needs an ingress list ending in a catch-all service. Edit the config file before managing hostnames."))?;
    let mut routes = Vec::new();
    let mut fallback_service = None;
    for (index, value) in rules.iter().enumerate() {
        let rule = value.as_mapping().ok_or_else(|| {
            invalid("Every ingress rule must be a mapping. Fix the config file first.")
        })?;
        let hostname = optional_string(rule, "hostname")?;
        let path = optional_string(rule, "path")?;
        let service = rule.get("service").and_then(Value::as_str).ok_or_else(|| {
            invalid("Every ingress rule needs a text service. Fix the config file first.")
        })?;
        validate_service(service)?;
        if hostname.is_none() && path.is_none() {
            if index != rules.len() - 1 {
                return Err(invalid("The config has a catch-all before its final rule. Move the catch-all to the end in the config file first."));
            }
            fallback_service = Some(service.to_string());
        } else if index == rules.len() - 1 {
            return Err(invalid("The final ingress rule must be an unconditional catch-all service. Fix the config file first."));
        } else if let Some(hostname) = hostname {
            validate_hostname(&hostname)?;
            routes.push(HostnameRoute {
                index: Some(index),
                hostname,
                service: service.to_string(),
                path,
            });
        }
    }
    Ok(HostnameConfig {
        revision,
        routes,
        fallback_service: fallback_service.unwrap(),
    })
}

fn optional_string(rule: &Mapping, key: &str) -> Result<Option<String>, ConfigError> {
    match rule.get(key) {
        None => Ok(None),
        Some(Value::String(value)) if !value.is_empty() => Ok(Some(value.clone())),
        _ => Err(invalid(format!(
            "Ingress `{key}` must be nonempty text when present. Fix the config file first."
        ))),
    }
}

fn validate_hostname(hostname: &str) -> Result<(), ConfigError> {
    if hostname.is_empty()
        || hostname
            .chars()
            .any(|c| c.is_whitespace() || c.is_control())
        || hostname.contains(['/', ':', '\\', '?', '#', '@', '[', ']', '%'])
    {
        return Err(invalid(
            "Enter a hostname without a scheme, port, path, URL delimiters, or spaces.",
        ));
    }
    Ok(())
}

fn validate_service(service: &str) -> Result<(), ConfigError> {
    if service.trim().is_empty() || service.chars().any(char::is_control) {
        return Err(invalid(
            "Enter a service without control characters, such as http://localhost:3000.",
        ));
    }
    Ok(())
}

fn fully_shadows(earlier: &str, hostname: &str) -> bool {
    earlier == "*"
        || earlier == hostname
        || earlier.strip_prefix("*.").is_some_and(|suffix| {
            !suffix.contains('*') && hostname.ends_with(&format!(".{suffix}"))
        })
}

fn normalize_service(service: &str) -> Result<String, ConfigError> {
    validate_service(service)?;
    let service = service.trim();
    // cloudflared's special services are not URL origins.
    if service.contains("://")
        || service.starts_with("http_status:")
        || service.starts_with("unix:")
        || service.starts_with("unix+tls:")
        || matches!(service, "hello_world" | "bastion")
    {
        Ok(service.to_string())
    } else {
        Ok(crate::supervisor::normalize_origin(service))
    }
}

fn patch(rule: &mut Mapping, key: &str, value: &str) {
    if rule.get(key).and_then(Value::as_str) != Some(value) {
        rule.insert(key.into(), value.to_string().into());
    }
}

fn revision(path: &Path, text: &str) -> String {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hash);
    text.hash(&mut hash);
    format!("{:016x}", hash.finish())
}

fn invalid(message: impl Into<String>) -> ConfigError {
    ConfigError::Invalid(message.into())
}
