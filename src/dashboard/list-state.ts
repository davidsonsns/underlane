import { next } from "$lib/listnav";

/** Keep live logs bounded without trimming the user's scrolled view. */
export function appendLogWindow<T>(
  lines: T[],
  held: T[],
  add: T[],
  follow: boolean,
  cap: number,
  ceiling: number
) {
  if (!add.length) {
    return { held, lines };
  }
  if (follow) {
    return { held: [], lines: [...lines, ...held, ...add].slice(-cap) };
  }
  const room = Math.max(0, ceiling - lines.length);
  return {
    held: add.length > room ? [...held, ...add.slice(room)].slice(-cap) : held,
    lines: room ? [...lines, ...add.slice(0, room)] : lines,
  };
}

/** Describe the virtual request window and its anchor before mutating the view. */
export function prependRequestWindow<T extends { id: string }>(
  rows: T[],
  batch: T[],
  top: number | null,
  full: boolean,
  cap: number,
  ceiling: number,
  rowHeight: number
) {
  const ids = new Set(batch.map((row) => row.id));
  const merged = [...batch, ...rows.filter((row) => !ids.has(row.id))];
  if (top === null || full || top <= 0) {
    return {
      anchor: null,
      rows: merged.slice(0, cap),
      shift: false,
      wait: false,
    };
  }
  if (merged.length <= ceiling) {
    return { anchor: null, rows: merged, shift: true, wait: true };
  }
  const i = Math.floor(top / rowHeight);
  const j = merged.findIndex((row) => row.id === rows[i]?.id);
  return {
    anchor: j >= 0 ? j * rowHeight + (top - i * rowHeight) : null,
    rows: merged.slice(0, ceiling),
    shift: false,
    wait: true,
  };
}

type KeyEvent = Pick<KeyboardEvent, "key" | "altKey" | "ctrlKey" | "metaKey">;
const modified = (event: KeyEvent) =>
  [event.altKey, event.ctrlKey, event.metaKey].some(Boolean);
const fieldKeys: Record<string, boolean | undefined> = {
  ArrowLeft: false,
  ArrowRight: true,
  Enter: true,
};
const viewEdges: Record<string, boolean | undefined> = {
  ArrowDown: false,
  ArrowUp: true,
  PageDown: false,
  PageUp: true,
};
type LogAction =
  | { type: "fields"; open: boolean; index: number }
  | { type: "move"; index: number };

/** Resolve treegrid keys without consuming modifiers or unrelated shortcuts. */
export function logKeyAction(
  event: KeyEvent,
  active: number,
  count: number,
  page: number,
  edge: (last: boolean) => number
): LogAction | null {
  if (modified(event)) {
    return null;
  }
  const open = fieldKeys[event.key];
  if (open !== undefined) {
    return active < 0 ? null : { index: active, open, type: "fields" };
  }
  const last = viewEdges[event.key];
  if (active < 0 && last !== undefined) {
    return count ? { index: edge(last), type: "move" } : null;
  }
  const index = next(event.key, active, count, page);
  return index === null ? null : { index, type: "move" };
}

/** Requests clear selection on Escape; other list keys share the clamped navigation. */
export function requestKeyAction(
  event: KeyEvent,
  selected: boolean,
  active: number,
  count: number,
  page: number
): { type: "clear" } | { type: "move"; index: number } | null {
  if (modified(event)) {
    return null;
  }
  if (event.key === "Escape" && selected) {
    return { type: "clear" };
  }
  const index = next(event.key, active, count, page);
  return index === null ? null : { index, type: "move" };
}

/** Only actual upward movement stops following; virtua's trim jump does not. */
export function scrollFollowState(
  bottom: boolean,
  trimming: boolean,
  up: number,
  lastTop: number,
  top: number
) {
  if (bottom) {
    return { stop: false, up: 0 };
  }
  if (trimming) {
    return { stop: false, up };
  }
  const movement = up + Math.max(0, lastTop - top);
  return { stop: movement > 1, up: movement };
}
