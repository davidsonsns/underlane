import type { Tunnel } from "./types";

interface TunnelGroup {
  id: string | null;
  name: string;
  tunnels: Tunnel[];
}

/** Preserve backend ordering and keep ungrouped tunnels together. */
export function groupTunnels(tunnels: Tunnel[]): TunnelGroup[] {
  const groups = new Map<string | null, TunnelGroup>();
  for (const tunnel of tunnels) {
    const group = groups.get(tunnel.group_id) ?? {
      id: tunnel.group_id,
      name: tunnel.group_name,
      tunnels: [],
    };
    group.tunnels.push(tunnel);
    groups.set(tunnel.group_id, group);
  }
  return [...groups.values()];
}
