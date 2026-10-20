//! Public DNS observations and explicit, non-overwriting cloudflared DNS routing.

use std::ffi::OsString;
use std::future::Future;
use std::path::Path;
use std::time::Duration;

use anyhow::{ensure, Context};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::Request;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use serde::{Deserialize, Serialize};

use crate::cli::{self, ConfigPolicy, Invocation, StdoutMode};
use crate::secret::Secret;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DnsStatus {
    Connected,
    Missing,
    Conflict,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostnameDns {
    pub status: DnsStatus,
    pub message: String,
    pub can_create: bool,
    pub needs_dns_access: bool,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub expected_tunnel_id: Option<String>,
    #[serde(default)]
    pub observed_tunnel_id: Option<String>,
    #[serde(default)]
    pub actual_target: Option<String>,
    #[serde(default)]
    pub can_repair: bool,
    #[serde(default = "public_source")]
    pub source: String,
    #[serde(default)]
    pub checked_at: i64,
}

fn public_source() -> String {
    "public".into()
}

pub fn checked_at() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

impl HostnameDns {
    fn new(status: DnsStatus, message: &str, can_create: bool) -> Self {
        Self {
            status,
            message: message.into(),
            can_create,
            needs_dns_access: false,
            reason: None,
            expected_tunnel_id: None,
            observed_tunnel_id: None,
            actual_target: None,
            can_repair: false,
            source: public_source(),
            checked_at: checked_at(),
        }
    }

    pub fn unknown(message: &str) -> Self {
        Self::new(DnsStatus::Unknown, message, false)
    }
}

pub fn canonical_hostname(hostname: &str) -> anyhow::Result<String> {
    let hostname = hostname.strip_suffix('.').unwrap_or(hostname);
    let domain = hostname.strip_prefix("*.").unwrap_or(hostname);
    ensure!(
        hostname.len() <= 253 && domain.contains('.') && domain.is_ascii(),
        "Enter a valid ASCII hostname, such as app.example.com."
    );
    ensure!(
        domain.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label.as_bytes()[0].is_ascii_alphanumeric()
                && label.as_bytes()[label.len() - 1].is_ascii_alphanumeric()
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        }),
        "Enter a valid ASCII hostname, such as app.example.com."
    );
    Ok(hostname.to_ascii_lowercase())
}

pub fn canonical_tunnel_id(tunnel_id: &str) -> anyhow::Result<String> {
    let id = uuid::Uuid::parse_str(tunnel_id)
        .context("Set up a valid tunnel identity before creating DNS records.")?;
    ensure!(
        !id.is_nil() && tunnel_id.len() == 36 && id.to_string().eq_ignore_ascii_case(tunnel_id),
        "Set up a valid tunnel identity before creating DNS records."
    );
    Ok(id.to_string())
}

fn inputs(hostname: &str, tunnel_id: &str) -> anyhow::Result<(String, uuid::Uuid)> {
    let hostname = canonical_hostname(hostname)?;
    let id = canonical_tunnel_id(tunnel_id)?;
    Ok((
        hostname,
        uuid::Uuid::parse_str(&id).expect("canonical tunnel ID"),
    ))
}

#[derive(Deserialize)]
struct DnsResponse {
    #[serde(rename = "Status")]
    status: u32,
    #[serde(rename = "Answer", default)]
    answer: Vec<DnsAnswer>,
    #[serde(rename = "TC", default)]
    truncated: bool,
}

#[derive(Deserialize)]
struct DnsAnswer {
    name: String,
    #[serde(rename = "type")]
    kind: u32,
    data: String,
}

fn response(bytes: &[u8], hostname: &str, id: uuid::Uuid) -> HostnameDns {
    let mut result = public_response(bytes, hostname, id);
    result.expected_tunnel_id = Some(id.to_string());
    result
}

fn public_response(bytes: &[u8], hostname: &str, id: uuid::Uuid) -> HostnameDns {
    let Ok(response) = serde_json::from_slice::<DnsResponse>(bytes) else {
        return HostnameDns::unknown(
            "Could not verify DNS: the resolver returned an invalid response. Retry.",
        );
    };
    if response.truncated {
        return HostnameDns::unknown(
            "Could not verify DNS: the resolver response was incomplete. Retry.",
        );
    }
    match response.status {
        0 | 3 => {
            // A proxied Cloudflare record hides its CNAME. Absence is not proof of a missing route.
            let mut cnames = response.answer.iter().filter(|answer| {
                answer.kind == 5
                    && answer
                        .name
                        .trim_end_matches('.')
                        .eq_ignore_ascii_case(hostname)
            });
            let Some(cname) = cnames.next() else {
                if response.status == 3 {
                    return if response.answer.is_empty() {
                        HostnameDns::new(
                            DnsStatus::Missing,
                            "No public DNS record was found for this hostname.",
                            true,
                        )
                    } else {
                        HostnameDns::unknown("Public DNS returned an unresolved alias. Review its existing DNS record in Cloudflare.")
                    };
                }
                return HostnameDns::unknown("Cloudflare proxying may hide this hostname's DNS target. Verify it with your Cloudflare login.");
            };
            if cnames.next().is_some() {
                return HostnameDns::unknown(
                    "Could not verify DNS: the resolver returned multiple CNAME targets. Retry.",
                );
            }
            let target = cname.data.trim_end_matches('.').to_ascii_lowercase();
            if target == format!("{id}.cfargotunnel.com") {
                let mut result = HostnameDns::unknown("Public DNS points to this tunnel, but cannot confirm Cloudflare proxying. Verify it with your Cloudflare login.");
                result.actual_target = Some(target);
                result.observed_tunnel_id = Some(id.to_string());
                return result;
            }
            if target
                .strip_suffix(".cfargotunnel.com")
                .is_some_and(|target| {
                    uuid::Uuid::parse_str(target).is_ok_and(|target_id| {
                        !target_id.is_nil() && target_id.to_string() == target
                    })
                })
            {
                let mut result = HostnameDns::new(DnsStatus::Conflict, "This hostname points to another tunnel. Review its existing DNS record in Cloudflare before changing it.", false);
                result.reason = Some("wrong_tunnel".into());
                result.observed_tunnel_id = tunnel_target(&target);
                result.actual_target = Some(target);
                return result;
            }
            let mut result = HostnameDns::unknown("This hostname has a CNAME, but public DNS does not confirm which tunnel it reaches. Review its DNS record in Cloudflare.");
            result.actual_target = Some(target);
            result
        }
        _ => HostnameDns::unknown(
            "Could not verify DNS: the resolver could not answer this query. Retry.",
        ),
    }
}

async fn get(url: &str, token: Option<&Secret<String>>) -> anyhow::Result<Vec<u8>> {
    request(url, token, hyper::Method::GET, Vec::new()).await
}

async fn request(
    url: &str,
    token: Option<&Secret<String>>,
    method: hyper::Method,
    body: Vec<u8>,
) -> anyhow::Result<Vec<u8>> {
    let https = hyper_rustls::HttpsConnectorBuilder::new()
        .with_native_roots()
        .map_err(|_| anyhow::anyhow!("Could not load trusted HTTPS certificates."))?
        .https_only()
        .enable_http1()
        .build();
    let client = Client::builder(TokioExecutor::new()).build::<_, Full<Bytes>>(https);
    let mut request = Request::builder()
        .method(method)
        .uri(url)
        .header("content-type", "application/json")
        .header(
            "accept",
            if token.is_some() {
                "application/json"
            } else {
                "application/dns-json"
            },
        );
    if let Some(token) = token {
        let bearer = Secret::new(format!("Bearer {}", token.expose()));
        let mut authorization =
            hyper::header::HeaderValue::from_str(bearer.expose()).map_err(|_| {
                anyhow::anyhow!(
                    "Cloudflare DNS access is invalid. Reconnect your Cloudflare login or DNS access."
                )
            })?;
        authorization.set_sensitive(true);
        request = request.header(hyper::header::AUTHORIZATION, authorization);
    }
    let request = request
        .body(Full::new(Bytes::from(body)))
        .map_err(|_| anyhow::anyhow!("Could not build the DNS request."))?;
    let response = client.request(request).await.map_err(|_| {
        anyhow::anyhow!("Could not reach Cloudflare. Check your connection and retry.")
    })?;
    ensure!(!matches!(response.status().as_u16(), 401 | 403), "Cloudflare rejected DNS access. Reconnect your Cloudflare login or check DNS access for this domain.");
    ensure!(
        response.status().is_success(),
        "Cloudflare could not answer the DNS request. Retry."
    );
    let mut body = response.into_body();
    let mut bytes = Vec::new();
    while let Some(frame) = body.frame().await {
        if let Ok(data) = frame
            .map_err(|_| anyhow::anyhow!("Could not read Cloudflare's DNS response. Retry."))?
            .into_data()
        {
            ensure!(
                bytes.len() + data.len() <= 64 * 1024,
                "DNS resolver response is too large."
            );
            bytes.extend_from_slice(&data);
        }
    }
    Ok(bytes)
}

pub(crate) async fn bounded_get(
    url: &str,
    token: Option<&Secret<String>>,
) -> anyhow::Result<Vec<u8>> {
    tokio::time::timeout(Duration::from_secs(15), get(url, token))
        .await
        .map_err(|_| anyhow::anyhow!("Cloudflare DNS verification timed out. Retry."))?
}

pub(crate) async fn bounded_patch(
    url: &str,
    token: &Secret<String>,
    body: Vec<u8>,
) -> anyhow::Result<Vec<u8>> {
    tokio::time::timeout(
        Duration::from_secs(15),
        request(url, Some(token), hyper::Method::PATCH, body),
    )
    .await
    .map_err(|_| {
        anyhow::anyhow!("DNS migration timed out. Retry to verify whether Cloudflare accepted it.")
    })?
}

pub(crate) async fn bounded_post(
    url: &str,
    token: &Secret<String>,
    body: Vec<u8>,
) -> anyhow::Result<Vec<u8>> {
    tokio::time::timeout(
        Duration::from_secs(15),
        request(url, Some(token), hyper::Method::POST, body),
    )
    .await
    .map_err(|_| {
        anyhow::anyhow!("DNS creation timed out. Retry to verify whether Cloudflare accepted it.")
    })?
}

async fn lookup(hostname: &str) -> anyhow::Result<Vec<u8>> {
    // Validation excludes query delimiters; the only allowed reserved character is a wildcard.
    let name = hostname.replace('*', "%2A");
    get(
        &format!("https://cloudflare-dns.com/dns-query?name={name}&type=CNAME"),
        None,
    )
    .await
}

fn cloudflare_id(id: &str) -> anyhow::Result<&str> {
    ensure!(
        id.len() == 32 && id.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "The Cloudflare zone or account ID is invalid."
    );
    Ok(id)
}

#[derive(Deserialize)]
struct Envelope<T> {
    success: bool,
    result: T,
    #[serde(default)]
    result_info: Option<ResultInfo>,
}

#[derive(Deserialize)]
struct ResultInfo {
    page: Option<u32>,
    total_pages: Option<u32>,
    count: Option<usize>,
    total_count: Option<usize>,
}

fn envelope<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> anyhow::Result<Envelope<T>> {
    let payload: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|_| anyhow::anyhow!("Cloudflare returned an unexpected DNS response. Retry."))?;
    ensure!(payload.get("success").and_then(serde_json::Value::as_bool) != Some(false), "Cloudflare could not verify DNS. Check your Cloudflare login or DNS access for this domain.");
    let envelope: Envelope<T> = serde_json::from_slice(bytes)
        .map_err(|_| anyhow::anyhow!("Cloudflare returned an unexpected DNS response. Retry."))?;
    ensure!(envelope.success, "Cloudflare could not verify DNS. Check your Cloudflare login or DNS access for this domain.");
    Ok(envelope)
}

#[derive(Deserialize)]
struct Zone {
    name: String,
    account: Account,
}

#[derive(Deserialize)]
struct Account {
    id: String,
}

fn zone_response(bytes: &[u8], account_id: &str, hostname: &str) -> anyhow::Result<()> {
    let zone = envelope::<Zone>(bytes)?.result;
    ensure!(zone.account.id.eq_ignore_ascii_case(account_id), "This domain belongs to a different Cloudflare account than the tunnel. Use the matching Cloudflare login and DNS access.");
    let zone_name = canonical_hostname(&zone.name)?;
    let domain = hostname.strip_prefix("*.").unwrap_or(hostname);
    ensure!(
        domain == zone_name
            || domain
                .strip_suffix(&zone_name)
                .is_some_and(|prefix| prefix.ends_with('.')),
        "This hostname is outside the domain authorized by the Cloudflare login certificate."
    );
    Ok(())
}

/// Cloudflare DNS access verifies the certificate's zone before CLI writes.
pub async fn verify_zone(
    token: &Secret<String>,
    zone_id: &str,
    account_id: &str,
    hostname: &str,
) -> anyhow::Result<()> {
    let hostname = canonical_hostname(hostname)?;
    let zone_id = cloudflare_id(zone_id)?;
    cloudflare_id(account_id)?;
    ensure!(
        !token.expose().trim().is_empty(),
        "Connect your Cloudflare login or DNS access for this domain."
    );
    let bytes = bounded_get(
        &format!("https://api.cloudflare.com/client/v4/zones/{zone_id}"),
        Some(token),
    )
    .await?;
    zone_response(&bytes, account_id, &hostname)
}

#[derive(Deserialize)]
struct Record {
    #[serde(default)]
    id: Option<String>,
    name: String,
    #[serde(rename = "type")]
    kind: String,
    content: Option<String>,
    proxied: Option<bool>,
}

fn records(bytes: &[u8], hostname: &str) -> anyhow::Result<Vec<Record>> {
    let envelope = envelope::<Vec<Record>>(bytes)?;
    let records = envelope.result;
    if let Some(info) = envelope.result_info {
        ensure!(info.page.is_none_or(|page| page == 1)
            && info.total_pages.is_none_or(|pages| pages <= 1)
            && info.count.is_none_or(|count| count == records.len())
            && info.total_count.is_none_or(|count| count == records.len()), "Cloudflare returned incomplete DNS records. Retry verification before creating a record.");
    }
    ensure!(
        records.iter().all(|record| canonical_hostname(&record.name)
            .is_ok_and(|name| name == hostname)
            && !record.kind.is_empty()
            && (record.kind != "CNAME"
                || (record
                    .content
                    .as_ref()
                    .is_some_and(|content| !content.trim().is_empty())
                    && record.proxied.is_some()))),
        "Cloudflare returned unexpected DNS records. Retry verification before creating a record."
    );
    Ok(records)
}

fn tunnel_target(target: &str) -> Option<String> {
    let target = target.trim_end_matches('.').to_ascii_lowercase();
    canonical_tunnel_id(target.strip_suffix(".cfargotunnel.com")?).ok()
}

fn records_response(bytes: &[u8], hostname: &str, tunnel_id: &str) -> anyhow::Result<HostnameDns> {
    let records = records(bytes, hostname)?;
    let mut result = records_status(&records, tunnel_id);
    result.source = "cloudflare".into();
    result.expected_tunnel_id = Some(tunnel_id.into());
    if let [record] = records.as_slice() {
        if record.kind == "CNAME" {
            result.actual_target = record.content.clone();
            result.observed_tunnel_id = record.content.as_deref().and_then(tunnel_target);
        }
    }
    Ok(result)
}

fn records_status(records: &[Record], tunnel_id: &str) -> HostnameDns {
    if records.is_empty() {
        return HostnameDns::new(
            DnsStatus::Missing,
            "No DNS record exists for this hostname in Cloudflare.",
            true,
        );
    }
    let target = format!("{tunnel_id}.cfargotunnel.com");
    if records.len() == 1
        && records[0].kind == "CNAME"
        && records[0]
            .content
            .as_ref()
            .is_some_and(|content| content.trim_end_matches('.').eq_ignore_ascii_case(&target))
        && records[0].proxied == Some(true)
    {
        return HostnameDns::new(
            DnsStatus::Connected,
            "Cloudflare confirms this hostname is proxied to this tunnel.",
            false,
        );
    }
    if records.len() == 1
        && records[0].kind == "CNAME"
        && records[0]
            .content
            .as_ref()
            .is_some_and(|content| content.trim_end_matches('.').eq_ignore_ascii_case(&target))
    {
        let mut result = HostnameDns::new(DnsStatus::Conflict, "DNS points to this tunnel, but Cloudflare proxying is disabled. Enable proxying for this record in Cloudflare.", false);
        result.reason = Some("proxy_disabled".into());
        return result;
    }
    let mut result = HostnameDns::new(DnsStatus::Conflict, "This hostname has an existing DNS record that does not proxy to this tunnel. It was preserved; review the record in Cloudflare before changing it.", false);
    result.reason = Some("other_conflict".into());
    if let [record] = records {
        if record.kind == "CNAME"
            && record.proxied == Some(true)
            && record.content.as_deref().and_then(tunnel_target).is_some()
        {
            result.reason = Some("wrong_tunnel".into());
            result.message = "This hostname is proxied to another tunnel. Review the current and expected targets before repairing it.".into();
            result.can_repair = record
                .id
                .as_deref()
                .is_some_and(|id| cloudflare_id(id).is_ok());
        }
    }
    result
}

/// Authoritative Cloudflare DNS status, including CNAMEs hidden by proxying.
pub async fn check_records(
    token: &Secret<String>,
    zone_id: &str,
    hostname: &str,
    tunnel_id: &str,
) -> anyhow::Result<HostnameDns> {
    let (hostname, id) = inputs(hostname, tunnel_id)?;
    let zone_id = cloudflare_id(zone_id)?;
    ensure!(
        !token.expose().trim().is_empty(),
        "Connect your Cloudflare login or DNS access for this domain."
    );
    let name = hostname.replace('*', "%2A");
    let bytes = bounded_get(&format!("https://api.cloudflare.com/client/v4/zones/{zone_id}/dns_records?name.exact={name}&per_page=100"), Some(token)).await?;
    records_response(&bytes, &hostname, &id.to_string())
}

/// Backend-only snapshot of the exact singleton proxied tunnel record reviewed by the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DnsRepairRecord {
    pub hostname: String,
    pub record_id: String,
    pub target: String,
    pub tunnel_id: String,
}

fn repair_record(record: Record, hostname: &str) -> anyhow::Result<DnsRepairRecord> {
    let target = record
        .content
        .context("The DNS record has no target. Review it in Cloudflare.")?;
    let tunnel_id = tunnel_target(&target).context(
        "Only a proxied tunnel CNAME can be repaired. Review this record in Cloudflare.",
    )?;
    let record_id = record
        .id
        .context("Cloudflare did not identify this DNS record. Review DNS again.")?;
    cloudflare_id(&record_id)?;
    ensure!(
        canonical_hostname(&record.name)? == hostname
            && record.kind == "CNAME"
            && record.proxied == Some(true),
        "The DNS record changed or is not a proxied tunnel CNAME. Review DNS again."
    );
    Ok(DnsRepairRecord {
        hostname: hostname.into(),
        record_id,
        target,
        tunnel_id,
    })
}

fn repair_list(bytes: &[u8], hostname: &str) -> anyhow::Result<DnsRepairRecord> {
    let mut records = records(bytes, hostname)?;
    ensure!(records.len() == 1,
        "The reviewed DNS record is missing or there are multiple records. Nothing was changed; review DNS again.");
    repair_record(records.remove(0), hostname)
}

fn record_url(zone: &str, record: &DnsRepairRecord) -> anyhow::Result<String> {
    Ok(format!(
        "https://api.cloudflare.com/client/v4/zones/{}/dns_records/{}",
        cloudflare_id(zone)?,
        cloudflare_id(&record.record_id)?
    ))
}

fn records_url(zone: &str, hostname: &str) -> anyhow::Result<String> {
    Ok(format!(
        "https://api.cloudflare.com/client/v4/zones/{}/dns_records?name.exact={}&per_page=100",
        cloudflare_id(zone)?,
        canonical_hostname(hostname)?.replace('*', "%2A")
    ))
}

pub async fn review_repair(
    token: &Secret<String>,
    zone: &str,
    hostname: &str,
    expected: &str,
) -> anyhow::Result<DnsRepairRecord> {
    let hostname = canonical_hostname(hostname)?;
    let expected = canonical_tunnel_id(expected)?;
    let record = repair_list(
        &bounded_get(&records_url(zone, &hostname)?, Some(token)).await?,
        &hostname,
    )?;
    ensure!(
        record.tunnel_id != expected,
        "This hostname already points to this tunnel. Refresh DNS status."
    );
    Ok(record)
}

fn reviewed_or_repaired(
    current: &DnsRepairRecord,
    reviewed: &DnsRepairRecord,
    expected: &str,
) -> anyhow::Result<bool> {
    ensure!(
        current.hostname == reviewed.hostname && current.record_id == reviewed.record_id,
        "The reviewed DNS record was replaced. Nothing was changed; review DNS again."
    );
    if current.tunnel_id == expected {
        return Ok(true);
    }
    ensure!(
        current == reviewed,
        "The reviewed DNS record changed. Nothing was changed; review DNS again."
    );
    Ok(false)
}

async fn repair_with<F, Fut, G, GuardFuture, LocalGuard>(
    zone: &str,
    reviewed: &DnsRepairRecord,
    expected: &str,
    mut request: F,
    validate: G,
    validate_local: LocalGuard,
) -> anyhow::Result<HostnameDns>
where
    F: FnMut(hyper::Method, String, Vec<u8>) -> Fut,
    Fut: Future<Output = anyhow::Result<Vec<u8>>>,
    G: Fn() -> GuardFuture,
    GuardFuture: Future<Output = anyhow::Result<()>>,
    LocalGuard: Fn() -> anyhow::Result<()>,
{
    let expected = canonical_tunnel_id(expected)?;
    ensure!(
        canonical_tunnel_id(&reviewed.tunnel_id)? != expected,
        "The reviewed DNS target must differ from the expected tunnel."
    );
    validate().await?;
    let list_url = records_url(zone, &reviewed.hostname)?;
    let bytes = request(hyper::Method::GET, list_url.clone(), Vec::new()).await?;
    // Reuse migration validation, but missing records never select its create path.
    crate::separate_tunnel::plan_dns(
        &bytes,
        &reviewed.hostname,
        &reviewed.tunnel_id,
        Some(&expected),
    )?;
    let current = repair_list(&bytes, &reviewed.hostname)?;
    if reviewed_or_repaired(&current, reviewed, &expected)? {
        validate_local()?;
        return records_response(&bytes, &reviewed.hostname, &expected);
    }
    let url = record_url(zone, reviewed)?;
    let bytes = request(hyper::Method::GET, url.clone(), Vec::new()).await?;
    let current = repair_record(envelope::<Record>(&bytes)?.result, &reviewed.hostname)?;
    let already_repaired = reviewed_or_repaired(&current, reviewed, &expected)?;
    validate_local()?;
    let (update, invalid_reply) = if already_repaired {
        (Ok(()), false)
    } else {
        // shortcut: Cloudflare has no documented atomic conditional record update; an external
        // edit between this final read and PATCH remains an optimistic race.
        match request(
            hyper::Method::PATCH,
            url,
            serde_json::to_vec(
                &serde_json::json!({"content":format!("{expected}.cfargotunnel.com")}),
            )?,
        )
        .await
        {
            Ok(bytes) => (
                (|| {
                    let updated =
                        repair_record(envelope::<Record>(&bytes)?.result, &reviewed.hostname)?;
                    ensure!(reviewed_or_repaired(&updated, reviewed, &expected)?,
                    "Cloudflare did not confirm the repaired target. Review DNS before retrying.");
                    Ok(())
                })(),
                true,
            ),
            Err(error) => (Err(error), false),
        }
    };
    // Even a timeout can have committed. A fresh complete record set is required for success.
    let bytes = request(hyper::Method::GET, list_url, Vec::new()).await
        .map_err(|_| anyhow::anyhow!("Could not verify the DNS repair outcome. Cloudflare may have accepted it; retry this review to verify safely."))?;
    let current = repair_list(&bytes, &reviewed.hostname)?;
    if reviewed_or_repaired(&current, reviewed, &expected)? {
        if let Err(error) = update {
            if error.to_string().contains("rejected") || error.to_string().contains("DNS access") {
                return Err(error);
            }
            if invalid_reply {
                anyhow::bail!("Cloudflare returned an ambiguous repair response. The target now matches; refresh and review DNS before retrying.");
            }
        }
        validate_local()?;
        return records_response(&bytes, &reviewed.hostname, &expected);
    }
    update?;
    anyhow::bail!("Cloudflare did not retain the repaired target. Review DNS before retrying.")
}

/// Repair only the reviewed record ID. Callers bind account, config, run, and user confirmation.
pub async fn repair_reviewed<G, GuardFuture, LocalGuard>(
    token: &Secret<String>,
    zone: &str,
    reviewed: &DnsRepairRecord,
    expected: &str,
    validate: G,
    validate_local: LocalGuard,
) -> anyhow::Result<HostnameDns>
where
    G: Fn() -> GuardFuture,
    GuardFuture: Future<Output = anyhow::Result<()>>,
    LocalGuard: Fn() -> anyhow::Result<()>,
{
    repair_with(
        zone,
        reviewed,
        expected,
        |method, url, body| async move {
            if method == hyper::Method::GET {
                bounded_get(&url, Some(token)).await
            } else {
                bounded_patch(&url, token, body).await
            }
        },
        validate,
        validate_local,
    )
    .await
}

/// Public DNS cannot always reveal a Cloudflare-proxied CNAME; unknown is never treated as missing.
pub async fn check(hostname: &str, tunnel_id: &str) -> HostnameDns {
    let (hostname, id) = match inputs(hostname, tunnel_id) {
        Ok(input) => input,
        Err(error) => return HostnameDns::unknown(&error.to_string()),
    };
    match tokio::time::timeout(Duration::from_secs(15), lookup(&hostname)).await {
        Ok(Ok(bytes)) => response(&bytes, &hostname, id),
        _ => HostnameDns::unknown("Could not verify DNS. Check your connection and retry."),
    }
}

/// The caller verifies project membership/account access before this explicit remote mutation.
pub async fn create(
    binary: &Path,
    data_dir: &Path,
    cert: &Path,
    tunnel_id: &str,
    hostname: &str,
) -> anyhow::Result<HostnameDns> {
    let (hostname, id) = inputs(hostname, tunnel_id)?;
    let args: Vec<OsString> = vec![
        "--origincert".into(),
        cert.as_os_str().into(),
        "route".into(),
        "dns".into(),
        "--overwrite-dns=false".into(),
        "--".into(),
        id.to_string().into(),
        hostname.clone().into(),
    ];
    let mut invocation = Invocation::new(args, ConfigPolicy::Neutral(data_dir.into()));
    invocation.timeout = Duration::from_secs(30);
    invocation.stdout = StdoutMode::Secret;
    let output = cli::exec(binary, invocation).await.map_err(|error| {
        let message = match error.kind {
            cli::ErrorKind::DnsRecordExists => "A DNS record already exists for this hostname. It was preserved; review its DNS record in Cloudflare before changing it.",
            cli::ErrorKind::NotLoggedIn | cli::ErrorKind::Auth => "Cloudflare rejected DNS access. Check that the login certificate can manage this hostname's domain, then retry.",
            cli::ErrorKind::Timeout => "DNS creation timed out. Cloudflare may have accepted the record; retry to check it safely without overwriting existing records.",
            _ => "Could not create the DNS record. Existing records were preserved; check the hostname's DNS record and your Cloudflare account access, then retry.",
        };
        anyhow::anyhow!(message)
    })?;
    let stdout = match &output.stdout {
        cli::Stdout::Secret(bytes) => std::str::from_utf8(bytes.expose()).unwrap_or(""),
        _ => "",
    };
    ensure!(route_confirmed(stdout, &hostname) || route_confirmed(&output.stderr, &hostname), "Cloudflare did not confirm routing for this exact hostname. Verify DNS before retrying; any existing record was preserved.");
    let mut result = HostnameDns::new(DnsStatus::Connected, "Cloudflare confirmed this hostname is routed to this tunnel. DNS changes may take time to appear publicly.", false);
    result.source = "cloudflare".into();
    result.expected_tunnel_id = Some(id.to_string());
    result.observed_tunnel_id = Some(id.to_string());
    result.actual_target = Some(format!("{id}.cfargotunnel.com"));
    Ok(result)
}

fn route_confirmed(text: &str, hostname: &str) -> bool {
    text.lines().any(|line| {
        let json: Option<serde_json::Value> = serde_json::from_str(line).ok();
        let line = json
            .as_ref()
            .and_then(|value| value.get("message").and_then(serde_json::Value::as_str))
            .unwrap_or(line);
        let added = line
            .split_once("Added CNAME ")
            .and_then(|(_, rest)| rest.split_once(" which will route "))
            .map(|(name, _)| name);
        let existing = line
            .split_once(" is already configured to route to your tunnel")
            .and_then(|(prefix, _)| prefix.split_whitespace().last());
        added
            .or(existing)
            .is_some_and(|name| canonical_hostname(name).is_ok_and(|name| name == hostname))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "00000000-0000-4000-8000-000000000001";
    const ACCOUNT: &str = "00000000000000000000000000000001";
    const OTHER: &str = "00000000-0000-4000-8000-000000000002";

    fn record(target: &str) -> serde_json::Value {
        serde_json::json!({"id":ACCOUNT,"name":"app.example.com","type":"CNAME",
            "content":format!("{target}.cfargotunnel.com"),"proxied":true})
    }

    fn record_set(records: Vec<serde_json::Value>) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({"success":true,"result":records})).unwrap()
    }

    fn single_record(record: serde_json::Value) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({"success":true,"result":record})).unwrap()
    }

    fn reviewed() -> DnsRepairRecord {
        repair_list(&record_set(vec![record(OTHER)]), "app.example.com").unwrap()
    }

    #[test]
    fn mismatch_details_authorize_only_a_single_proxied_tunnel_record() {
        let mismatch =
            records_response(&record_set(vec![record(OTHER)]), "app.example.com", ID).unwrap();
        assert_eq!(mismatch.reason.as_deref(), Some("wrong_tunnel"));
        assert_eq!(mismatch.expected_tunnel_id.as_deref(), Some(ID));
        assert_eq!(mismatch.observed_tunnel_id.as_deref(), Some(OTHER));
        assert_eq!(
            mismatch.actual_target.as_deref(),
            Some(format!("{OTHER}.cfargotunnel.com").as_str())
        );
        assert!(mismatch.can_repair);
        assert_eq!(mismatch.source, "cloudflare");
        assert!(mismatch.checked_at > 0);
        for (mut entry, reason) in [
            (record(ID), "proxy_disabled"),
            (record(OTHER), "other_conflict"),
        ] {
            entry["proxied"] = false.into();
            let status = records_response(&record_set(vec![entry]), "app.example.com", ID).unwrap();
            assert_eq!(status.reason.as_deref(), Some(reason));
            assert!(!status.can_repair);
        }
        let mut unrelated = record(OTHER);
        unrelated["content"] = "elsewhere.example.com".into();
        let status =
            records_response(&record_set(vec![unrelated.clone()]), "app.example.com", ID).unwrap();
        assert_eq!(status.reason.as_deref(), Some("other_conflict"));
        assert!(!status.can_repair);
        assert!(repair_list(&record_set(vec![unrelated]), "app.example.com").is_err());
        assert!(repair_list(
            &record_set(vec![record(OTHER), record(OTHER)]),
            "app.example.com"
        )
        .is_err());
    }

    #[tokio::test]
    async fn reviewed_repair_never_writes_deleted_replaced_or_changed_records() {
        let mut replaced = record(OTHER);
        replaced["id"] = "00000000000000000000000000000002".into();
        let mut unproxied = record(OTHER);
        unproxied["proxied"] = false.into();
        let mut changed = record(OTHER);
        changed["content"] = "00000000-0000-4000-8000-000000000003.cfargotunnel.com".into();
        for entries in [
            vec![],
            vec![replaced.clone()],
            vec![unproxied.clone()],
            vec![changed.clone()],
            vec![record(OTHER), record(OTHER)],
        ] {
            let result = repair_with(
                ACCOUNT,
                &reviewed(),
                ID,
                |method, _, _| {
                    assert_eq!(method, hyper::Method::GET);
                    std::future::ready(Ok(record_set(entries.clone())))
                },
                || std::future::ready(Ok(())),
                || Ok(()),
            )
            .await;
            assert!(result.is_err());
        }
        for entry in [replaced, unproxied, changed] {
            let mut calls = 0;
            let result = repair_with(
                ACCOUNT,
                &reviewed(),
                ID,
                |method, _, _| {
                    assert_eq!(method, hyper::Method::GET);
                    calls += 1;
                    std::future::ready(Ok(if calls == 1 {
                        record_set(vec![record(OTHER)])
                    } else {
                        single_record(entry.clone())
                    }))
                },
                || std::future::ready(Ok(())),
                || Ok(()),
            )
            .await;
            assert!(result.is_err());
            assert_eq!(calls, 2);
        }
    }

    #[tokio::test]
    async fn repair_patches_only_content_and_requires_confirmed_full_readback() {
        let mut calls = 0;
        let status = repair_with(
            ACCOUNT,
            &reviewed(),
            ID,
            |method, url, body| {
                calls += 1;
                let response = match calls {
                    1 => {
                        assert_eq!(method, hyper::Method::GET);
                        record_set(vec![record(OTHER)])
                    }
                    2 => {
                        assert_eq!(method, hyper::Method::GET);
                        assert!(url.ends_with(ACCOUNT));
                        single_record(record(OTHER))
                    }
                    3 => {
                        assert_eq!(method, hyper::Method::PATCH);
                        assert!(url.ends_with(ACCOUNT));
                        assert_eq!(
                            serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
                            serde_json::json!({"content":format!("{ID}.cfargotunnel.com")})
                        );
                        single_record(record(ID))
                    }
                    4 => {
                        assert_eq!(method, hyper::Method::GET);
                        assert!(url.contains("name.exact="));
                        record_set(vec![record(ID)])
                    }
                    _ => panic!("unexpected DNS request"),
                };
                std::future::ready(Ok(response))
            },
            || std::future::ready(Ok(())),
            || Ok(()),
        )
        .await
        .unwrap();
        assert_eq!(calls, 4);
        assert_eq!(status.status, DnsStatus::Connected);
        assert!(!status.can_repair);
    }

    #[tokio::test]
    async fn idempotent_retry_requires_the_same_record_id_and_never_patches() {
        let mut calls = 0;
        let status = repair_with(
            ACCOUNT,
            &reviewed(),
            ID,
            |method, _, _| {
                calls += 1;
                assert_eq!(method, hyper::Method::GET);
                std::future::ready(Ok(record_set(vec![record(ID)])))
            },
            || std::future::ready(Ok(())),
            || Ok(()),
        )
        .await
        .unwrap();
        assert_eq!(calls, 1);
        assert_eq!(status.status, DnsStatus::Connected);
        let mut replaced = record(ID);
        replaced["id"] = "00000000000000000000000000000002".into();
        assert!(repair_with(
            ACCOUNT,
            &reviewed(),
            ID,
            |method, _, _| {
                assert_eq!(method, hyper::Method::GET);
                std::future::ready(Ok(record_set(vec![replaced.clone()])))
            },
            || std::future::ready(Ok(())),
            || Ok(())
        )
        .await
        .is_err());
    }

    #[tokio::test]
    async fn stale_backend_binding_blocks_patch_after_final_record_read() {
        let mut calls = 0;
        let result = repair_with(
            ACCOUNT,
            &reviewed(),
            ID,
            |method, _, _| {
                calls += 1;
                assert_eq!(method, hyper::Method::GET);
                std::future::ready(Ok(if calls == 1 {
                    record_set(vec![record(OTHER)])
                } else {
                    single_record(record(OTHER))
                }))
            },
            || std::future::ready(Ok(())),
            || anyhow::bail!("config or connector run changed"),
        )
        .await;
        assert!(result.unwrap_err().to_string().contains("run changed"));
        assert_eq!(calls, 2);
    }

    #[tokio::test]
    async fn unavailable_expected_tunnel_blocks_all_dns_requests() {
        let result = repair_with(
            ACCOUNT,
            &reviewed(),
            ID,
            |_, _, _| std::future::ready(Err(anyhow::anyhow!("Unexpected DNS request"))),
            || {
                std::future::ready(Err(anyhow::anyhow!(
                    "The tunnel is missing from this account"
                )))
            },
            || Ok(()),
        )
        .await;
        assert!(result.unwrap_err().to_string().contains("missing"));
    }

    #[tokio::test]
    async fn patch_timeout_is_reread_and_ambiguous_outcomes_never_invent_success() {
        for (mode, success) in [
            ("timeout-committed", true),
            ("timeout-old", false),
            ("malformed", false),
            ("readback-failed", false),
            ("added-record", false),
            ("denied", false),
        ] {
            let mut calls = 0;
            let result = repair_with(
                ACCOUNT,
                &reviewed(),
                ID,
                |method, _, _| {
                    calls += 1;
                    let response = match calls {
                        1 => Ok(record_set(vec![record(OTHER)])),
                        2 => Ok(single_record(record(OTHER))),
                        3 => {
                            assert_eq!(method, hyper::Method::PATCH);
                            match mode {
                                "timeout-committed" | "timeout-old" => {
                                    Err(anyhow::anyhow!("DNS repair timed out"))
                                }
                                "malformed" => Ok(b"{}".to_vec()),
                                "denied" => Err(anyhow::anyhow!("Cloudflare rejected DNS access")),
                                _ => Ok(single_record(record(ID))),
                            }
                        }
                        4 => {
                            assert_eq!(method, hyper::Method::GET);
                            match mode {
                                "readback-failed" => Err(anyhow::anyhow!("connection unavailable")),
                                "timeout-old" => Ok(record_set(vec![record(OTHER)])),
                                "added-record" => Ok(record_set(vec![record(ID), record(OTHER)])),
                                _ => Ok(record_set(vec![record(ID)])),
                            }
                        }
                        _ => panic!("unexpected DNS request"),
                    };
                    std::future::ready(response)
                },
                || std::future::ready(Ok(())),
                || Ok(()),
            )
            .await;
            assert_eq!(result.is_ok(), success, "{mode}: {result:?}");
            assert_eq!(calls, 4);
        }
    }

    #[test]
    fn zone_authorization_requires_hostname_boundary_and_same_account() {
        let bytes = serde_json::to_vec(&serde_json::json!({"success":true,"result":{"name":"example.com","account":{"id":ACCOUNT}}})).unwrap();
        for hostname in [
            "example.com",
            "app.example.com",
            "*.example.com",
            "*.app.example.com",
        ] {
            assert!(
                zone_response(&bytes, ACCOUNT, hostname).is_ok(),
                "{hostname}"
            );
        }
        for hostname in ["example.com.evil.com", "otherexample.com", "example.net"] {
            assert!(
                zone_response(&bytes, ACCOUNT, hostname).is_err(),
                "{hostname}"
            );
        }
        assert!(zone_response(
            &bytes,
            "00000000000000000000000000000002",
            "app.example.com"
        )
        .is_err());
        for id in ["", "../zone", "x".repeat(32).as_str(), "123"] {
            assert!(cloudflare_id(id).is_err());
        }
    }

    #[test]
    fn authoritative_records_distinguish_proxy_missing_and_existing_conflicts() {
        use serde_json::json;
        let parse = |records: serde_json::Value| {
            records_response(
                &serde_json::to_vec(&json!({"success":true,"result":records})).unwrap(),
                "app.example.com",
                ID,
            )
            .unwrap()
        };
        let connected = parse(
            json!([{"name":"APP.example.com.","type":"CNAME","content":format!("{ID}.cfargotunnel.com"),"proxied":true}]),
        );
        assert_eq!(connected.status, DnsStatus::Connected);
        assert!(!connected.can_create);
        let missing = parse(json!([]));
        assert_eq!(missing.status, DnsStatus::Missing);
        assert!(missing.can_create);
        for record in [
            json!({"name":"app.example.com","type":"CNAME","content":format!("{ID}.cfargotunnel.com"),"proxied":false}),
            json!({"name":"app.example.com","type":"CNAME","content":"other.example.com","proxied":true}),
            json!({"name":"app.example.com","type":"A","content":"192.0.2.1","proxied":true}),
            json!({"name":"app.example.com","type":"AAAA","content":"2001:db8::1","proxied":true}),
        ] {
            let conflict = parse(json!([record]));
            assert_eq!(conflict.status, DnsStatus::Conflict);
            assert!(!conflict.can_create);
        }
    }

    #[test]
    fn non_cname_records_without_content_or_proxy_fields_are_preserved_as_conflicts() {
        use serde_json::json;
        for record in [
            json!({"name":"app.example.com","type":"TXT","content":"verification"}),
            json!({"name":"app.example.com","type":"SRV","data":{"port":443,"priority":0,"weight":1,"target":"service.example.com"}}),
            json!({"name":"app.example.com","type":"HTTPS","data":{"priority":1,"target":".","value":"alpn=\"h2\""}}),
        ] {
            let result = records_response(
                &serde_json::to_vec(&json!({"success":true,"result":[record]})).unwrap(),
                "app.example.com",
                ID,
            )
            .unwrap();
            assert_eq!(result.status, DnsStatus::Conflict);
            assert!(!result.can_create);
            assert!(result.actual_target.is_none());
            assert!(!result.can_repair);
        }
    }

    #[test]
    fn authoritative_errors_malformed_records_and_pagination_are_rejected_safely() {
        use serde_json::json;
        for value in [
            json!({"success":false,"result":null,"errors":[{"message":"synthetic-api-secret"}]}),
            json!({"success":true}),
            json!({"success":"true","result":[]}),
            json!({"success":true,"result":[{"name":"app.example.com","type":"CNAME","content":"target"}]}),
            json!({"success":true,"result":[{"name":"app.example.com","type":"CNAME","proxied":true}]}),
            json!({"success":true,"result":[{"name":"app.example.com","type":"CNAME","content":" ","proxied":true}]}),
            json!({"success":true,"result":[{"name":"app.example.com","type":"CNAME","content":"target","proxied":null}]}),
            json!({"success":true,"result":[{"name":"other.example.com","type":"A","content":"192.0.2.1","proxied":true}]}),
            json!({"success":true,"result":[],"result_info":{"page":2,"total_pages":2}}),
            json!({"success":true,"result":[],"result_info":{"total_count":1}}),
        ] {
            let error =
                records_response(&serde_json::to_vec(&value).unwrap(), "app.example.com", ID)
                    .unwrap_err()
                    .to_string();
            assert!(!error.contains("synthetic-api-secret"));
        }
    }

    #[test]
    fn cli_summary_accepts_plain_or_json_logs_for_only_the_exact_hostname() {
        assert!(route_confirmed("2026-10-13 INF app.example.com is already configured to route to your tunnel tunnelID=test", "app.example.com"));
        assert!(route_confirmed(
            r#"{"level":"info","message":"app.example.com is already configured to route to your tunnel tunnelID=test"}"#,
            "app.example.com"
        ));
        assert!(route_confirmed(
            r#"{"level":"info","message":"Added CNAME app.example.com which will route to this tunnel"}"#,
            "app.example.com"
        ));
        assert!(!route_confirmed(
            "Added CNAME app.example.com.evil.com which will route to this tunnel",
            "app.example.com"
        ));
        assert!(!route_confirmed(
            "Failed to add route to app.example.com",
            "app.example.com"
        ));
    }

    fn parse(value: serde_json::Value) -> HostnameDns {
        response(
            &serde_json::to_vec(&value).unwrap(),
            "app.example.com",
            uuid::Uuid::parse_str(ID).unwrap(),
        )
    }

    #[test]
    fn distinguishes_missing_visible_targets_and_hidden_targets() {
        use serde_json::json;
        let missing = parse(json!({"Status":3}));
        assert_eq!(missing.status, DnsStatus::Missing);
        assert!(missing.can_create);
        let hidden = parse(
            json!({"Status":0,"Answer":[{"name":"app.example.com.","type":1,"data":"192.0.2.1"}]}),
        );
        assert!(hidden.actual_target.is_none());
        assert!(hidden.observed_tunnel_id.is_none());
        assert!(!hidden.can_repair);
        assert_eq!(parse(json!({"Status":0,"Answer":[{"name":"app.example.com.","type":5,"data":"00000000-0000-4000-8000-000000000002.cfargotunnel.com."}]})).status, DnsStatus::Conflict);
        for answer in [
            json!([]),
            json!([{"name":"app.example.com.","type":1,"data":"192.0.2.1"}]),
            json!([{"name":"app.example.com.","type":28,"data":"2001:db8::1"}]),
            json!([{"name":"app.example.com.","type":5,"data":"elsewhere.example.com."}]),
            json!([{"name":"other.example.com.","type":5,"data":format!("{ID}.cfargotunnel.com.")}]),
        ] {
            let result = parse(json!({"Status":0,"Answer":answer}));
            assert_eq!(result.status, DnsStatus::Unknown);
            assert!(!result.can_create);
            assert!(!result.can_repair);
            assert!(result.observed_tunnel_id.is_none());
            assert_eq!(result.source, "public");
            assert!(!result.message.contains("creating a record"));
        }
    }

    #[test]
    fn a_matching_public_cname_cannot_confirm_proxying() {
        use serde_json::json;
        for status in [0, 3] {
            let result = parse(
                json!({"Status":status,"Answer":[{"name":"APP.example.com.","type":5,"data":format!("{ID}.cfargotunnel.com.")}]}),
            );
            assert_eq!(result.status, DnsStatus::Unknown);
            assert!(!result.can_create);
            assert!(result
                .message
                .contains("cannot confirm Cloudflare proxying"));
            assert!(!result.can_repair);
            assert_eq!(result.source, "public");
        }
    }

    #[test]
    fn malformed_resolver_results_are_never_missing() {
        use serde_json::json;
        for value in [
            json!({}),
            json!({"Status":"3"}),
            json!({"Status":2}),
            json!({"Status":5}),
            json!({"Status":0,"Answer":[{"name":"app.example.com.","type":5}]}),
            json!({"Status":0,"Answer":[{"name":"app.example.com.","type":5,"data":format!("{ID}.cfargotunnel.com.")},{"name":"app.example.com.","type":5,"data":"elsewhere.example.com."}]}),
        ] {
            let result = parse(value);
            assert_eq!(result.status, DnsStatus::Unknown);
            assert!(!result.can_create);
        }
    }

    #[test]
    fn nxdomain_with_a_dangling_cname_does_not_offer_to_create_a_record() {
        use serde_json::json;
        let result = parse(
            json!({"Status":3,"Answer":[{"name":"app.example.com.","type":5,"data":"missing.example.com."}]}),
        );
        assert_eq!(result.status, DnsStatus::Unknown);
        assert!(!result.can_create);
        let truncated = parse(json!({"Status":3,"TC":true}));
        assert_eq!(truncated.status, DnsStatus::Unknown);
        assert!(!truncated.can_create);
    }

    #[test]
    fn validates_hostname_and_tunnel_before_network_or_cli() {
        assert_eq!(
            inputs("*.APP.example.com.", ID).unwrap().0,
            "*.app.example.com"
        );
        for hostname in [
            "",
            "localhost",
            "-app.example.com",
            "app-.example.com",
            "app..example.com",
            "app.example.com?type=A",
            "a/b.example.com",
            "a b.example.com",
            "é.example.com",
            "a*.example.com",
            "--overwrite-dns",
            "*.example.com..",
        ] {
            assert!(inputs(hostname, ID).is_err(), "{hostname}");
        }
        for id in [
            "name",
            "00000000-0000-0000-0000-000000000000",
            "00000000000040008000000000000001",
            "{00000000-0000-4000-8000-000000000001}",
        ] {
            assert!(inputs("app.example.com", id).is_err());
        }
        assert!(inputs(&format!("{}.example.com", "a".repeat(64)), ID).is_err());
    }
}
