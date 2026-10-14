import { describe, expect, it } from "vitest";
import { activeTunnelOwner, sharedTunnelProjects } from "./tunnel-conflict";
import type { Tunnel, TunnelState } from "./types";

const uuid = "00000000-0000-4000-8000-000000000104";
const otherUuid = "00000000-0000-4000-8000-000000000103";

function project(
  id: string,
  sourceId: string | null = uuid,
  state: Partial<TunnelState> = {}
): Tunnel {
  return {
    autostart: false,
    cloudflare_tunnel_id: sourceId,
    config_path: `/configs/${id}.yml`,
    created_at: 0,
    group_id: null,
    group_name: "",
    has_token: false,
    id,
    inspect: false,
    kind: "config",
    loglevel: "info",
    name: id,
    origin_url: null,
    sort: 0,
    state: {
      config_revision: 0,
      connections: 0,
      detail: null,
      failure: null,
      hint: null,
      id,
      inspecting: false,
      mappings: [],
      metrics_addr: null,
      pid: null,
      public_url: null,
      restarts: 0,
      run: 0,
      started_at: null,
      status: "stopped",
      supervised: false,
      ...state,
    },
  };
}

describe("shared Cloudflare tunnels", () => {
  it("matches resolved identities even when project names and config paths differ", () => {
    const example = project("example");
    const citizen = project("newlycitizen");
    expect(sharedTunnelProjects(citizen, [example, citizen])).toEqual([example]);
    expect(activeTunnelOwner(citizen, [example, citizen])).toBeUndefined();
  });

  it.each(["starting", "connected", "degraded"] as const)(
    "blocks another project while its owner is %s",
    (status) => {
      const owner = project("example", uuid, { status });
      expect(activeTunnelOwner(project("citizen"), [owner])).toBe(owner);
    }
  );

  it("keeps ownership through a supervised crash backoff", () => {
    const owner = project("example", uuid, { status: "crashed", supervised: true });
    expect(activeTunnelOwner(project("citizen"), [owner])).toBe(owner);
    owner.state.supervised = false;
    expect(activeTunnelOwner(project("citizen"), [owner])).toBeUndefined();
  });

  it("excludes self, different identities, quick tunnels and unidentified projects", () => {
    const target = project("citizen");
    const quick = {
      ...project("quick", uuid, { status: "connected" }),
      kind: "quick" as const,
    };
    expect(
      sharedTunnelProjects(target, [
        target,
        project("other", otherUuid),
        quick,
        project("missing", null),
      ])
    ).toEqual([]);
    expect(sharedTunnelProjects(quick, [target])).toEqual([]);
    expect(
      sharedTunnelProjects(project("missing", null), [
        project("also-missing", null),
      ])
    ).toEqual([]);
  });

  it("uses the running identity until the connector stops, even after a source edit", () => {
    const owner = project("example", otherUuid, {
      cloudflare_tunnel_id: uuid,
      status: "connected",
    });
    const target = project("citizen");
    expect(activeTunnelOwner(target, [owner])).toBe(owner);
    expect(
      sharedTunnelProjects(project("new-source", otherUuid), [owner])
    ).toEqual([]);
    owner.state.status = "stopped";
    expect(sharedTunnelProjects(target, [owner])).toEqual([]);
    expect(
      sharedTunnelProjects(project("new-source", otherUuid), [owner])
    ).toEqual([owner]);
  });

  it("ignores a rejected project's stale conflict diagnostic after the owner stops", () => {
    const owner = project("example");
    const target = project("citizen", uuid, {
      tunnel_conflict: {
        project_id: owner.id,
        project_name: owner.name,
        tunnel_id: uuid,
      },
    });
    expect(activeTunnelOwner(target, [owner])).toBeUndefined();
    expect(activeTunnelOwner(project("third"), [target])).toBeUndefined();
  });
});
