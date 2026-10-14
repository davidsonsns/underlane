//! Durable, explicit recovery from two project configs sharing one Cloudflare tunnel.
//! The journal contains identities and fingerprints only; source YAML and secrets stay in files.

use std::collections::HashMap;
use std::future::Future;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{bail, ensure, Context};
use serde::{Deserialize, Serialize};
use serde_norway::Value;

use crate::config::{resolve_path, TunnelConfig};
use crate::secret::Secret;
use crate::tunnel_setup::SetupIdentity;

#[derive(Clone, Serialize, Deserialize)]
pub struct Intent {
    pub project_id: String,
    pub attempt_id: String,
    pub source_path: PathBuf,
    pub source_revision: String,
    pub old_tunnel_id: String,
    pub hostnames: Vec<String>,
}

pub fn revision(bytes: &[u8]) -> String {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hash);
    format!("{:016x}", hash.finish())
}

fn project_id(id: &str) -> anyhow::Result<String> {
    crate::dns::canonical_tunnel_id(id).context("The project ID is invalid.")
}

fn intent_path(data: &Path, id: &str) -> anyhow::Result<PathBuf> {
    Ok(data
        .join("separate-tunnel")
        .join(format!("{}.json", project_id(id)?)))
}

pub fn read_intent(data: &Path, id: &str) -> anyhow::Result<Option<Intent>> {
    let path = intent_path(data, id)?;
    if let Ok(metadata) = std::fs::symlink_metadata(path.parent().unwrap()) {
        ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "The recovery directory must be a regular directory."
        );
    }
    let metadata = match std::fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => bail!("Could not read the saved tunnel recovery. Retry."),
    };
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() <= 16 * 1024,
        "The saved tunnel recovery is invalid. It was preserved."
    );
    let intent: Intent = serde_json::from_slice(&std::fs::read(&path)?)
        .map_err(|_| anyhow::anyhow!("The saved tunnel recovery is invalid. It was preserved."))?;
    ensure!(
        intent.project_id == project_id(id)?
            && project_id(&intent.attempt_id).is_ok()
            && crate::dns::canonical_tunnel_id(&intent.old_tunnel_id).is_ok()
            && intent.source_path.is_absolute()
            && intent.source_revision.len() == 16
            && intent.hostnames.len() <= 50
            && intent
                .hostnames
                .iter()
                .all(|host| crate::dns::canonical_hostname(host)
                    .is_ok_and(|canonical| canonical == *host)),
        "The saved tunnel recovery is invalid. It was preserved."
    );
    Ok(Some(intent))
}

/// The committed project config pointer proves DNS/config recovery completed. Remove only its
/// journal; credentials, the provisioning namespace, and both configs remain available.
pub fn clear_completed_intent(
    data: &Path,
    home: &Path,
    id: &str,
    selected_config: &Path,
) -> anyhow::Result<bool> {
    let Some(intent) = read_intent(data, id)? else {
        return Ok(false);
    };
    if selected_config != intent.config_path(home) {
        return Ok(false);
    }
    let path = intent_path(data, id)?;
    match std::fs::remove_file(&path) {
        Ok(()) => {},
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(true),
        Err(_) => bail!("The separate tunnel is ready, but its completed recovery journal could not be cleared. Check the recovery directory permissions and retry."),
    }
    #[cfg(unix)]
    {
        std::fs::File::open(path.parent().unwrap())?.sync_all()?;
    }
    Ok(true)
}

impl Intent {
    pub fn new(
        id: &str,
        config: &TunnelConfig,
        original: &str,
        old_id: &str,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            project_id: project_id(id)?,
            attempt_id: uuid::Uuid::new_v4().to_string(),
            source_path: config.source.clone(),
            source_revision: revision(original.as_bytes()),
            old_tunnel_id: crate::dns::canonical_tunnel_id(old_id)?,
            hostnames: hostnames(config)?,
        })
    }

    pub fn config_path(&self, home: &Path) -> PathBuf {
        home.join(".cloudflared/underlane")
            .join(format!("{}-separate.yml", self.attempt_id))
    }

    pub fn credentials_path(&self, home: &Path) -> PathBuf {
        home.join(".cloudflared/underlane")
            .join(format!("{}.json", self.attempt_id))
    }

    pub fn name(&self) -> String {
        format!("Underlane separate {}", self.attempt_id)
    }

    pub fn save(&self, data: &Path) -> anyhow::Result<()> {
        let path = intent_path(data, &self.project_id)?;
        let bytes = serde_json::to_vec(self)?;
        publish_new(&path, &bytes)
    }
}

/// Complete, durable publication without overwriting another recovery or user file.
pub fn publish_new(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let directory = path
        .parent()
        .context("The recovery file has no parent directory.")?;
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(directory)?;
    let metadata = std::fs::symlink_metadata(directory)?;
    ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "The recovery directory must be a regular directory."
    );
    if let Ok(metadata) = std::fs::symlink_metadata(path) {
        ensure!(
            metadata.is_file() && !metadata.file_type().is_symlink(),
            "The recovery file is not a regular file. It was preserved."
        );
        ensure!(
            std::fs::read(path)? == bytes,
            "The recovery file changed. It was preserved; restore it before retrying."
        );
        return Ok(());
    }
    let temporary = directory.join(format!(".separate-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::hard_link(&temporary, path)
            .context("Could not publish recovery safely. Retry to check the saved files.")?;
        #[cfg(unix)]
        {
            std::fs::File::open(directory)?.sync_all()?;
        }
        Ok(())
    })();
    let _ = std::fs::remove_file(temporary);
    result
}

pub fn hostnames(config: &TunnelConfig) -> anyhow::Result<Vec<String>> {
    ensure!(
        !config.is_single_origin(),
        "Single-origin configs cannot be separated automatically."
    );
    let root = config
        .raw
        .as_mapping()
        .context("The config must be a YAML mapping.")?;
    ensure!(
        !root.contains_key("token") && !root.contains_key("token-file"),
        "Token-authenticated configs cannot be separated automatically."
    );
    ensure!(!config.ingress.iter().any(|rule| rule.hostname.is_none() && rule.path.is_some()), "Path-only rules can match unknown hostnames. Give them explicit hostnames before separating this tunnel.");
    let mut names = config
        .ingress
        .iter()
        .filter_map(|rule| rule.hostname.as_deref())
        .map(crate::dns::canonical_hostname)
        .collect::<anyhow::Result<Vec<_>>>()?;
    names.sort();
    names.dedup();
    ensure!(
        names.len() <= 50,
        "Separate at most 50 hostnames at a time. Split this config before continuing."
    );
    Ok(names)
}

/// DNS belongs to the hostname, even when local profiles split its path filters.
pub fn overlaps(a: &str, b: &str) -> bool {
    let suffix_matches = |wildcard: &str, other: &str| {
        wildcard
            .strip_prefix("*.")
            .is_some_and(|suffix| other.ends_with(&format!(".{suffix}")))
    };
    a == b || suffix_matches(a, b) || suffix_matches(b, a)
}

pub fn config_text(config: &TunnelConfig, identity: &SetupIdentity) -> anyhow::Result<String> {
    hostnames(config)?;
    let mut raw = crate::derived::derive(config, &HashMap::new());
    // Once the config moves, its Unix socket services must keep resolving from the old folder.
    if let Some(rules) = raw
        .as_mapping_mut()
        .and_then(|root| root.get_mut("ingress"))
        .and_then(Value::as_sequence_mut)
    {
        for rule in rules {
            if let Some(map) = rule.as_mapping_mut() {
                if let Some(service) = map.get("service").and_then(Value::as_str) {
                    if let Some((scheme, path)) = service
                        .split_once(':')
                        .filter(|(scheme, _)| matches!(*scheme, "unix" | "unix+tls"))
                    {
                        let absolute =
                            resolve_path(config.source.parent().unwrap_or(Path::new(".")), path);
                        map.insert(
                            "service".into(),
                            format!("{scheme}:{}", absolute.display()).into(),
                        );
                    }
                }
            }
        }
    }
    let absolute = TunnelConfig::parse(&config.source, &serde_norway::to_string(&raw)?)?;
    crate::tunnel_setup::reidentified_config(&absolute, identity)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DnsMove {
    Missing,
    AlreadyMoved,
    Move(String),
}

fn cf_id(id: &str) -> anyhow::Result<&str> {
    ensure!(
        id.len() == 32 && id.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "Cloudflare returned an invalid DNS record or zone ID."
    );
    Ok(id)
}

fn payload(bytes: &[u8]) -> anyhow::Result<serde_json::Value> {
    let response: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|_| anyhow::anyhow!("Cloudflare returned invalid DNS migration data. Retry."))?;
    ensure!(response.get("success").and_then(serde_json::Value::as_bool) == Some(true), "Cloudflare rejected DNS migration. The login needs DNS Write access; retry after restoring access.");
    Ok(response)
}

fn record_move(
    record: &serde_json::Value,
    hostname: &str,
    old: &str,
    new: Option<&str>,
) -> anyhow::Result<DnsMove> {
    let name = record
        .get("name")
        .and_then(serde_json::Value::as_str)
        .context("Cloudflare returned invalid DNS migration data.")?;
    ensure!(
        crate::dns::canonical_hostname(name).is_ok_and(|name| name == hostname),
        "Cloudflare returned a DNS record for another hostname. It was preserved."
    );
    let id = cf_id(
        record
            .get("id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(""),
    )?;
    ensure!(record.get("type").and_then(serde_json::Value::as_str) == Some("CNAME") && record.get("proxied").and_then(serde_json::Value::as_bool) == Some(true), "This hostname has an unrelated or unproxied DNS record. It was preserved; review it in Cloudflare before separating the tunnel.");
    let target = record
        .get("content")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .trim_end_matches('.');
    if new.is_some_and(|new| target.eq_ignore_ascii_case(&format!("{new}.cfargotunnel.com"))) {
        return Ok(DnsMove::AlreadyMoved);
    }
    ensure!(target.eq_ignore_ascii_case(&format!("{old}.cfargotunnel.com")), "This hostname points somewhere other than the original tunnel. Its DNS record was preserved.");
    Ok(DnsMove::Move(id.to_owned()))
}

pub fn plan_dns(
    bytes: &[u8],
    hostname: &str,
    old: &str,
    new: Option<&str>,
) -> anyhow::Result<DnsMove> {
    let hostname = crate::dns::canonical_hostname(hostname)?;
    let old = crate::dns::canonical_tunnel_id(old)?;
    let new = new.map(crate::dns::canonical_tunnel_id).transpose()?;
    let response = payload(bytes)?;
    let records = response
        .get("result")
        .and_then(serde_json::Value::as_array)
        .context("Cloudflare returned invalid DNS migration data.")?;
    if let Some(info) = response.get("result_info") {
        ensure!(
            info.get("page")
                .and_then(serde_json::Value::as_u64)
                .is_none_or(|page| page == 1)
                && info
                    .get("total_pages")
                    .and_then(serde_json::Value::as_u64)
                    .is_none_or(|pages| pages <= 1)
                && info
                    .get("count")
                    .and_then(serde_json::Value::as_u64)
                    .is_none_or(|count| count == records.len() as u64)
                && info
                    .get("total_count")
                    .and_then(serde_json::Value::as_u64)
                    .is_none_or(|count| count == records.len() as u64),
            "Cloudflare returned incomplete DNS records. Nothing was moved; retry verification."
        );
    }
    match records.as_slice() {
        [] => Ok(DnsMove::Missing),
        [record] => record_move(record, &hostname, &old, new.as_deref()),
        _ => bail!("This hostname has multiple DNS records. They were preserved; review them in Cloudflare before separating the tunnel."),
    }
}

fn list_url(zone: &str, hostname: &str) -> anyhow::Result<String> {
    let zone = cf_id(zone)?;
    let hostname = crate::dns::canonical_hostname(hostname)?.replace('*', "%2A");
    Ok(format!("https://api.cloudflare.com/client/v4/zones/{zone}/dns_records?name.exact={hostname}&per_page=100"))
}

pub async fn check_dns(
    token: &Secret<String>,
    zone: &str,
    hostname: &str,
    old: &str,
    new: Option<&str>,
) -> anyhow::Result<DnsMove> {
    plan_dns(
        &crate::dns::bounded_get(&list_url(zone, hostname)?, Some(token)).await?,
        hostname,
        old,
        new,
    )
}

async fn move_with<F, Fut>(
    zone: &str,
    hostname: &str,
    old: &str,
    new: &str,
    mut request: F,
) -> anyhow::Result<DnsMove>
where
    F: FnMut(hyper::Method, String, Vec<u8>) -> Fut,
    Fut: Future<Output = anyhow::Result<Vec<u8>>>,
{
    let new = crate::dns::canonical_tunnel_id(new)?;
    ensure!(
        crate::dns::canonical_tunnel_id(old)? != new,
        "The separate tunnel must have a new identity."
    );
    let list = list_url(zone, hostname)?;
    let current = request(hyper::Method::GET, list.clone(), Vec::new()).await?;
    let decision = plan_dns(&current, hostname, old, Some(&new))?;
    let (method, url, body) = match &decision {
        DnsMove::AlreadyMoved => return Ok(decision),
        DnsMove::Missing => (
            hyper::Method::POST,
            format!("https://api.cloudflare.com/client/v4/zones/{zone}/dns_records"),
            serde_json::json!({"type":"CNAME", "name":crate::dns::canonical_hostname(hostname)?, "content":format!("{new}.cfargotunnel.com"), "proxied":true, "ttl":1}),
        ),
        DnsMove::Move(record_id) => {
            let record_url = format!(
                "https://api.cloudflare.com/client/v4/zones/{zone}/dns_records/{record_id}"
            );
            let current =
                payload(&request(hyper::Method::GET, record_url.clone(), Vec::new()).await?)?;
            let current = record_move(
                current
                    .get("result")
                    .context("Cloudflare returned invalid DNS migration data.")?,
                hostname,
                old,
                Some(&new),
            )?;
            if current == DnsMove::AlreadyMoved {
                return Ok(current);
            }
            ensure!(
                current == decision,
                "The DNS record changed during recovery. It was preserved; retry verification."
            );
            // PATCH addresses only this record; it cannot delete a newly added A/AAAA or another CNAME.
            // shortcut: Cloudflare has no documented conditional record update; a concurrent edit
            // to this exact record between the final read and PATCH remains an optimistic race.
            (
                hyper::Method::PATCH,
                record_url,
                serde_json::json!({"content":format!("{new}.cfargotunnel.com")}),
            )
        }
    };
    let updated = payload(&request(method, url, serde_json::to_vec(&body)?).await?)?;
    ensure!(
        record_move(
            updated
                .get("result")
                .context("Cloudflare returned invalid DNS migration data.")?,
            hostname,
            old,
            Some(&new)
        )? == DnsMove::AlreadyMoved,
        "Cloudflare did not confirm the new DNS target. Retry recovery to verify its state."
    );
    ensure!(
        plan_dns(
            &request(hyper::Method::GET, list, Vec::new()).await?,
            hostname,
            old,
            Some(&new)
        )? == DnsMove::AlreadyMoved,
        "DNS changed after migration. Retry recovery to verify its state."
    );
    Ok(decision)
}

pub async fn move_dns(
    token: &Secret<String>,
    zone: &str,
    hostname: &str,
    old: &str,
    new: &str,
) -> anyhow::Result<DnsMove> {
    move_with(zone, hostname, old, new, |method, url, body| async move {
        if method == hyper::Method::GET {
            crate::dns::bounded_get(&url, Some(token)).await
        } else if method == hyper::Method::POST {
            crate::dns::bounded_post(&url, token, body).await
        } else {
            crate::dns::bounded_patch(&url, token, body).await
        }
    })
    .await
}

pub fn saved_identity(
    path: &Path,
    account: &Secret<String>,
) -> anyhow::Result<Option<SetupIdentity>> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => crate::tunnel_setup::credential_identity(path, account).map(Some),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => bail!("Could not inspect saved recovery credentials."),
    }
}

#[cfg(test)]
#[path = "../tests/common/fake_cloudflared.rs"]
#[allow(dead_code)]
mod fake_cloudflared;

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    const OLD: &str = "00000000-0000-4000-8000-000000000001";
    const NEW: &str = "00000000-0000-4000-8000-000000000002";
    const ZONE: &str = "00000000000000000000000000000003";
    const RECORD: &str = "00000000000000000000000000000004";
    const HOST: &str = "app.example.invalid";

    fn record(target: &str) -> serde_json::Value {
        serde_json::json!({"id":RECORD,"type":"CNAME","name":HOST,"content":format!("{target}.cfargotunnel.com"),"proxied":true,"ttl":300,"comment":"keep me"})
    }
    fn list(records: Vec<serde_json::Value>) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({"success":true,"result":records})).unwrap()
    }
    fn single(record: serde_json::Value) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({"success":true,"result":record})).unwrap()
    }

    #[test]
    fn planner_rejects_unrelated_unproxied_multiple_and_incomplete_records() {
        assert_eq!(
            plan_dns(&list(vec![]), HOST, OLD, Some(NEW)).unwrap(),
            DnsMove::Missing
        );
        assert_eq!(
            plan_dns(&list(vec![record(OLD)]), HOST, OLD, Some(NEW)).unwrap(),
            DnsMove::Move(RECORD.into())
        );
        assert_eq!(
            plan_dns(&list(vec![record(NEW)]), HOST, OLD, Some(NEW)).unwrap(),
            DnsMove::AlreadyMoved
        );
        let unrelated = "00000000-0000-4000-8000-000000000099";
        let mut unproxied = record(OLD);
        unproxied["proxied"] = false.into();
        let mut address = record(OLD);
        address["type"] = "A".into();
        address["content"] = "127.0.0.1".into();
        let mut wrong_name = record(OLD);
        wrong_name["name"] = "other.example.invalid".into();
        for records in [
            vec![record(unrelated)],
            vec![unproxied],
            vec![address],
            vec![wrong_name],
            vec![record(OLD), record(OLD)],
        ] {
            assert!(plan_dns(&list(records), HOST, OLD, Some(NEW)).is_err());
        }
        let incomplete = serde_json::to_vec(&serde_json::json!({"success":true,"result":[record(OLD)],"result_info":{"total_pages":2,"count":1,"total_count":101}})).unwrap();
        assert!(plan_dns(&incomplete, HOST, OLD, Some(NEW)).is_err());
    }

    #[tokio::test]
    async fn migration_patches_only_the_verified_record_and_is_idempotent() {
        let replies = Arc::new(Mutex::new(VecDeque::from([
            list(vec![record(OLD)]),
            single(record(OLD)),
            single(record(NEW)),
            list(vec![record(NEW)]),
        ])));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let output = move_with(ZONE, HOST, OLD, NEW, |method, url, body| {
            requests.lock().unwrap().push((method, url, body));
            let response = replies.lock().unwrap().pop_front().unwrap();
            async { Ok(response) }
        })
        .await
        .unwrap();
        assert_eq!(output, DnsMove::Move(RECORD.into()));
        {
            let calls = requests.lock().unwrap();
            assert_eq!(
                calls.iter().map(|call| call.0.clone()).collect::<Vec<_>>(),
                vec![
                    hyper::Method::GET,
                    hyper::Method::GET,
                    hyper::Method::PATCH,
                    hyper::Method::GET
                ]
            );
            assert!(calls[2].1.ends_with(&format!("/dns_records/{RECORD}")));
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&calls[2].2).unwrap(),
                serde_json::json!({"content":format!("{NEW}.cfargotunnel.com")})
            );
        }
        let calls = Arc::new(Mutex::new(0));
        assert_eq!(
            move_with(ZONE, HOST, OLD, NEW, |method, _, _| {
                assert_eq!(method, hyper::Method::GET);
                *calls.lock().unwrap() += 1;
                async { Ok(list(vec![record(NEW)])) }
            })
            .await
            .unwrap(),
            DnsMove::AlreadyMoved
        );
        assert_eq!(*calls.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn missing_reviewed_hostname_is_created_without_overwrite() {
        let replies = Arc::new(Mutex::new(VecDeque::from([
            list(vec![]),
            single(record(NEW)),
            list(vec![record(NEW)]),
        ])));
        let calls = Arc::new(Mutex::new(Vec::new()));
        assert_eq!(
            move_with(ZONE, HOST, OLD, NEW, |method, url, body| {
                calls.lock().unwrap().push((method, url, body));
                let reply = replies.lock().unwrap().pop_front().unwrap();
                async { Ok(reply) }
            })
            .await
            .unwrap(),
            DnsMove::Missing
        );
        let calls = calls.lock().unwrap();
        assert_eq!(calls[1].0, hyper::Method::POST);
        assert!(calls[1].1.ends_with("/dns_records"));
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&calls[1].2).unwrap(),
            serde_json::json!({"type":"CNAME","name":HOST,"content":format!("{NEW}.cfargotunnel.com"),"proxied":true,"ttl":1})
        );
    }

    #[tokio::test]
    async fn changed_record_and_ambiguous_write_require_verification_before_retry() {
        let calls = Arc::new(Mutex::new(0));
        let replies = Arc::new(Mutex::new(VecDeque::from([
            list(vec![record(OLD)]),
            single(record("00000000-0000-4000-8000-000000000099")),
        ])));
        assert!(move_with(ZONE, HOST, OLD, NEW, |method, _, _| {
            assert_eq!(method, hyper::Method::GET);
            *calls.lock().unwrap() += 1;
            let reply = replies.lock().unwrap().pop_front().unwrap();
            async { Ok(reply) }
        })
        .await
        .is_err());
        assert_eq!(*calls.lock().unwrap(), 2);
        let state = Arc::new(Mutex::new(record(OLD)));
        assert!(move_with(ZONE, HOST, OLD, NEW, |method, url, _| {
            let output = if method == hyper::Method::PATCH {
                *state.lock().unwrap() = record(NEW);
                Err(anyhow::anyhow!("simulated lost response"))
            } else if url.contains('?') {
                Ok(list(vec![state.lock().unwrap().clone()]))
            } else {
                Ok(single(state.lock().unwrap().clone()))
            };
            async { output }
        })
        .await
        .is_err());
        assert_eq!(
            move_with(ZONE, HOST, OLD, NEW, |method, _, _| {
                assert_eq!(method, hyper::Method::GET);
                let output = list(vec![state.lock().unwrap().clone()]);
                async { Ok(output) }
            })
            .await
            .unwrap(),
            DnsMove::AlreadyMoved
        );
    }

    #[test]
    fn moving_config_preserves_advanced_routes_and_absolutizes_relative_paths() {
        let config = TunnelConfig::parse(Path::new("/original/workspace/tunnel.yml"), &format!("tunnel: {OLD}\ncredentials-file: old.json\norigincert: cert.pem\nlogfile: tunnel.log\noriginRequest:\n  caPool: root-ca.pem\n  connectTimeout: 9s\ningress:\n - hostname: app.example.invalid\n   path: ^/api\n   service: unix:sockets/app.sock\n   originRequest:\n     caPool: rule-ca.pem\n     noTLSVerify: true\n     httpHostHeader: internal.invalid\n - service: http_status:404\n")).unwrap();
        let identity = SetupIdentity {
            tunnel_id: NEW.into(),
            credentials_file: "/app/separate.json".into(),
        };
        let text = config_text(&config, &identity).unwrap();
        let moved = TunnelConfig::parse(Path::new("/app/separate.yml"), &text).unwrap();
        assert_eq!(moved.tunnel.as_deref(), Some(NEW));
        assert_eq!(
            moved.ingress[0].service,
            "unix:/original/workspace/sockets/app.sock"
        );
        assert_eq!(moved.ingress[0].path.as_deref(), Some("^/api"));
        assert_eq!(
            moved.raw["originRequest"]["caPool"].as_str(),
            Some("/original/workspace/root-ca.pem")
        );
        assert_eq!(
            moved.raw["ingress"][0]["originRequest"]["caPool"].as_str(),
            Some("/original/workspace/rule-ca.pem")
        );
        assert_eq!(
            moved.raw["ingress"][0]["originRequest"]["httpHostHeader"].as_str(),
            Some("internal.invalid")
        );
        assert_eq!(
            moved.raw["origincert"].as_str(),
            Some("/original/workspace/cert.pem")
        );
        assert_eq!(
            moved.raw["logfile"].as_str(),
            Some("/original/workspace/tunnel.log")
        );
    }

    #[test]
    fn recovery_intent_and_config_publication_are_durable_and_do_not_overwrite() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source.yml");
        let text = format!("tunnel: {OLD}\ningress:\n - hostname: {HOST}\n   service: http://localhost:8080\n - service: http_status:404\n");
        let config = TunnelConfig::parse(&source, &text).unwrap();
        let intent = Intent::new(NEW, &config, &text, OLD).unwrap();
        intent.save(directory.path()).unwrap();
        let loaded = read_intent(directory.path(), NEW).unwrap().unwrap();
        assert_eq!(loaded.attempt_id, intent.attempt_id);
        assert_eq!(loaded.hostnames, vec![HOST]);
        intent.save(directory.path()).unwrap();
        let different = Intent::new(NEW, &config, &text, OLD).unwrap();
        assert!(different.save(directory.path()).is_err());
        assert_eq!(
            read_intent(directory.path(), NEW)
                .unwrap()
                .unwrap()
                .attempt_id,
            intent.attempt_id
        );
        let target = intent.config_path(directory.path());
        publish_new(&target, b"source contents").unwrap();
        publish_new(&target, b"source contents").unwrap();
        assert!(publish_new(&target, b"replacement").is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"source contents");
    }

    #[test]
    fn committed_recovery_clears_only_its_journal_and_allows_a_fresh_action() {
        let directory = tempfile::tempdir().unwrap();
        let original_path = directory.path().join("original.yml");
        let original = format!("tunnel: {OLD}\ningress: [{{service: http_status:404}}]\n");
        let config = TunnelConfig::parse(&original_path, &original).unwrap();
        let intent = Intent::new(NEW, &config, &original, OLD).unwrap();
        intent.save(directory.path()).unwrap();
        let target = intent.config_path(directory.path());
        let credentials = intent.credentials_path(directory.path());
        let setup = directory
            .path()
            .join("project-setup")
            .join(format!("{}.json", intent.attempt_id));
        let identity = SetupIdentity {
            tunnel_id: NEW.into(),
            credentials_file: credentials.clone(),
        };
        let text = config_text(&config, &identity).unwrap();
        publish_new(&target, text.as_bytes()).unwrap();
        publish_new(&credentials, b"saved credentials").unwrap();
        publish_new(&setup, b"saved provisioning namespace").unwrap();
        assert!(
            !clear_completed_intent(directory.path(), directory.path(), NEW, &original_path)
                .unwrap()
        );
        assert!(read_intent(directory.path(), NEW).unwrap().is_some());
        // Crash after DB pointer commit: the original can change/disappear without blocking cleanup.
        assert!(clear_completed_intent(directory.path(), directory.path(), NEW, &target).unwrap());
        assert!(read_intent(directory.path(), NEW).unwrap().is_none());
        assert!(!clear_completed_intent(directory.path(), directory.path(), NEW, &target).unwrap());
        assert_eq!(std::fs::read(&target).unwrap(), text.as_bytes());
        assert_eq!(std::fs::read(&credentials).unwrap(), b"saved credentials");
        assert_eq!(
            std::fs::read(&setup).unwrap(),
            b"saved provisioning namespace"
        );
        let current = TunnelConfig::load(&target).unwrap();
        let next = Intent::new(NEW, &current, &text, NEW).unwrap();
        assert_ne!(next.attempt_id, intent.attempt_id);
        next.save(directory.path()).unwrap();
        assert_eq!(
            read_intent(directory.path(), NEW)
                .unwrap()
                .unwrap()
                .source_path,
            target
        );
    }

    #[test]
    fn shared_hostname_paths_and_wildcards_cannot_be_split_into_dns_ownership() {
        assert!(overlaps("*.example.invalid", HOST));
        assert!(overlaps(HOST, "*.example.invalid"));
        assert!(overlaps("*.example.invalid", "*.nested.example.invalid"));
        assert!(!overlaps("*.example.invalid", "example.invalid"));
        assert!(!overlaps(HOST, "other.example.invalid"));
        for text in [
            "token: secret\ningress: [{service: http_status:404}]",
            "url: http://localhost:8080",
            "ingress: [{path: ^/api, service: http://localhost:8080}, {service: http_status:404}]",
        ] {
            assert!(hostnames(
                &TunnelConfig::parse(Path::new("/synthetic/source.yml"), text).unwrap()
            )
            .is_err());
        }
    }

    #[tokio::test]
    async fn fresh_session_retry_reuses_one_created_tunnel_after_partial_dns_failure() {
        use crate::cli::Cancellation;
        let fixture = fake_cloudflared::fake("ok");
        let cli = include_str!("../tests/common/setup_cloudflared.py")
            .replace(&format!("ident = \"{OLD}\""), &format!("ident = \"{NEW}\""));
        std::fs::write(&fixture.bin, cli).unwrap();
        let cert = fixture.root.join("pinned.pem");
        std::fs::write(&cert, "-----BEGIN ARGO TUNNEL TOKEN-----\neyJhY2NvdW50SUQiOiJzeW50aGV0aWMtYWNjb3VudCIsInpvbmVJRCI6InN5bnRoZXRpYy16b25lIiwiYXBpVG9rZW4iOiJzeW50aGV0aWMtYXBpLXRva2VuIn0=\n-----END ARGO TUNNEL TOKEN-----\n").unwrap();
        let original_path = fixture.root.join("original/source.yml");
        std::fs::create_dir_all(original_path.parent().unwrap()).unwrap();
        let second = "api.example.invalid";
        let original = format!("tunnel: {OLD}\ncredentials-file: old.json\ningress:\n - hostname: {HOST}\n   service: http://localhost:8080\n - hostname: {second}\n   service: http://localhost:8081\n - service: http_status:404\n");
        std::fs::write(&original_path, &original).unwrap();
        let config = TunnelConfig::parse(&original_path, &original).unwrap();
        let intent = Intent::new(
            "00000000-0000-4000-8000-000000000007",
            &config,
            &original,
            OLD,
        )
        .unwrap();
        intent.save(&fixture.data).unwrap();
        let identity = crate::tunnel_setup::setup_separate(
            &fixture.bin,
            &fixture.data,
            &fixture.root,
            &intent.attempt_id,
            &intent.name(),
            &cert,
            Cancellation::new(),
            |_| {},
        )
        .await
        .unwrap();
        assert_eq!(identity.tunnel_id, NEW);
        let target = intent.config_path(&fixture.root);
        publish_new(&target, config_text(&config, &identity).unwrap().as_bytes()).unwrap();
        let mut other_record = record(OLD);
        other_record["id"] = "00000000000000000000000000000005".into();
        other_record["name"] = second.into();
        let records = Arc::new(Mutex::new(HashMap::from([
            (HOST.to_owned(), record(OLD)),
            (second.to_owned(), other_record),
        ])));
        let patches = Arc::new(Mutex::new(Vec::new()));
        let respond = |method: hyper::Method, url: String, body: Vec<u8>, fail: bool| {
            let mut records = records.lock().unwrap();
            if url.contains('?') {
                let name = url
                    .split("name.exact=")
                    .nth(1)
                    .unwrap()
                    .split('&')
                    .next()
                    .unwrap();
                return Ok(list(vec![records[name].clone()]));
            }
            let id = url.rsplit('/').next().unwrap();
            let record = records
                .values_mut()
                .find(|record| record["id"] == id)
                .unwrap();
            if method == hyper::Method::PATCH {
                if fail {
                    return Err(anyhow::anyhow!("simulated second-hostname DNS failure"));
                }
                let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
                record["content"] = body["content"].clone();
                patches
                    .lock()
                    .unwrap()
                    .push(record["name"].as_str().unwrap().to_owned());
            }
            Ok(single(record.clone()))
        };
        move_with(ZONE, HOST, OLD, &identity.tunnel_id, |method, url, body| {
            let result = respond(method, url, body, false);
            async { result }
        })
        .await
        .unwrap();
        assert!(move_with(
            ZONE,
            second,
            OLD,
            &identity.tunnel_id,
            |method, url, body| {
                let result = respond(method, url, body, true);
                async { result }
            }
        )
        .await
        .is_err());
        assert_eq!(*patches.lock().unwrap(), vec![HOST]);
        // A fresh process reads only the journal/files, with no previous workflow object.
        let project = intent.project_id.clone();
        let attempt = intent.attempt_id.clone();
        drop(intent);
        drop(identity);
        let intent = read_intent(&fixture.data, &project).unwrap().unwrap();
        assert_eq!(intent.attempt_id, attempt);
        let identity = crate::tunnel_setup::setup_separate(
            &fixture.bin,
            &fixture.data,
            &fixture.root,
            &intent.attempt_id,
            &intent.name(),
            &cert,
            Cancellation::new(),
            |_| {},
        )
        .await
        .unwrap();
        assert_eq!(identity.tunnel_id, NEW);
        let calls = std::fs::read_to_string(fixture.root.join("bin/calls.jsonl")).unwrap();
        assert_eq!(
            calls
                .lines()
                .filter(|line| serde_json::from_str::<Vec<String>>(line).unwrap()[5] == "create")
                .count(),
            1
        );
        let config = TunnelConfig::load(&intent.source_path).unwrap();
        publish_new(&target, config_text(&config, &identity).unwrap().as_bytes()).unwrap();
        for hostname in &intent.hostnames {
            move_with(
                ZONE,
                hostname,
                OLD,
                &identity.tunnel_id,
                |method, url, body| {
                    let result = respond(method, url, body, false);
                    async { result }
                },
            )
            .await
            .unwrap();
        }
        assert_eq!(*patches.lock().unwrap(), vec![HOST, second]);
        assert_eq!(std::fs::read_to_string(&original_path).unwrap(), original);
        assert_eq!(
            TunnelConfig::load(&target).unwrap().tunnel.as_deref(),
            Some(NEW)
        );
    }

    #[test]
    fn saved_credentials_cannot_cross_the_pinned_certificate_account() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("old.json");
        std::fs::write(&path, serde_json::to_vec(&serde_json::json!({"AccountTag":"account-a", "TunnelID":OLD,"TunnelSecret":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="})).unwrap()).unwrap();
        assert!(saved_identity(&path, &Secret::new("account-a".into()))
            .unwrap()
            .is_some());
        assert!(saved_identity(&path, &Secret::new("account-b".into())).is_err());
    }
}
