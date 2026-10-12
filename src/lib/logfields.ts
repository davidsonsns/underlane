// Kept out of format.ts: format.ts is shared with the popover bundle, which doesn't need this.
/** A log line's structured fields as [key, value] pairs; empty unless `json` is a non-empty object. */
export function logFields(json: string): [string, string][] {
  let o: unknown;
  try {
    o = JSON.parse(json);
  } catch {
    return [];
  }
  if (!o || typeof o !== "object" || Array.isArray(o)) return [];
  return Object.entries(o).map(([k, v]) => [k, typeof v === "string" ? v : JSON.stringify(v)]);
}
