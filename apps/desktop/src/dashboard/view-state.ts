import { shortService } from "$lib/format";

interface InspectionView {
  inspect: boolean;
  state: { inspecting: boolean; status: string; public_url: string | null };
}
export function inspectionSubtitle(
  tunnel: InspectionView | null,
  paused: boolean
): string {
  if (paused) {
    return "Capture paused";
  }
  if (!tunnel) {
    return "";
  }
  if (!tunnel.inspect) {
    return "Inspection is off for this tunnel";
  }
  return tunnel.state.inspecting
    ? "Inspecting"
    : "Inspection on, starts with the tunnel";
}
export function requestEmptyMessage(
  tunnel: InspectionView | null,
  filtered: boolean,
  paused: boolean
): string {
  if (filtered) {
    return "Try a different filter.";
  }
  if (paused) {
    return "Resume capture from Columns and actions to record new requests.";
  }
  if (tunnel?.state.status === "connected") {
    return `Requests to ${tunnel.state.public_url ?? "this tunnel"} will appear here as they happen.`;
  }
  return "Start the tunnel, then send it some traffic.";
}
export type Reach = "up" | "ready" | "down" | "idle" | "none";
export function routeReach(
  reachable: boolean | null | undefined,
  live: boolean
): Reach {
  if (typeof reachable !== "boolean") {
    return "none";
  }
  if (reachable) {
    return live ? "up" : "ready";
  }
  return live ? "down" : "idle";
}
export function routeReachTip(
  service: string,
  reach: Reach
): string | undefined {
  const address = shortService(service);
  const messages = {
    down: `Nothing is listening on ${address}. Requests routed here get a 502 error until it starts.`,
    idle: `Nothing is listening on ${address}. Start it before starting the tunnel.`,
    none: undefined,
    ready: `Something is listening on ${address}, but the tunnel is stopped. Start it to route traffic here. Underlane doesn't manage this service.`,
    up: `Local service reachable at ${address}. Underlane doesn't manage this service.`,
  };
  return messages[reach];
}
