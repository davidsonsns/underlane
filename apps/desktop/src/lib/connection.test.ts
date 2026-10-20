import { describe, expect, it } from "vitest";
import {
  connectionWarning,
  currentConnection,
  mergeTunnelSnapshot,
  newestConnection,
  resolvedOriginFailure,
  routeFixAction,
  routePresentation,
  safeTestPath,
} from "./connection";
import type { RouteConnection, Tunnel, TunnelConnection } from "./types";

const now = 1_800_000_000_000;
function fixture() {
  const route: RouteConnection = {
    checking: false,
    dns: {
      can_create: false,
      message: "DNS correct",
      needs_dns_access: false,
      source: "cloudflare",
      status: "connected",
    },
    hostname: "app.example.com",
    local: { checked_at: now, message: "Listening", status: "reachable" },
    path: null,
    public: {
      checked_at: now,
      cloudflare_error: null,
      http_status: 200,
      message: "Verified",
      status: "verified",
      test_url: "https://app.example.com/",
    },
    rule_index: 0,
    service: "http://localhost:4000",
    test_path: null,
  };
  const tunnel: Tunnel & { connection: TunnelConnection } = {
    autostart: false,
    config_path: "/test.yml",
    connection: {
      checked_at: now,
      checking: false,
      config_changed: false,
      config_revision: 3,
      error: null,
      expected_tunnel_id: "expected",
      routes: [route],
      run: 2,
    },
    created_at: now,
    group_id: null,
    group_name: "",
    has_token: false,
    id: "test",
    inspect: false,
    kind: "config",
    loglevel: "info",
    name: "Test",
    origin_url: null,
    sort: 0,
    state: {
      config_revision: 3,
      connections: 4,
      detail: null,
      failure: null,
      hint: null,
      id: "test",
      inspecting: false,
      mappings: [
        {
          hostname: route.hostname,
          inspected: false,
          note: null,
          path: route.path,
          service: route.service,
        },
      ],
      metrics_addr: null,
      pid: 123,
      public_url: null,
      restarts: 0,
      run: 2,
      started_at: now,
      status: "connected",
      supervised: true,
    },
  };
  return { route, tunnel };
}

describe("connection route identity", () => {
  it.each(["removed", "reordered", "hostname", "path", "service"] as const)(
    "rejects cached config evidence when a stopped source route is %s at the same run and revision",
    (change) => {
      const { tunnel, route } = fixture();
      tunnel.state.status = "stopped";
      tunnel.state.supervised = false;
      tunnel.state.pid = null;
      tunnel.state.connections = 0;
      route.dns.status = "conflict";
      route.dns.can_repair = true;
      const [mapping] = tunnel.state.mappings;
      if (change === "removed") {
        tunnel.state.mappings = [];
      } else if (change === "reordered") {
        tunnel.state.mappings = [
          { ...mapping, hostname: "different.example.com" },
          mapping,
        ];
      } else if (change === "hostname") {
        mapping.hostname = "changed.example.com";
      } else if (change === "path") {
        mapping.path = "/changed";
      } else {
        mapping.service = "http://localhost:9000";
      }
      expect(currentConnection(tunnel)).toBeNull();
      expect(connectionWarning(tunnel)).toBeNull();
    }
  );
  it("checks every evidence row while retaining paused rows with unchanged identity", () => {
    const { tunnel, route } = fixture();
    tunnel.state.mappings.push({
      ...tunnel.state.mappings[0],
      enabled: false,
      hostname: "api.example.com",
    });
    tunnel.connection.routes.push({
      ...route,
      hostname: "api.example.com",
      rule_index: 1,
    });
    expect(currentConnection(tunnel)).toBe(tunnel.connection);
    tunnel.state.mappings[1].service = "http://localhost:9000";
    expect(currentConnection(tunnel)).toBeNull();
  });
  it("keeps quick-tunnel evidence when the generated hostname is absent from ingress mappings", () => {
    const { tunnel } = fixture();
    tunnel.kind = "quick";
    tunnel.state.mappings[0].hostname = null;
    tunnel.state.public_url = "https://app.example.com";
    expect(currentConnection(tunnel)).toBe(tunnel.connection);
  });
});

describe("connection presentation", () => {
  it("keeps a stopped hostname neutral even when cached public evidence was verified", () => {
    const { tunnel, route } = fixture();
    tunnel.state.mappings = [
      {
        enabled: false,
        hostname: route.hostname,
        inspected: false,
        note: null,
        path: route.path,
        service: route.service,
      },
    ];
    route.dns.status = "conflict";
    route.dns.can_repair = true;
    expect(routePresentation(tunnel, route, now)).toMatchObject({
      action: null,
      title: "Hostname stopped",
      tone: "neutral",
    });
    expect(routeFixAction(tunnel, route)).toBeNull();
    expect(connectionWarning(tunnel)).toBeNull();
  });
  it("prioritizes wrong DNS while retaining all the route evidence", () => {
    const { tunnel, route } = fixture();
    route.dns.status = "conflict";
    route.dns.reason = "wrong_tunnel";
    route.dns.can_repair = true;
    route.local.status = "unreachable";
    tunnel.state.status = "stopped";
    expect(routePresentation(tunnel, route, now)).toMatchObject({
      action: "repair",
      tone: "warning",
    });
    expect(connectionWarning(tunnel)).toContain("different tunnel");
    expect(route.local.status).toBe("unreachable");
    route.dns.status = "connected";
    expect(routePresentation(tunnel, route, now).action).toBe("start");
  });
  it("requires recent same-run successful end-to-end evidence", () => {
    const { tunnel, route } = fixture();
    expect(routePresentation(tunnel, route, now).tone).toBe("ok");
    route.dns.source = "public_dns";
    expect(routePresentation(tunnel, route, now).tone).toBe("neutral");
    route.dns.source = "cloudflare";
    expect(routePresentation(tunnel, route, now + 90_001).tone).toBe("neutral");
    route.local.status = "unsupported";
    expect(routePresentation(tunnel, route, now).tone).toBe("neutral");
    route.local.status = "reachable";
    tunnel.state.run += 1;
    expect(currentConnection(tunnel)).toBeNull();
    expect(routePresentation(tunnel, route, now).tone).toBe("neutral");
  });
  it("keeps unknown DNS neutral, and never treats HTTP responses as verified", () => {
    const { tunnel, route } = fixture();
    route.dns.status = "unknown";
    expect(routePresentation(tunnel, route, now).tone).toBe("neutral");
    route.dns.status = "connected";
    for (const status of ["responded", "auth_required"] as const) {
      route.public.status = status;
      expect(routePresentation(tunnel, route, now).tone).toBe("neutral");
    }
    route.public.status = "verified";
    route.public.http_status = 404;
    expect(routePresentation(tunnel, route, now).tone).toBe("neutral");
  });
  it("distinguishes local, Cloudflare, origin, and transport errors", () => {
    const { tunnel, route } = fixture();
    route.local.status = "unreachable";
    expect(routePresentation(tunnel, route, now).message).toContain(
      route.service
    );
    route.local.status = "reachable";
    route.public.status = "cloudflare_error";
    route.public.cloudflare_error = 1033;
    expect(routePresentation(tunnel, route, now).title).toContain("1033");
    route.public.cloudflare_error = null;
    route.public.http_status = 530;
    expect(routePresentation(tunnel, route, now).title).toContain("530");
    route.public.status = "origin_error";
    expect(routePresentation(tunnel, route, now).title).toContain("502");
    route.public.status = "transport_error";
    expect(routePresentation(tunnel, route, now).title).toContain(
      "could not be verified"
    );
  });
  it("only dismisses a known origin failure after all routes recover", () => {
    const { tunnel, route } = fixture();
    tunnel.state.failure = {
      action: null,
      cause: "cloudflared reported an origin connection failure.",
      raw: "origin unavailable",
    };
    expect(resolvedOriginFailure(tunnel, now)).toBe(true);
    route.local.status = "unreachable";
    expect(resolvedOriginFailure(tunnel, now)).toBe(false);
    route.local.status = "reachable";
    tunnel.state.failure.cause = "Unknown failure";
    expect(resolvedOriginFailure(tunnel, now)).toBe(false);
  });
  it("dismisses a recovered active origin failure without requiring paused hostnames to verify", () => {
    const { tunnel, route } = fixture();
    tunnel.state.failure = {
      action: null,
      cause: "cloudflared reported an origin connection failure.",
      raw: "origin unavailable",
    };
    tunnel.state.mappings = [false, true].map((enabled) => ({
      enabled,
      hostname: route.hostname,
      inspected: false,
      note: null,
      path: route.path,
      service: route.service,
    }));
    tunnel.connection.routes = [route, { ...route, rule_index: 1 }];
    expect(resolvedOriginFailure(tunnel, now)).toBe(true);
    tunnel.state.mappings[1].enabled = false;
    expect(resolvedOriginFailure(tunnel, now)).toBe(false);
    tunnel.state.mappings[1].enabled = true;
    tunnel.connection.routes[1].local = {
      ...route.local,
      status: "unreachable",
    };
    expect(resolvedOriginFailure(tunnel, now)).toBe(false);
  });
  it("keeps a correct but unproxied DNS target distinct from a different tunnel", () => {
    const { tunnel, route } = fixture();
    route.dns.status = "conflict";
    route.dns.reason = "proxy_disabled";
    route.dns.expected_tunnel_id = "expected";
    route.dns.observed_tunnel_id = "expected";
    route.dns.can_repair = false;
    expect(routePresentation(tunnel, route, now)).toMatchObject({
      action: "check",
      title: "DNS needs attention",
    });
    route.dns.observed_tunnel_id = "different";
    expect(routePresentation(tunnel, route, now).title).toBe(
      "This hostname points to a different tunnel"
    );
  });
});

describe("actionable hostname banners", () => {
  it("only offers an authorized DNS repair or creation", () => {
    const { tunnel, route } = fixture();
    route.dns.status = "conflict";
    route.dns.can_repair = true;
    expect(routeFixAction(tunnel, route)).toBe("repair");
    route.dns.can_repair = false;
    expect(routeFixAction(tunnel, route)).toBeNull();
    route.dns.status = "missing";
    route.dns.can_create = true;
    expect(routeFixAction(tunnel, route)).toBe("create");
    route.dns.needs_dns_access = true;
    expect(routeFixAction(tunnel, route)).toBeNull();
    route.dns.needs_dns_access = false;
    route.dns.source = "public";
    expect(routeFixAction(tunnel, route)).toBeNull();
  });
  it("keeps normal lifecycle and informational states compact", () => {
    const { tunnel, route } = fixture();
    for (const status of [
      "stopped",
      "starting",
      "connected",
      "crashed",
    ] as const) {
      tunnel.state.status = status;
      expect(routeFixAction(tunnel, route)).toBeNull();
    }
    route.local.status = "unreachable";
    for (const status of [
      "not_checked",
      "verified",
      "responded",
      "auth_required",
      "transport_error",
      "cloudflare_error",
      "origin_error",
      "http_error",
      "unsupported",
    ] as const) {
      route.public.status = status;
      expect(routeFixAction(tunnel, route)).toBeNull();
    }
    route.dns.status = "unknown";
    expect(routeFixAction(tunnel, route)).toBeNull();
    route.dns.status = "conflict";
    route.dns.can_repair = true;
    tunnel.connection.config_changed = true;
    expect(routeFixAction(tunnel, route)).toBeNull();
  });
});

describe("connection ordering", () => {
  it("orders same-run overview states by publication sequence before transport timing", () => {
    const { tunnel } = fixture();
    const stopped = {
      ...tunnel,
      state: { ...tunnel.state, state_seq: 10, status: "stopped" as const },
    };
    const old = { ...tunnel, state: { ...tunnel.state, state_seq: 9 } };
    expect(mergeTunnelSnapshot(stopped, old).state).toBe(stopped.state);
    const newer = { ...tunnel, state: { ...tunnel.state, state_seq: 11 } };
    expect(mergeTunnelSnapshot(stopped, newer, true).state).toBe(newer.state);
    expect(mergeTunnelSnapshot(stopped, tunnel).state).toBe(stopped.state);
    expect(
      mergeTunnelSnapshot(stopped, {
        ...tunnel,
        state: { ...tunnel.state, state_seq: 0 },
      }).state
    ).toBe(stopped.state);
    const nextRun = {
      ...tunnel,
      state: { ...tunnel.state, run: tunnel.state.run + 1, state_seq: 1 },
    };
    expect(
      mergeTunnelSnapshot(
        { ...tunnel, state: { ...tunnel.state, state_seq: 100 } },
        nextRun
      ).state
    ).toBe(nextRun.state);
  });
  it("keeps legacy same-run revision ordering when both publications have no sequence", () => {
    const { tunnel } = fixture();
    const older = { ...tunnel, state: { ...tunnel.state, config_revision: 2 } };
    expect(mergeTunnelSnapshot(tunnel, older).state).toBe(tunnel.state);
    const newer = { ...tunnel, state: { ...tunnel.state, config_revision: 4 } };
    expect(mergeTunnelSnapshot(tunnel, newer, true).state).toBe(newer.state);
  });
  it("keeps a newer completed generation over old and same-generation checking events", () => {
    const { tunnel } = fixture();
    const completed = {
      ...tunnel.connection,
      checked_at: 200,
      checking: false,
      generation: 2,
    };
    const oldChecking = {
      ...completed,
      checked_at: 100,
      checking: true,
      generation: 1,
    };
    expect(newestConnection(completed, oldChecking)).toBe(completed);
    expect(newestConnection(completed, { ...completed, checking: true })).toBe(
      completed
    );
    expect(
      newestConnection(completed, { ...oldChecking, generation: 3 })
    ).not.toBe(completed);
  });
  it("merges a stale overview without replacing newer connection evidence", () => {
    const { tunnel } = fixture();
    const completed = {
      ...tunnel.connection,
      checked_at: 200,
      checking: false,
      generation: 2,
    };
    const previous = { ...tunnel, connection: completed };
    const incoming = {
      ...tunnel,
      connection: { ...completed, checked_at: 100, generation: 1 },
    };
    expect(mergeTunnelSnapshot(previous, incoming).connection).toBe(completed);
    expect(
      mergeTunnelSnapshot(previous, {
        ...incoming,
        connection: { ...completed, checking: true },
      }).connection
    ).toBe(completed);
    expect(
      mergeTunnelSnapshot(previous, { ...incoming, connection: null })
        .connection
    ).toBeNull();
  });
  it("retains the newer run and matching diagnostics when an older overview resolves", () => {
    const { tunnel } = fixture();
    const incoming = {
      ...tunnel,
      connection: { ...tunnel.connection, generation: 1, run: 1 },
      state: { ...tunnel.state, run: 1 },
    };
    const merged = mergeTunnelSnapshot(tunnel, incoming);
    expect(merged.state).toBe(tunnel.state);
    expect(merged.connection).toBe(tunnel.connection);
    const changed = mergeTunnelSnapshot(
      tunnel,
      { ...tunnel, state: { ...tunnel.state, status: "stopped" } },
      true
    );
    expect(changed.state).toBe(tunnel.state);
    const nextRun = {
      ...tunnel,
      connection: { ...tunnel.connection, generation: 3, run: 3 },
      state: { ...tunnel.state, run: 3 },
    };
    expect(mergeTunnelSnapshot(tunnel, nextRun, true).state.run).toBe(3);
  });
  it("preserves newer-run warning evidence when an older overview has no diagnostics", () => {
    const { tunnel, route } = fixture();
    route.dns.status = "conflict";
    route.dns.reason = "wrong_tunnel";
    const incoming = {
      ...tunnel,
      connection: null,
      state: { ...tunnel.state, run: 1 },
    };
    const merged = mergeTunnelSnapshot(tunnel, incoming);
    expect(merged.state).toBe(tunnel.state);
    expect(merged.connection).toBe(tunnel.connection);
    expect(connectionWarning(merged)).toBe(
      "This hostname points to a different tunnel"
    );
  });
  it("preserves evidence received during a same-state overview request, while current null invalidates", () => {
    const { tunnel } = fixture();
    const incoming = { ...tunnel, connection: null };
    expect(mergeTunnelSnapshot(tunnel, incoming, false, true).connection).toBe(
      tunnel.connection
    );
    expect(
      mergeTunnelSnapshot(tunnel, { ...tunnel, connection: null }, false, false)
        .connection
    ).toBeNull();
  });
});

describe("public test path", () => {
  it("accepts only relative paths on the same hostname", () => {
    expect(safeTestPath("/api/health?check=1")).toBe(true);
    for (const value of [
      "https://example.com",
      "//example.com",
      "/\\evil",
      "/a#fragment",
      "/white space",
      "/a\nb",
      "health",
    ]) {
      expect(safeTestPath(value)).toBe(false);
    }
  });
});
