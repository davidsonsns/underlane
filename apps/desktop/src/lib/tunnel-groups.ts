import type { Tunnel } from "./types";

interface TunnelGroup {
  id: string | null;
  name: string;
  tunnels: Tunnel[];
}

/** Keep named groups in sidebar order, including empty groups, then remaining buckets. */
export function groupTunnels(
  tunnels: Tunnel[],
  namedGroups: readonly { id: string; name: string }[] = []
): TunnelGroup[] {
  const groups = new Map<string | null, TunnelGroup>();
  for (const tunnel of tunnels) {
    const group = groups.get(tunnel.group_id) ?? {
      id: tunnel.group_id,
      name: tunnel.group_id === null ? "Ungrouped" : tunnel.group_name,
      tunnels: [],
    };
    group.tunnels.push(tunnel);
    groups.set(tunnel.group_id, group);
  }
  const ordered = namedGroups.map((group) => ({
    id: group.id,
    name: group.name,
    tunnels: groups.get(group.id)?.tunnels ?? [],
  }));
  const known = new Set(namedGroups.map((group) => group.id));
  return [
    ...ordered,
    ...[...groups.values()].filter(
      (group) => group.id === null || !known.has(group.id)
    ),
  ];
}
