import { next } from "./listnav";

const NAV_KEYS = new Set(["ArrowDown", "ArrowUp", "Home", "End"]);

export function popoverKeyAction(
  event: Pick<KeyboardEvent, "key" | "metaKey" | "ctrlKey" | "altKey">
): "hide" | "navigate" | null {
  if (event.key === "Escape") {
    return "hide";
  }
  if (event.metaKey || event.ctrlKey || event.altKey) {
    return null;
  }
  return NAV_KEYS.has(event.key) ? "navigate" : null;
}

export function popoverRowIndex(
  key: string,
  currentIndex: number,
  length: number
): number | null {
  if (currentIndex < 0 && key === "ArrowUp") {
    return length - 1;
  }
  return next(key, currentIndex, length);
}

export function tunnelSummary(running: number, starting: number): string {
  const parts: string[] = [];
  if (running) {
    parts.push(`${running} running`);
  }
  if (starting) {
    parts.push(`${starting} starting`);
  }
  return parts.join(", ") || "No tunnels running";
}
