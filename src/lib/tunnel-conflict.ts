import { isOn } from "./format";
import type { Tunnel } from "./types";

function identity(tunnel: Tunnel): string | null | undefined {
  if (tunnel.kind === "quick") return null;
  // A running connector still owns its loaded identity after the source file is edited.
  return (isOn(tunnel) ? tunnel.state.cloudflare_tunnel_id : null) ?? tunnel.cloudflare_tunnel_id;
}

export function sharedTunnelProjects(tunnel: Tunnel, all: Tunnel[]): Tunnel[] {
  const id = identity(tunnel);
  return id ? all.filter((other) => other.id !== tunnel.id && identity(other) === id) : [];
}

export function activeTunnelOwner(tunnel: Tunnel, all: Tunnel[]): Tunnel | undefined {
  return sharedTunnelProjects(tunnel, all).find(isOn);
}
