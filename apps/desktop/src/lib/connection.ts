import type {
  RouteConnection,
  Tunnel,
  TunnelConnection,
  TunnelState,
} from "./types";

const ORIGIN_FAILURE = /cloudflared reported an origin connection failure\./i;
// biome-ignore lint/suspicious/noControlCharactersInRegex: Prevent control characters in public probe URLs.
const UNSAFE_PATH = /[\\#\s\u0000-\u001f\u007f]/;

/** Compare publications across both events and overview replies; sequence is local to a run. */
export function compareTunnelState(
  previous: TunnelState,
  incoming: TunnelState
): number {
  if (previous.run !== incoming.run) {
    return previous.run - incoming.run;
  }
  const previousSeq = previous.state_seq ?? 0;
  const incomingSeq = incoming.state_seq ?? 0;
  if (previousSeq || incomingSeq) {
    return previousSeq - incomingSeq;
  }
  return previous.config_revision - incoming.config_revision;
}

export function newestConnection(
  previous: TunnelConnection | null | undefined,
  incoming: TunnelConnection
) {
  if (
    !previous ||
    previous.run !== incoming.run ||
    previous.config_revision !== incoming.config_revision
  ) {
    return incoming;
  }
  const oldGeneration = previous.generation ?? 0;
  const newGeneration = incoming.generation ?? 0;
  if (newGeneration < oldGeneration) {
    return previous;
  }
  if (
    newGeneration === oldGeneration &&
    ((!previous.checking && incoming.checking) ||
      (!incoming.checking &&
        (previous.checked_at ?? 0) > (incoming.checked_at ?? 0)))
  ) {
    return previous;
  }
  return incoming;
}

export function mergeTunnelSnapshot(
  previous: Tunnel | undefined,
  incoming: Tunnel,
  stateChanged = false,
  connectionChanged = false
): Tunnel {
  if (!previous) {
    return incoming;
  }
  const order = compareTunnelState(previous.state, incoming.state);
  const stateIsNewer =
    order > 0 ||
    (order === 0 &&
      stateChanged &&
      previous.state.run === incoming.state.run &&
      previous.state.config_revision === incoming.state.config_revision);
  const merged = stateIsNewer
    ? { ...incoming, state: previous.state }
    : incoming;
  const priorConnection = currentConnection({
    ...previous,
    state: merged.state,
  });
  if (incoming.connection) {
    const next = currentConnection(merged);
    merged.connection = next
      ? newestConnection(priorConnection, next)
      : priorConnection;
  } else if ((stateIsNewer || connectionChanged) && priorConnection) {
    merged.connection = priorConnection;
  }
  return merged;
}

export function currentConnection(tunnel: Tunnel) {
  const { connection } = tunnel;
  if (
    !connection ||
    connection.run !== tunnel.state.run ||
    connection.config_revision !== tunnel.state.config_revision
  ) {
    return null;
  }
  if (
    tunnel.kind === "config" &&
    connection.routes.some((route) => {
      const mapping = tunnel.state.mappings[route.rule_index];
      return (
        !mapping ||
        mapping.hostname !== route.hostname ||
        mapping.path !== route.path ||
        mapping.service !== route.service
      );
    })
  ) {
    return null;
  }
  return connection;
}

const result = (
  title: string,
  message: string,
  tone: "warning" | "neutral" | "ok",
  action: "repair" | "start" | "check" | null = "check"
) => ({ action, message, title, tone });

export function routePresentation(
  tunnel: Tunnel,
  route: RouteConnection,
  now = Date.now()
) {
  const connection = currentConnection(tunnel);
  if (tunnel.state.mappings[route.rule_index]?.enabled === false) {
    return result(
      "Hostname stopped",
      "Start this hostname to forward public traffic to its local service.",
      "neutral",
      null
    );
  }
  if (route.dns.status === "conflict") {
    const expected =
      route.dns.expected_tunnel_id ?? connection?.expected_tunnel_id;
    const wrongTunnel =
      route.dns.reason === "wrong_tunnel" ||
      Boolean(
        route.dns.observed_tunnel_id &&
          expected &&
          route.dns.observed_tunnel_id !== expected
      );
    return result(
      wrongTunnel
        ? "This hostname points to a different tunnel"
        : "DNS needs attention",
      route.dns.message,
      "warning",
      route.dns.can_repair ? "repair" : "check"
    );
  }
  if (route.dns.status === "missing") {
    return result("DNS record missing", route.dns.message, "warning", null);
  }
  if (route.dns.status === "unknown") {
    return result("DNS target unverified", route.dns.message, "neutral");
  }
  if (connection?.config_changed) {
    return result(
      "Config changed on disk",
      "Restart the tunnel to apply your changes, then check again.",
      "warning"
    );
  }
  if (tunnel.state.status === "stopped" || tunnel.state.status === "crashed") {
    return result(
      "DNS is ready. Start the tunnel.",
      "This hostname routes to this project, but its connector is stopped.",
      "neutral",
      "start"
    );
  }
  if (route.local.status === "unreachable") {
    return result(
      "Your local service is not responding",
      `Start the service at ${route.service}, then check again. ${route.local.message}`,
      "warning"
    );
  }
  return publicPresentation(tunnel, route, now);
}

function publicPresentation(
  tunnel: Tunnel,
  route: RouteConnection,
  now: number
) {
  const connection = currentConnection(tunnel);
  const endpoint = route.public;
  if (endpoint.status === "cloudflare_error") {
    return result(
      endpoint.cloudflare_error === 1033
        ? "Cloudflare cannot reach the tunnel (1033)"
        : `Cloudflare returned an error${endpoint.http_status ? ` (${endpoint.http_status})` : ""}`,
      endpoint.message,
      "warning"
    );
  }
  if (endpoint.status === "origin_error") {
    return result(
      "The tunnel cannot reach your service (502)",
      endpoint.message,
      "warning"
    );
  }
  if (endpoint.status === "transport_error") {
    return result(
      "Public connection could not be verified",
      endpoint.message,
      "warning"
    );
  }
  if (endpoint.status === "http_error") {
    return result(
      `Public endpoint returned HTTP ${endpoint.http_status ?? "error"}`,
      endpoint.message,
      "warning"
    );
  }
  if (endpoint.status === "auth_required") {
    return result(
      "Public endpoint responded. Sign-in is required.",
      `The application behind authentication is not verified. ${endpoint.message}`,
      "neutral"
    );
  }
  if (endpoint.status === "responded") {
    return result(
      "Public endpoint responded",
      `Application verification is incomplete. ${endpoint.message}`,
      "neutral"
    );
  }
  const checked =
    endpoint.checked_at < 1e12
      ? endpoint.checked_at * 1000
      : endpoint.checked_at;
  if (
    endpoint.status === "verified" &&
    endpoint.http_status !== null &&
    endpoint.http_status >= 200 &&
    endpoint.http_status < 300 &&
    now - checked < 90_000 &&
    now >= checked &&
    tunnel.state.status === "connected" &&
    route.local.status === "reachable" &&
    (tunnel.kind !== "config" || route.dns.source === "cloudflare") &&
    connection &&
    !connection.error
  ) {
    return result("Public connection verified", endpoint.message, "ok");
  }
  return result(
    "Public connection not verified",
    endpoint.message || "Check the public endpoint to verify this hostname.",
    "neutral"
  );
}

export function routeFixAction(tunnel: Tunnel, route: RouteConnection) {
  if (tunnel.state.mappings[route.rule_index]?.enabled === false) {
    return null;
  }
  const connection = currentConnection(tunnel);
  if (
    !connection ||
    connection.config_changed ||
    connection.error ||
    route.dns.source !== "cloudflare"
  ) {
    return null;
  }
  if (route.dns.status === "conflict" && route.dns.can_repair) {
    return "repair";
  }
  if (
    route.dns.status === "missing" &&
    route.dns.can_create &&
    !route.dns.needs_dns_access
  ) {
    return "create";
  }
  return null;
}

export function connectionWarning(tunnel: Tunnel) {
  const connection = currentConnection(tunnel);
  if (!connection) {
    return null;
  }
  if (connection.error) {
    return "Connection check incomplete";
  }
  if (connection.config_changed) {
    return "Config changed on disk";
  }
  const warning = connection.routes
    .map((route) => routePresentation(tunnel, route))
    .find((item) => item.tone === "warning");
  return warning?.title ?? null;
}

export function resolvedOriginFailure(tunnel: Tunnel, now = Date.now()) {
  const { failure } = tunnel.state;
  if (!(failure && ORIGIN_FAILURE.test(failure.cause))) {
    return false;
  }
  const connection = currentConnection(tunnel);
  const active =
    connection?.routes.filter(
      (route) => tunnel.state.mappings[route.rule_index]?.enabled !== false
    ) ?? [];
  return (
    active.length > 0 &&
    active.every((route) => routePresentation(tunnel, route, now).tone === "ok")
  );
}

export function checkedTime(value: number | null | undefined) {
  if (!value) {
    return "Not checked yet";
  }
  const date = new Date(value < 1e12 ? value * 1000 : value);
  return `Checked ${date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}`;
}

export function safeTestPath(value: string) {
  return (
    value.startsWith("/") &&
    !value.startsWith("//") &&
    !UNSAFE_PATH.test(value) &&
    value.length <= 2048
  );
}
