import { describe, expect, it } from "vitest";
import { groupTunnels } from "./tunnel-groups";
import { preserveConfigMappings } from "./tunnel-state";
import type { Tunnel, TunnelState } from "./types";

function project(id: string, state: Partial<TunnelState> = {}): Tunnel {
  return {
    autostart: false,
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

describe("grouped tunnel presentation", () => {
  it("includes empty named groups in sidebar order and only includes a populated ungrouped bucket", () => {
    const grouped = { ...project("web"), group_id: "work", group_name: "Work" };
    const named = [
      { id: "empty", name: "Personal" },
      { id: "work", name: "Work" },
    ];
    const groups = groupTunnels([grouped, project("scratch")], named);
    expect(groups.map((group) => group.id)).toEqual(["empty", "work", null]);
    expect(groups[0].tunnels).toEqual([]);
    expect(
      groups.flatMap((group) => group.tunnels).map((tunnel) => tunnel.id)
    ).toEqual(["web", "scratch"]);
    expect(groupTunnels([], named).map((group) => group.id)).toEqual([
      "empty",
      "work",
    ]);
  });
  it("keeps backend ordering and separates identical group names by identity", () => {
    const first = { ...project("first"), group_id: "b", group_name: "Same" };
    const ungrouped = project("ungrouped");
    const second = { ...project("second"), group_id: "a", group_name: "Same" };
    const later = { ...project("later"), group_id: "b", group_name: "Same" };
    const groups = groupTunnels([first, ungrouped, second, later]);
    expect(groups.map((g) => g.id)).toEqual(["b", null, "a"]);
    expect(groups[0].tunnels).toEqual([first, later]);
    expect(groups[1].tunnels).toEqual([ungrouped]);
    expect(groupTunnels([])).toEqual([]);
  });
});

describe("stopped tunnel route presentation", () => {
  const mapping = {
    hostname: "app.test",
    inspected: true,
    note: null,
    path: null,
    service: "http://localhost:3000",
  };
  it("retains config routes when a stop event drops mappings, without mutating the event", () => {
    const config = project("config", {
      mappings: [mapping],
      status: "connected",
    });
    const event = project("config").state;
    const applied = preserveConfigMappings(config, event);
    expect(applied.status).toBe("stopped");
    expect(applied.mappings).toEqual([mapping]);
    expect(event.mappings).toEqual([]);
  });
  it("accepts fresh routes and clears routes for quick/token tunnel events", () => {
    const config = project("config", { mappings: [mapping] });
    const event = {
      ...config.state,
      mappings: [{ ...mapping, hostname: "new.test" }],
    };
    expect(preserveConfigMappings(config, event)).toBe(event);
    const cleared = project("config").state;
    expect(preserveConfigMappings({ ...config, kind: "quick" }, cleared)).toBe(
      cleared
    );
    expect(preserveConfigMappings({ ...config, kind: "token" }, cleared)).toBe(
      cleared
    );
  });
});
