// Pure formatting helpers (unit-tested in format.test.ts).
import type { RequestDetail, RouteMapping, Tunnel } from "./types";

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  const units = ["KB", "MB", "GB"];
  let v = n / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v < 10 ? v.toFixed(1) : Math.round(v)} ${units[i]}`;
}

export function formatDuration(ms: number | null): string {
  if (ms == null) return "open";
  if (ms < 1) return "<1 ms";
  if (ms < 1000) return `${Math.round(ms)} ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(ms < 10_000 ? 2 : 1)} s`;
  const m = Math.floor(ms / 60_000);
  return `${m}m ${Math.round((ms % 60_000) / 1000)}s`;
}

export function formatClock(ts: number): string {
  const d = new Date(ts);
  const p = (n: number, w = 2) => String(n).padStart(w, "0");
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}.${p(d.getMilliseconds(), 3)}`;
}

export function formatUptime(startedAt: number | null, now = Date.now()): string {
  if (!startedAt) return "";
  const s = Math.max(0, Math.floor((now - startedAt) / 1000));
  if (s < 60) return `${s}s`;
  if (s < 3600) return `${Math.floor(s / 60)}m`;
  if (s < 86400) return `${Math.floor(s / 3600)}h ${Math.floor((s % 3600) / 60)}m`;
  return `${Math.floor(s / 86400)}d ${Math.floor((s % 86400) / 3600)}h`;
}

/** `http://localhost:4000` -> `localhost:4000`; keeps other schemes visible. */
export function shortService(service: string): string {
  return service.replace(/^https?:\/\//, "").replace(/\/$/, "");
}

/** Public URL a user most likely wants: the quick-tunnel URL or the first concrete hostname. */
export function publicUrl(t: Pick<Tunnel, "state" | "kind">): string | null {
  if (t.state.public_url) return t.state.public_url;
  const host = t.state.mappings.find((m: RouteMapping) => m.hostname && !m.hostname.includes("*"))?.hostname;
  return host ? `https://${host}` : null;
}

/** Pretty-print JSON; returns null if the text isn't JSON. */
export function prettyJson(text: string): string | null {
  const t = text.trim();
  if (!t || !"{[".includes(t[0])) return null;
  try {
    return JSON.stringify(JSON.parse(t), null, 2);
  } catch {
    return null;
  }
}

export function base64ToBytes(b64: string): Uint8Array {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

/** Classic 16-bytes-per-row hex dump with an ASCII gutter. */
export function hexDump(bytes: Uint8Array, limit = 4096): string {
  const rows: string[] = [];
  const n = Math.min(bytes.length, limit);
  for (let off = 0; off < n; off += 16) {
    const slice = bytes.subarray(off, Math.min(off + 16, n));
    const hex = Array.from(slice, (b) => b.toString(16).padStart(2, "0")).join(" ");
    const ascii = Array.from(slice, (b) => (b >= 0x20 && b < 0x7f ? String.fromCharCode(b) : ".")).join("");
    rows.push(`${off.toString(16).padStart(8, "0")}  ${hex.padEnd(47, " ")}  ${ascii}`);
  }
  if (bytes.length > limit) rows.push(`… ${bytes.length - limit} more bytes`);
  return rows.join("\n");
}

function shellQuote(s: string): string {
  return `'${s.replace(/'/g, `'\\''`)}'`;
}

const SKIP_CURL_HEADERS = new Set([
  "host", "content-length", "connection", "accept-encoding", "cdn-loop", "cf-connecting-ip", "cf-ew-via", "cf-ipcountry",
  "cf-ray", "cf-visitor", "cf-warp-tag-id", "cf-worker", "x-forwarded-for", "x-forwarded-proto",
]);

/** Rebuild a request as a curl command against its public URL. Redacted values stay redacted. */
export function toCurl(r: Pick<RequestDetail, "method" | "host" | "path" | "req_headers" | "req">): string {
  const parts = ["curl"];
  if (r.method !== "GET") parts.push("-X", r.method);
  parts.push(shellQuote(`https://${r.host}${r.path}`));
  for (const [k, v] of r.req_headers) {
    if (SKIP_CURL_HEADERS.has(k.toLowerCase())) continue;
    parts.push("-H", shellQuote(`${k}: ${v}`));
  }
  if (r.req.stored > 0) {
    if (r.req.is_text && r.req.text != null) parts.push("--data-raw", shellQuote(r.req.text));
    else parts.push("--data-binary", "@body.bin");
  }
  return parts.join(" ");
}

export function statusClass(code: number | null): "ok" | "redirect" | "client" | "server" | "none" {
  if (code == null) return "none";
  if (code >= 500) return "server";
  if (code >= 400) return "client";
  if (code >= 300) return "redirect";
  return "ok";
}

export const statusLabel: Record<string, string> = {
  stopped: "Stopped",
  starting: "Starting",
  connected: "Connected",
  degraded: "Degraded",
  crashed: "Crashed",
};
