import type { Tunnel, TunnelState } from "./types";

/** A stopped config tunnel continues showing routes parsed from its file. */
export function preserveConfigMappings(
  tunnel: Tunnel,
  state: TunnelState
): TunnelState {
  if (tunnel.kind !== "config") {
    return state;
  }
  if (state.mappings.length > 0) {
    return state;
  }
  return { ...state, mappings: tunnel.state.mappings };
}
