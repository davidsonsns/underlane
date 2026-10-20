//! Shared hostname evidence. Connector readiness remains the supervisor's responsibility.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::Emitter;
use tokio::sync::{Notify, Semaphore};
use underlane_core::config::{resolve_path, ConfigFingerprint, TunnelConfig};
use underlane_core::connection::{self, PublicCheck, PublicStatus, ServiceCheck, ServiceStatus};
use underlane_core::dns::{DnsStatus, HostnameDns};
use underlane_core::model::{Tunnel, TunnelKind};
use underlane_core::proxy::now_ms;
use underlane_core::route_control::RouteKey;
use underlane_core::secret::redact_text;
use underlane_core::supervisor::{RouteMapping, Status, TunnelState};

use crate::state::Core;

const PUBLIC_INTERVAL: i64 = 60_000;
const DNS_INTERVAL: i64 = 300_000;
const PUBLIC_FRESHNESS: i64 = 90_000;

#[derive(Debug, Clone, Serialize)]
pub struct RouteConnection {
    pub rule_index: usize,
    pub hostname: String,
    pub service: String,
    pub path: Option<String>,
    pub dns: HostnameDns,
    pub local: ServiceCheck,
    pub public: PublicCheck,
    pub test_path: Option<String>,
    pub checking: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TunnelConnection {
    pub generation: u64,
    pub run: u64,
    pub config_revision: u64,
    pub expected_tunnel_id: Option<String>,
    pub config_changed: bool,
    pub checking: bool,
    pub routes: Vec<RouteConnection>,
    pub checked_at: Option<i64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
struct Binding {
    epoch: u64,
    run: u64,
    config_revision: u64,
    status: Status,
    supervised: bool,
    pid: Option<u32>,
    expected: Option<String>,
    source: Option<String>,
    fingerprint: Option<ConfigFingerprint>,
    auth: Vec<Option<(u64, u128)>>,
    mappings: Vec<RouteMapping>,
    paths: HashMap<usize, String>,
    kind: TunnelKind,
    config_changed: bool,
}

fn same_routes(previous: &[RouteMapping], current: &[RouteMapping]) -> bool {
    previous.len() == current.len()
        && previous.iter().zip(current).all(|(old, new)| {
            let mut new = new.clone();
            new.enabled = old.enabled;
            old == &new
        })
}

impl Binding {
    fn same_connector(&self, other: &Self) -> bool {
        let mut other = other.clone();
        let routes_match = same_routes(&self.mappings, &other.mappings);
        other.mappings.clone_from(&self.mappings);
        routes_match && self == &other
    }
}

fn update_gates(value: &mut TunnelConnection, previous: &Binding, current: &[RouteMapping]) {
    for route in &mut value.routes {
        let index = route.rule_index;
        if previous.mappings[index].enabled != current[index].enabled {
            route.public = public_pending(if current[index].enabled {
                "This hostname just started. Its public endpoint has not been checked yet."
            } else {
                "This hostname is stopped. Start it to verify its public endpoint."
            });
            route.checking = false;
        }
    }
}

struct Snapshot {
    id: String,
    binding: Binding,
    error: Option<String>,
}

struct Entry {
    binding: Binding,
    generation: u64,
    value: TunnelConnection,
    in_flight: bool,
    next_public: i64,
    next_dns: i64,
    dns_failures: u32,
    public_failures: u32,
    pending_routes: Vec<usize>,
}

struct Work {
    snapshot: Snapshot,
    generation: u64,
    value: TunnelConnection,
    check_dns: bool,
    targets: Vec<usize>,
    targeted: bool,
}

pub struct Connections {
    entries: Mutex<HashMap<String, Entry>>,
    generation: AtomicU64,
    slots: Arc<Semaphore>,
    local_slots: Arc<Semaphore>,
    changed: Notify,
}

impl Default for Connections {
    fn default() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            generation: AtomicU64::new(0),
            slots: Arc::new(Semaphore::new(4)),
            local_slots: Arc::new(Semaphore::new(4)),
            changed: Notify::new(),
        }
    }
}

impl Connections {
    pub fn wake(&self) {
        self.changed.notify_one();
    }

    pub fn invalidate(&self, id: &str) {
        self.entries.lock().unwrap().remove(id);
        self.wake();
    }

    pub fn hostname_changed(&self, id: &str, key: &RouteKey) {
        if let Some(entry) = self.entries.lock().unwrap().get_mut(id) {
            entry.generation = self.generation.fetch_add(1, Ordering::Relaxed) + 1;
            entry.value.generation = entry.generation;
            entry.in_flight = false;
            for route in &mut entry.value.routes {
                let changed = route.hostname == key.hostname
                    && route.path == key.path
                    && route.service == key.service;
                if changed {
                    route.public =
                        public_pending("This hostname changed. Check its public endpoint again.");
                }
                if (changed || route.checking) && !entry.pending_routes.contains(&route.rule_index)
                {
                    entry.pending_routes.push(route.rule_index);
                }
            }
        }
        self.wake();
    }

    pub fn clear(&self) {
        self.entries.lock().unwrap().clear();
        self.wake();
    }

    pub fn view(&self, id: &str, state: &TunnelState) -> Option<TunnelConnection> {
        let entries = self.entries.lock().unwrap();
        let entry = entries.get(id)?;
        if entry.binding.run != state.run
            || entry.binding.config_revision != state.config_revision
            || entry.binding.status != state.status
            || entry.binding.supervised != state.supervised
            || (entry.binding.kind == TunnelKind::Config
                && !same_routes(&entry.binding.mappings, &state.mappings))
        {
            return None;
        }
        let mut value = entry.value.clone();
        if entry.binding.kind == TunnelKind::Config {
            update_gates(&mut value, &entry.binding, &state.mappings);
        }
        expire(&mut value, now_ms());
        Some(value)
    }

    fn current(&self, id: &str, generation: u64) -> bool {
        self.entries
            .lock()
            .unwrap()
            .get(id)
            .is_some_and(|entry| entry.generation == generation)
    }

    fn reserve(
        &self,
        snapshot: Snapshot,
        force: bool,
        requested: Option<&[usize]>,
        now: i64,
    ) -> Option<Work> {
        let mut entries = self.entries.lock().unwrap();
        let previous = entries.get(&snapshot.id);
        let same = previous.is_some_and(|entry| entry.binding == snapshot.binding);
        let reusable =
            previous.is_some_and(|entry| entry.binding.same_connector(&snapshot.binding));
        let gates_changed = reusable && !same;
        let pending = previous.is_some_and(|entry| !entry.pending_routes.is_empty());
        if same && !force {
            let previous = previous.unwrap();
            if previous.in_flight
                || !snapshot.binding.status.is_active()
                || (!pending && now < previous.next_public.min(previous.next_dns))
            {
                return None;
            }
        }
        let due =
            !reusable || previous.is_some_and(|entry| now >= entry.next_public.min(entry.next_dns));
        let targeted = requested.is_some()
            || ((gates_changed || pending) && !due && !previous.unwrap().in_flight);
        let mut value = if reusable {
            previous.unwrap().value.clone()
        } else {
            initial(&snapshot)
        };
        let targets = if (gates_changed || pending) && targeted && requested.is_none() {
            selected_routes(&snapshot.binding, None)
                .into_iter()
                .filter(|index| {
                    !previous.unwrap().binding.mappings[*index].enabled
                        || previous.unwrap().pending_routes.contains(index)
                })
                .collect()
        } else {
            selected_routes(&snapshot.binding, requested)
        };
        if gates_changed {
            update_gates(
                &mut value,
                &previous.unwrap().binding,
                &snapshot.binding.mappings,
            );
        }
        let check_dns = force
            || !reusable
            || now >= previous.unwrap().next_dns
            || value
                .routes
                .iter()
                .any(|route| targets.contains(&route.rule_index) && route.dns.checked_at == 0);
        value.checking = !targets.is_empty();
        for route in &mut value.routes {
            route.checking = targets.contains(&route.rule_index);
        }
        let generation = self.generation.fetch_add(1, Ordering::Relaxed) + 1;
        value.generation = generation;
        let entry =
            Entry {
                binding: snapshot.binding.clone(),
                generation,
                value: value.clone(),
                in_flight: true,
                next_public: previous
                    .filter(|_| reusable)
                    .map(|e| e.next_public)
                    .unwrap_or(0),
                next_dns: previous
                    .filter(|_| reusable)
                    .map(|e| e.next_dns)
                    .unwrap_or(0),
                dns_failures: previous
                    .filter(|_| reusable)
                    .map(|e| e.dns_failures)
                    .unwrap_or(0),
                public_failures: previous
                    .filter(|_| reusable)
                    .map(|e| e.public_failures)
                    .unwrap_or(0),
                pending_routes: previous
                    .filter(|_| reusable)
                    .map(|entry| {
                        selected_routes(&snapshot.binding, None)
                            .into_iter()
                            .filter(|index| {
                                !targets.contains(index)
                                    && (entry.pending_routes.contains(index)
                                        || !entry.binding.mappings[*index].enabled
                                        || entry.value.routes.iter().any(|route| {
                                            route.rule_index == *index && route.checking
                                        }))
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            };
        entries.insert(snapshot.id.clone(), entry);
        Some(Work {
            snapshot,
            generation,
            value,
            check_dns,
            targets,
            targeted,
        })
    }

    fn complete(&self, work: &Work, value: TunnelConnection, now: i64) -> bool {
        let mut entries = self.entries.lock().unwrap();
        let Some(entry) = entries.get_mut(&work.snapshot.id).filter(|entry| {
            entry.generation == work.generation && entry.binding == work.snapshot.binding
        }) else {
            return false;
        };
        if work.check_dns && !work.targeted {
            let failed = value
                .routes
                .iter()
                .filter(|route| work.targets.contains(&route.rule_index))
                .any(|route| route.dns.status == DnsStatus::Unknown);
            entry.dns_failures = if failed {
                entry.dns_failures.saturating_add(1)
            } else {
                0
            };
            entry.next_dns = now
                + if failed {
                    backoff(30_000, entry.dns_failures)
                } else {
                    DNS_INTERVAL
                };
        }
        let failed = value
            .routes
            .iter()
            .filter(|route| work.targets.contains(&route.rule_index))
            .any(|route| {
                matches!(
                    route.public.status,
                    PublicStatus::TransportError
                        | PublicStatus::CloudflareError
                        | PublicStatus::HttpError
                        | PublicStatus::OriginError
                )
            });
        if !work.targeted {
            entry.public_failures = if failed {
                entry.public_failures.saturating_add(1)
            } else {
                0
            };
            entry.next_public = now + backoff(PUBLIC_INTERVAL, entry.public_failures.max(1));
        }
        entry.in_flight = false;
        entry.value = value;
        if !entry.pending_routes.is_empty() {
            self.wake();
        }
        true
    }

    fn progress(&self, work: &Work) -> bool {
        let mut entries = self.entries.lock().unwrap();
        let Some(entry) = entries.get_mut(&work.snapshot.id).filter(|entry| {
            entry.generation == work.generation && entry.binding == work.snapshot.binding
        }) else {
            return false;
        };
        entry.value = work.value.clone();
        true
    }
}

fn selected_routes(binding: &Binding, requested: Option<&[usize]>) -> Vec<usize> {
    binding
        .mappings
        .iter()
        .enumerate()
        .filter_map(|(index, mapping)| {
            let selected = requested.map_or_else(
                || active_route(binding, index),
                |indices| indices.contains(&index),
            );
            (selected && mapping.hostname.is_some()).then_some(index)
        })
        .collect()
}

fn active_route(binding: &Binding, index: usize) -> bool {
    binding.status.is_active()
        && binding
            .mappings
            .get(index)
            .is_some_and(|mapping| mapping.enabled)
}

fn backoff(base: i64, failures: u32) -> i64 {
    base.saturating_mul(1_i64 << failures.saturating_sub(1).min(4))
        .min(DNS_INTERVAL)
}

fn expire(value: &mut TunnelConnection, now: i64) {
    for route in &mut value.routes {
        if route.public.status == PublicStatus::Verified
            && now - route.public.checked_at > PUBLIC_FRESHNESS
        {
            route.public.status = PublicStatus::NotChecked;
            route.public.message =
                "The last public check is out of date. Check the connection again.".into();
        }
    }
}

fn unverified(message: &str, expected: Option<&str>) -> HostnameDns {
    let mut result = HostnameDns::unknown(message);
    result.expected_tunnel_id = expected.map(str::to_owned);
    result
}

fn public_pending(message: &str) -> PublicCheck {
    PublicCheck {
        status: PublicStatus::NotChecked,
        message: message.into(),
        checked_at: now_ms(),
        http_status: None,
        cloudflare_error: None,
        test_url: None,
    }
}

fn initial(snapshot: &Snapshot) -> TunnelConnection {
    let b = &snapshot.binding;
    let routes = b
        .mappings
        .iter()
        .enumerate()
        .filter_map(|(index, route)| {
            let hostname = route.hostname.as_ref()?;
            Some(RouteConnection {
                rule_index: index,
                hostname: hostname.clone(),
                service: route.service.clone(),
                path: route.path.clone(),
                dns: unverified("DNS has not been checked yet.", b.expected.as_deref()),
                local: ServiceCheck {
                    status: ServiceStatus::Unsupported,
                    message: "The local service has not been checked yet.".into(),
                    checked_at: 0,
                },
                public: public_pending("The public endpoint has not been checked yet."),
                test_path: b.paths.get(&index).cloned(),
                checking: true,
            })
        })
        .collect();
    TunnelConnection {
        generation: 0,
        run: b.run,
        config_revision: b.config_revision,
        expected_tunnel_id: b.expected.clone(),
        config_changed: b.config_changed,
        checking: true,
        routes,
        checked_at: None,
        error: snapshot.error.clone(),
    }
}

#[derive(Deserialize, Serialize)]
struct SavedPath {
    hostname: String,
    service: String,
    path: Option<String>,
    test_path: String,
}

fn path_key(id: &str, index: usize) -> String {
    format!("connection-test-path:{id}:{index}")
}

fn stamp(path: &Path) -> Option<(u64, u128)> {
    let meta = std::fs::metadata(path).ok()?;
    Some((
        meta.len(),
        meta.modified()
            .ok()?
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_nanos(),
    ))
}

fn snapshot(core: &Core, id: &str) -> Result<Snapshot, String> {
    let tunnel = core
        .db
        .tunnel(id)
        .map_err(|e| redact_text(&e.to_string()))?
        .ok_or("Tunnel not found")?;
    let (mut state, _) = core.state_with_routes(&tunnel);
    if let Some(live) = core.sup.config_state(id) {
        let mappings = state.mappings;
        state = live;
        if state.mappings.is_empty() {
            state.mappings = mappings;
        }
    }
    let mut fingerprint = None;
    let mut auth = Vec::new();
    let mut config_changed = false;
    let mut error = None;
    let mut configured_id = None;
    if tunnel.kind == TunnelKind::Config {
        if let Some(path) = tunnel.config_path.as_deref() {
            match TunnelConfig::load(Path::new(path)) {
                Ok(config) => {
                    fingerprint = Some(config.fingerprint);
                    configured_id = underlane_core::supervisor::config_tunnel_identity(&config)
                        .ok()
                        .flatten();
                    config_changed = state
                        .loaded_config
                        .as_ref()
                        .is_some_and(|loaded| loaded.fingerprint != config.fingerprint);
                    let cert = config
                        .raw
                        .as_mapping()
                        .and_then(|map| map.get("origincert"))
                        .and_then(|v| v.as_str())
                        .map(|p| resolve_path(config.source.parent().unwrap_or(Path::new(".")), p))
                        .unwrap_or_else(|| {
                            underlane_core::config::home_dir().join(".cloudflared/cert.pem")
                        });
                    auth.push(stamp(&cert));
                    auth.push(config.credentials_file.as_deref().and_then(stamp));
                }
                Err(_) => {
                    config_changed = state.loaded_config.is_some();
                    error = Some("Could not read the project config. Fix it before verifying the connection.".into());
                }
            }
        }
    }
    let expected = if state.supervised {
        state
            .cloudflare_tunnel_id
            .clone()
            .or_else(|| configured_id.clone())
    } else {
        configured_id.clone()
    };
    config_changed |= state.supervised
        && expected.is_some()
        && configured_id.is_some()
        && configured_id != expected;
    if config_changed {
        error = Some("The config changed after this connector started. Restart the tunnel to verify the updated routes.".into());
    }
    let mut mappings = state.mappings;
    if tunnel.kind == TunnelKind::Quick {
        if mappings.is_empty() {
            if let Some(service) = &tunnel.origin_url {
                mappings.push(RouteMapping {
                    hostname: None,
                    path: None,
                    service: service.clone(),
                    inspected: false,
                    note: None,
                    route_key: None,
                    control_supported: false,
                    enabled: true,
                });
            }
        }
        if let Some(hostname) = state
            .public_url
            .as_deref()
            .and_then(|url| url.strip_prefix("https://"))
        {
            if let Some(route) = mappings.first_mut() {
                route.hostname = Some(hostname.into());
            }
        }
    }
    let mut paths = HashMap::new();
    for (index, route) in mappings.iter().enumerate() {
        if let Some(saved) = core
            .db
            .kv_get(&path_key(id, index))
            .ok()
            .flatten()
            .and_then(|json| serde_json::from_str::<SavedPath>(&json).ok())
        {
            if route.hostname.as_deref() == Some(&saved.hostname)
                && route.service == saved.service
                && route.path == saved.path
                && connection::test_path(&mappings, index, Some(&saved.test_path)).is_ok()
            {
                paths.insert(index, saved.test_path);
            }
        }
    }
    Ok(Snapshot {
        id: id.into(),
        binding: Binding {
            epoch: *core.data_epoch.read().unwrap(),
            run: state.run,
            config_revision: state.config_revision,
            status: state.status,
            supervised: state.supervised,
            pid: state.pid,
            expected,
            source: tunnel.config_path,
            fingerprint,
            auth,
            mappings,
            paths,
            kind: tunnel.kind,
            config_changed,
        },
        error,
    })
}

fn emit(core: &Core, id: &str, value: &TunnelConnection) {
    let _ = core.handle.emit(
        "connection-changed",
        serde_json::json!({ "id": id, "connection": value }),
    );
}

fn qualify_public(
    binding: &Binding,
    dns: &HostnameDns,
    local: &ServiceCheck,
    public: &mut PublicCheck,
) {
    if public.status != PublicStatus::Verified {
        return;
    }
    let reason = if binding.config_changed {
        Some("The config has changed; restart before verifying this route.")
    } else if binding.status != Status::Connected {
        Some("The public endpoint responded, but this connector is not connected.")
    } else if local.status == ServiceStatus::Unreachable {
        Some("The public endpoint responded, but the configured local service is unreachable.")
    } else if binding.kind == TunnelKind::Config
        && (dns.status != DnsStatus::Connected || dns.source != "cloudflare")
    {
        Some("The public endpoint responded, but DNS has not been verified for this tunnel.")
    } else if binding.kind == TunnelKind::Token {
        Some("The public endpoint responded. This remotely managed tunnel's DNS target is not verified.")
    } else {
        None
    };
    if let Some(reason) = reason {
        public.status = PublicStatus::Responded;
        public.message = reason.into();
    }
}

async fn perform(core: Arc<Core>, mut work: Work) -> Result<TunnelConnection, String> {
    let id = work.snapshot.id.clone();
    let cache = &core.connections;
    // Publish local reachability before waiting on DNS and public internet checks.
    let mut local_checks = tokio::task::JoinSet::new();
    for route in &work.value.routes {
        if !work.targets.contains(&route.rule_index)
            || !active_route(&work.snapshot.binding, route.rule_index)
        {
            continue;
        }
        let core = core.clone();
        let id = id.clone();
        let generation = work.generation;
        let index = route.rule_index;
        let service = route.service.clone();
        local_checks.spawn(async move {
            let _permit = core
                .connections
                .local_slots
                .clone()
                .acquire_owned()
                .await
                .ok()?;
            if !core.connections.current(&id, generation) {
                return None;
            }
            Some((index, connection::probe_service(&service).await))
        });
    }
    while let Some(result) = local_checks.join_next().await {
        if let Ok(Some((index, local))) = result {
            if let Some(route) = work
                .value
                .routes
                .iter_mut()
                .find(|route| route.rule_index == index)
            {
                route.local = local;
                qualify_public(
                    &work.snapshot.binding,
                    &route.dns,
                    &route.local,
                    &mut route.public,
                );
            }
        }
    }
    if snapshot(&core, &id)?.binding != work.snapshot.binding || !cache.progress(&work) {
        cache.wake();
        return Err("The connection changed while checking. Check again.".into());
    }
    emit(&core, &id, &work.value);
    let mut dns_results = HashMap::new();
    if work.check_dns {
        let mut tasks = tokio::task::JoinSet::new();
        for hostname in work
            .value
            .routes
            .iter()
            .filter(|route| work.targets.contains(&route.rule_index))
            .map(|route| route.hostname.clone())
            .collect::<std::collections::HashSet<_>>()
        {
            let core = core.clone();
            let expected = work.snapshot.binding.expected.clone();
            let kind = work.snapshot.binding.kind;
            let id = id.clone();
            let generation = work.generation;
            tasks.spawn(async move {
                let _permit = core.connections.slots.clone().acquire_owned().await.ok()?;
                if !core.connections.current(&id, generation) {
                    return None;
                }
                let result = if kind != TunnelKind::Config {
                    unverified(
                        "DNS target verification is available for locally managed config tunnels.",
                        expected.as_deref(),
                    )
                } else if let Some(expected) = expected {
                    match tokio::time::timeout(
                        Duration::from_secs(15),
                        crate::dns::check_expected(
                            core.clone(),
                            id,
                            hostname.clone(),
                            expected.clone(),
                        ),
                    )
                    .await
                    {
                        Ok(Ok(result)) => result,
                        Ok(Err(message)) => unverified(&message, Some(&expected)),
                        Err(_) => unverified(
                            "DNS verification timed out. Check your connection and retry.",
                            Some(&expected),
                        ),
                    }
                } else {
                    unverified(
                        "A resolved tunnel identity is required to verify DNS.",
                        None,
                    )
                };
                Some((hostname, result))
            });
        }
        while let Some(result) = tasks.join_next().await {
            if let Ok(Some((hostname, result))) = result {
                dns_results.insert(hostname, result);
            }
        }
    }
    if !cache.current(&id, work.generation) {
        return Err("The connection changed while checking. Check again.".into());
    }
    for route in &mut work.value.routes {
        if let Some(dns) = dns_results
            .get(&route.hostname)
            .filter(|_| work.targets.contains(&route.rule_index))
        {
            route.dns = dns.clone();
        }
    }
    let mut tasks = tokio::task::JoinSet::new();
    let mut routes = Vec::new();
    for mut route in work.value.routes.clone() {
        if !work.targets.contains(&route.rule_index) {
            routes.push(route);
            continue;
        }
        let core = core.clone();
        let id = id.clone();
        let binding = work.snapshot.binding.clone();
        let generation = work.generation;
        tasks.spawn(async move {
            let _permit = core.connections.slots.clone().acquire_owned().await.ok()?;
            if !core.connections.current(&id, generation) {
                return None;
            }
            if !active_route(&binding, route.rule_index) {
                route.public = public_pending(
                    "This hostname is stopped. Start it to verify its public endpoint.",
                );
                route.checking = false;
                return Some(route);
            }
            route.public = if binding.config_changed {
                public_pending("Restart the tunnel to verify the changed configuration.")
            } else if binding.status != Status::Connected {
                public_pending(match binding.status {
                    Status::Stopped => "Start the tunnel to verify the public endpoint.",
                    Status::Crashed => {
                        "The connector exited. Start it again to verify the public endpoint."
                    }
                    Status::Starting => "Waiting for the connector to connect to Cloudflare.",
                    _ => "The connector is reconnecting. Public verification is incomplete.",
                })
            } else {
                match connection::test_path(
                    &binding.mappings,
                    route.rule_index,
                    route.test_path.as_deref(),
                ) {
                    Ok(path) => connection::probe_public(&route.hostname, &path).await,
                    Err(message) => PublicCheck {
                        status: PublicStatus::Unsupported,
                        message,
                        ..public_pending("Public verification is not available for this route.")
                    },
                }
            };
            qualify_public(&binding, &route.dns, &route.local, &mut route.public);
            route.checking = false;
            Some(route)
        });
    }
    while let Some(result) = tasks.join_next().await {
        if let Ok(Some(route)) = result {
            routes.push(route);
        }
    }
    routes.sort_by_key(|route| route.rule_index);
    if routes.len() != work.value.routes.len() {
        return Err("The connection changed while checking. Check again.".into());
    }
    work.value.routes = routes;
    work.value.checking = false;
    work.value.checked_at = Some(now_ms());
    let current = snapshot(&core, &id)?;
    if current.binding != work.snapshot.binding
        || !cache.complete(&work, work.value.clone(), now_ms())
    {
        cache.wake();
        return Err("The connection changed while checking. Check again.".into());
    }
    emit(&core, &id, &work.value);
    Ok(work.value)
}

pub async fn check(
    core: Arc<Core>,
    id: String,
    hostname: Option<String>,
    rule_index: Option<usize>,
    test_path: Option<String>,
) -> Result<TunnelConnection, String> {
    let mut selected = snapshot(&core, &id)?;
    if let Some(path) = test_path {
        let _operation = core
            .operations
            .try_read()
            .map_err(|_| "App reset is in progress.")?;
        let index = rule_index.ok_or("Choose a hostname rule before setting its test path.")?;
        let route = selected
            .binding
            .mappings
            .get(index)
            .ok_or("This hostname rule changed. Refresh and try again.")?;
        if hostname.as_deref() != route.hostname.as_deref() {
            return Err("This hostname rule changed. Refresh and try again.".into());
        }
        let path = connection::test_path(&selected.binding.mappings, index, Some(&path))?;
        let saved = SavedPath {
            hostname: hostname.clone().ok_or("A hostname is required.")?,
            service: route.service.clone(),
            path: route.path.clone(),
            test_path: path,
        };
        core.db
            .kv_set(
                &path_key(&id, index),
                &serde_json::to_string(&saved).map_err(|_| "Could not save the test path.")?,
            )
            .map_err(|_| "Could not save the test path.")?;
        selected = snapshot(&core, &id)?;
    } else if let Some(hostname) = hostname.as_ref() {
        if !selected
            .binding
            .mappings
            .iter()
            .enumerate()
            .any(|(i, route)| {
                route.hostname.as_deref() == Some(hostname)
                    && rule_index.is_none_or(|index| i == index)
            })
        {
            return Err("This hostname rule changed. Refresh and try again.".into());
        }
    }
    let requested = hostname.as_ref().map(|hostname| {
        selected
            .binding
            .mappings
            .iter()
            .enumerate()
            .filter_map(|(index, route)| {
                (route.hostname.as_ref() == Some(hostname)
                    && rule_index.is_none_or(|requested| index == requested))
                .then_some(index)
            })
            .collect::<Vec<_>>()
    });
    let work = core
        .connections
        .reserve(selected, true, requested.as_deref(), now_ms())
        .expect("explicit checks always reserve a generation");
    emit(&core, &id, &work.value);
    perform(core, work).await
}

pub async fn monitor(core: Arc<Core>) {
    let mut tick = tokio::time::interval(Duration::from_secs(2));
    loop {
        tokio::select! { _ = tick.tick() => {}, _ = core.connections.changed.notified() => {} }
        if core.operations.try_read().is_err() {
            continue;
        }
        let Ok(tunnels) = core.db.tunnels() else {
            continue;
        };
        core.connections
            .entries
            .lock()
            .unwrap()
            .retain(|id, _| tunnels.iter().any(|tunnel| &tunnel.id == id));
        for Tunnel { id, .. } in tunnels {
            let Ok(selected) = snapshot(&core, &id) else {
                continue;
            };
            let Some(work) = core.connections.reserve(selected, false, None, now_ms()) else {
                continue;
            };
            emit(&core, &id, &work.value);
            let core = core.clone();
            tokio::spawn(async move {
                let _ = perform(core, work).await;
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Snapshot {
        Snapshot {
            id: "project".into(),
            error: None,
            binding: Binding {
                epoch: 0,
                run: 1,
                config_revision: 1,
                status: Status::Connected,
                supervised: true,
                pid: Some(1),
                expected: Some("ed7e97e9-837a-43ae-b2e5-d0233ab88fbe".into()),
                source: Some("synthetic.yml".into()),
                fingerprint: Some(ConfigFingerprint::of(b"source")),
                auth: vec![],
                paths: HashMap::new(),
                kind: TunnelKind::Config,
                config_changed: false,
                mappings: vec![RouteMapping {
                    hostname: Some("app.example.com".into()),
                    service: "http://localhost:8787".into(),
                    path: None,
                    inspected: false,
                    note: None,
                    route_key: None,
                    control_supported: false,
                    enabled: true,
                }],
            },
        }
    }

    fn hostname_fixture() -> Snapshot {
        let mut snapshot = fixture();
        let route = snapshot.binding.mappings[0].clone();
        snapshot.binding.mappings = ["app", "api", "docs"]
            .into_iter()
            .enumerate()
            .map(|(index, name)| RouteMapping {
                hostname: Some(format!("{name}.example.com")),
                enabled: index == 0,
                ..route.clone()
            })
            .collect();
        snapshot
    }

    #[test]
    fn view_rejects_evidence_for_changed_route_identity_or_count() {
        let cache = Connections::default();
        let snapshot = hostname_fixture();
        let mut state = TunnelState::stopped("project");
        state.run = snapshot.binding.run;
        state.config_revision = snapshot.binding.config_revision;
        state.status = snapshot.binding.status;
        state.supervised = snapshot.binding.supervised;
        state.mappings = snapshot.binding.mappings.clone();
        cache.reserve(snapshot, false, None, 1).unwrap();
        assert!(cache.view("project", &state).is_some());
        for change in 0..5 {
            let mut changed = state.clone();
            match change {
                0 => changed.mappings[0].hostname = Some("changed.example.com".into()),
                1 => changed.mappings[0].service = "http://localhost:9999".into(),
                2 => changed.mappings[0].path = Some("/new".into()),
                3 => {
                    changed.mappings.pop();
                }
                _ => changed.mappings.push(changed.mappings[0].clone()),
            }
            assert!(cache.view("project", &changed).is_none(), "change {change}");
        }
    }

    fn checking_indices(work: &Work) -> Vec<usize> {
        work.value
            .routes
            .iter()
            .filter(|route| route.checking)
            .map(|route| route.rule_index)
            .collect()
    }

    #[test]
    fn toggling_one_hostname_preserves_sibling_health_and_checks_only_the_started_route() {
        let cache = Connections::default();
        let mut running = hostname_fixture();
        for mapping in &mut running.binding.mappings {
            mapping.enabled = true;
        }
        let first = cache.reserve(running, false, None, 1).unwrap();
        let mut evidence = first.value.clone();
        evidence.checking = false;
        for route in &mut evidence.routes {
            route.checking = false;
            route.local.status = ServiceStatus::Unreachable;
            route.local.checked_at = 42;
            route.dns.status = DnsStatus::Connected;
            route.dns.checked_at = 42;
            route.public.status = PublicStatus::OriginError;
        }
        assert!(cache.complete(&first, evidence, 2));

        let mut paused = hostname_fixture();
        paused.binding.mappings[2].enabled = true;
        let stopped = cache.reserve(paused, false, None, 3).unwrap();
        assert_eq!(
            stopped.value.routes[0].local.status,
            ServiceStatus::Unreachable,
            "stopping api must not turn app's known red health dot into an unknown hollow dot"
        );
        assert_eq!(stopped.value.routes[2].local.checked_at, 42);
        assert_eq!(
            stopped.value.routes[0].public.status,
            PublicStatus::OriginError
        );
        assert!(
            stopped.targets.is_empty(),
            "stopping a route needs no network checks"
        );
        assert!(
            !stopped.check_dns,
            "a gate change must not repeat project-wide DNS checks"
        );
        assert_eq!(
            stopped.value.routes[1].public.status,
            PublicStatus::NotChecked
        );
        assert!(
            !cache.complete(&first, first.value.clone(), 4),
            "old work cannot restore paused public evidence"
        );
        assert!(cache.complete(&stopped, stopped.value.clone(), 4));

        let mut resumed = hostname_fixture();
        for mapping in &mut resumed.binding.mappings {
            mapping.enabled = true;
        }
        let started = cache.reserve(resumed, false, None, 5).unwrap();
        assert_eq!(
            started.targets,
            vec![1],
            "only the newly started hostname needs a public check"
        );
        assert!(
            !started.check_dns,
            "valid DNS evidence survives a hostname stop/start"
        );
        assert_eq!(started.value.routes[2].local.checked_at, 42);
    }

    #[test]
    fn rapid_stop_start_without_a_monitor_tick_rejects_pre_stop_public_results() {
        for in_flight in [false, true] {
            let cache = Connections::default();
            let mut snapshot = hostname_fixture();
            for mapping in &mut snapshot.binding.mappings {
                mapping.enabled = true;
            }
            let mut old = cache.reserve(snapshot, false, None, 1).unwrap();
            for route in &mut old.value.routes {
                route.public.status = PublicStatus::Verified;
                route.local.status = ServiceStatus::Reachable;
                route.dns.status = DnsStatus::Connected;
                route.dns.checked_at = 42;
                route.checking = in_flight;
            }
            if in_flight {
                assert!(cache.progress(&old));
            } else {
                assert!(cache.complete(&old, old.value.clone(), 2));
            }
            let key = RouteKey {
                hostname: "api.example.com".into(),
                path: None,
                service: "http://localhost:8787".into(),
            };
            cache.hostname_changed("project", &key); // Stop.
            cache.hostname_changed("project", &key); // Start before the monitor observes Stop.
            assert!(!cache.current("project", old.generation));
            assert!(!cache.progress(&old));
            assert!(!cache.complete(&old, old.value.clone(), 3));
            let next = cache.reserve(old.snapshot, false, None, 4).unwrap();
            assert_eq!(next.value.routes[1].public.status, PublicStatus::NotChecked);
            assert_eq!(next.value.routes[0].local.status, ServiceStatus::Reachable);
            if !in_flight {
                assert_eq!(next.targets, vec![1]);
                assert_eq!(next.value.routes[0].public.status, PublicStatus::Verified);
                assert!(!next.check_dns);
            }
        }
    }

    #[test]
    fn manual_sibling_check_does_not_consume_a_pending_or_in_flight_hostname_start() {
        for already_reserved in [false, true] {
            let cache = Connections::default();
            let first = cache.reserve(hostname_fixture(), false, None, 1).unwrap();
            let mut evidence = first.value.clone();
            for route in &mut evidence.routes {
                route.dns.status = DnsStatus::Connected;
                route.dns.checked_at = 42;
                route.checking = false;
            }
            assert!(cache.complete(&first, evidence, 2));
            let key = RouteKey {
                hostname: "api.example.com".into(),
                path: None,
                service: "http://localhost:8787".into(),
            };
            cache.hostname_changed("project", &key);
            let resumed = || {
                let mut snapshot = hostname_fixture();
                snapshot.binding.mappings[1].enabled = true;
                snapshot
            };
            if already_reserved {
                let work = cache.reserve(resumed(), false, None, 3).unwrap();
                assert_eq!(work.targets, vec![1]);
            }
            let manual = cache.reserve(resumed(), true, Some(&[0]), 4).unwrap();
            assert_eq!(manual.targets, vec![0]);
            assert!(cache.complete(&manual, manual.value.clone(), 5));
            let pending = cache.reserve(resumed(), false, None, 6).unwrap();
            assert_eq!(
                pending.targets,
                vec![1],
                "the manual sibling check must not lose the started hostname's check"
            );
        }
    }

    #[test]
    fn automatic_dns_selects_only_enabled_running_hostnames() {
        let cache = Connections::default();
        let work = cache.reserve(hostname_fixture(), false, None, 1).unwrap();
        assert_eq!(checking_indices(&work), vec![0]);
        assert!(active_route(&work.snapshot.binding, 0));
        assert!(!active_route(&work.snapshot.binding, 1));

        let mut all_enabled = hostname_fixture();
        for mapping in &mut all_enabled.binding.mappings {
            mapping.enabled = true;
        }
        let work = cache.reserve(all_enabled, false, None, 2).unwrap();
        assert_eq!(checking_indices(&work), vec![0, 1, 2]);
    }

    #[test]
    fn stopped_project_does_not_schedule_dns_for_desired_enabled_hostnames() {
        let cache = Connections::default();
        let mut stopped = hostname_fixture();
        stopped.binding.status = Status::Stopped;
        stopped.binding.supervised = false;
        stopped.binding.pid = None;
        for mapping in &mut stopped.binding.mappings {
            mapping.enabled = true;
        }
        let work = cache.reserve(stopped, false, None, 1).unwrap();
        assert!(checking_indices(&work).is_empty());
        assert!(work.targets.is_empty());
        assert!(!work.value.checking);
        for index in 0..work.snapshot.binding.mappings.len() {
            assert!(!active_route(&work.snapshot.binding, index));
        }
        let work = cache.reserve(work.snapshot, true, Some(&[1]), 2).unwrap();
        assert_eq!(checking_indices(&work), vec![1]);
        assert!(!active_route(&work.snapshot.binding, 1));
    }

    #[test]
    fn manual_disabled_hostname_check_does_not_select_inactive_siblings() {
        let cache = Connections::default();
        let old = cache.reserve(hostname_fixture(), false, None, 1).unwrap();
        let mut value = old.value.clone();
        value.routes[0].dns.status = DnsStatus::Connected;
        value.routes[0].dns.checked_at = 42;
        assert!(cache.complete(&old, value, 2));
        // Unknown evidence on inactive siblings must not increase the active-route backoff.
        let next_dns = {
            let entries = cache.entries.lock().unwrap();
            assert_eq!(entries["project"].dns_failures, 0);
            entries["project"].next_dns
        };
        let work = cache
            .reserve(hostname_fixture(), true, Some(&[1]), 3)
            .unwrap();
        assert_eq!(checking_indices(&work), vec![1]);
        assert_eq!(work.value.routes[0].dns.checked_at, 42);
        assert!(!cache.complete(&old, old.value.clone(), 4));
        assert!(cache.complete(&work, work.value.clone(), 4));
        let entries = cache.entries.lock().unwrap();
        assert_eq!(entries["project"].next_dns, next_dns);
        assert_eq!(
            entries["project"].value.routes[0].dns.status,
            DnsStatus::Connected
        );
    }

    #[test]
    fn changed_run_revision_source_auth_and_reset_reject_old_results() {
        for change in 0..7 {
            let cache = Connections::default();
            let old = cache.reserve(fixture(), false, None, 1).unwrap();
            let mut next = fixture();
            match change {
                0 => next.binding.run += 1,
                1 => next.binding.config_revision += 1,
                2 => next.binding.fingerprint = Some(ConfigFingerprint::of(b"changed")),
                3 => next.binding.auth.push(Some((4, 5))),
                4 => next.binding.status = Status::Stopped,
                5 => next.binding.epoch += 1,
                _ => next.binding.mappings[0].enabled = false,
            }
            let new = cache.reserve(next, false, None, 2).unwrap();
            assert!(!cache.complete(&old, old.value.clone(), 3));
            assert!(cache.complete(&new, new.value.clone(), 3));
        }
    }

    #[test]
    fn manual_checks_and_repairs_supersede_in_flight_work() {
        let cache = Connections::default();
        let old = cache.reserve(fixture(), false, None, 1).unwrap();
        assert!(cache.reserve(fixture(), false, None, 2).is_none());
        let new = cache.reserve(fixture(), true, None, 2).unwrap();
        assert!(new.value.generation > old.value.generation);
        assert!(!cache.complete(&old, old.value.clone(), 3));
        cache.invalidate("project");
        assert!(!cache.complete(&new, new.value.clone(), 4));
    }

    #[test]
    fn hostname_gate_change_cancels_work_reserved_before_the_change_completed() {
        let cache = Connections::default();
        let during_change = cache.reserve(fixture(), false, None, 1).unwrap();
        cache.hostname_changed(
            "project",
            &RouteKey {
                hostname: "app.example.com".into(),
                path: None,
                service: "http://localhost:8787".into(),
            },
        );
        let mut paused = fixture();
        paused.binding.mappings[0].enabled = false;
        let current = cache.reserve(paused, false, None, 2).unwrap();
        assert!(!cache.current("project", during_change.generation));
        assert!(cache.current("project", current.generation));
        assert!(!current.snapshot.binding.mappings[0].enabled);
    }

    #[test]
    fn stopped_projects_only_check_once_unless_requested_or_changed() {
        let cache = Connections::default();
        let mut stopped = fixture();
        stopped.binding.status = Status::Stopped;
        let work = cache.reserve(stopped, false, None, 1).unwrap();
        assert!(cache.complete(&work, work.value.clone(), 2));
        let mut stopped = fixture();
        stopped.binding.status = Status::Stopped;
        assert!(cache.reserve(stopped, false, None, 1_000_000).is_none());
    }

    #[test]
    fn public_success_requires_the_intended_live_route_and_fresh_evidence() {
        let snapshot = fixture();
        let mut dns = unverified("not read", snapshot.binding.expected.as_deref());
        let local = ServiceCheck {
            status: ServiceStatus::Reachable,
            message: String::new(),
            checked_at: 1,
        };
        let verified = PublicCheck {
            status: PublicStatus::Verified,
            http_status: Some(200),
            ..public_pending("OK")
        };
        let mut public = verified.clone();
        qualify_public(&snapshot.binding, &dns, &local, &mut public);
        assert_eq!(public.status, PublicStatus::Responded);
        // The reported bug: a healthy connector and listener cannot validate another tunnel's DNS.
        dns.status = DnsStatus::Conflict;
        dns.source = "cloudflare".into();
        dns.reason = Some("wrong_tunnel".into());
        dns.observed_tunnel_id = Some("5b1370f1-e811-4bcf-8077-a793650bb7e3".into());
        let mut wrong_tunnel = verified.clone();
        qualify_public(&snapshot.binding, &dns, &local, &mut wrong_tunnel);
        assert_eq!(wrong_tunnel.status, PublicStatus::Responded);
        assert_eq!(snapshot.binding.status, Status::Connected);
        dns.status = DnsStatus::Connected;
        dns.reason = None;
        dns.source = "cloudflare".into();
        let mut public = verified.clone();
        qualify_public(&snapshot.binding, &dns, &local, &mut public);
        assert_eq!(public.status, PublicStatus::Verified);
        let mut value = initial(&snapshot);
        value.routes[0].public = verified;
        let now = value.routes[0].public.checked_at + PUBLIC_FRESHNESS + 1;
        expire(&mut value, now);
        assert_eq!(value.routes[0].public.status, PublicStatus::NotChecked);
        for status in [Status::Stopped, Status::Crashed, Status::Degraded] {
            let mut binding = snapshot.binding.clone();
            binding.status = status;
            let mut public = PublicCheck {
                status: PublicStatus::Verified,
                ..public_pending("OK")
            };
            qualify_public(&binding, &dns, &local, &mut public);
            assert_eq!(public.status, PublicStatus::Responded);
        }
    }
}
