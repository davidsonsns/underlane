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

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/** A request's time of day, with its date in front when it isn't today (and the year when it
 *  isn't this year): `13:06:43`, `Oct 11 13:06:43`, `Dec 31 2025 23:59:59`. */
export function formatAt(ts: number, now = Date.now()): string {
  const d = new Date(ts);
  const n = new Date(now);
  const time = formatClock(ts).slice(0, 8);
  if (d.toDateString() === n.toDateString()) return time;
  const day = `${MONTHS[d.getMonth()]} ${d.getDate()}`;
  return d.getFullYear() === n.getFullYear() ? `${day} ${time}` : `${day} ${d.getFullYear()} ${time}`;
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
export function shortService(service: string, compact = false): string {
  const s = service.replace(/^https?:\/\//, "").replace(/\/$/, "");
  // In tight spaces a local origin is just its port.
  if (compact) {
    const m = s.match(/^(?:localhost|127\.0\.0\.1|\[::1\]):(\d+)$/);
    if (m) return `:${m[1]}`;
  }
  return s;
}

/** What a route's service does, in words for the non-address services cloudflared supports. */
export function serviceLabel(service: string): string {
  const status = service.match(/^http_status:(\d{3})$/);
  if (status) return `Responds ${status[1]}`;
  if (service === "hello_world") return "Hello world page";
  return shortService(service);
}

/** Heading summary for the probe results of a tunnel's routes (one per probed route).
 *  A stopped tunnel reaches nothing, so it only reports the local services. */
export function reachSummary(up: number, total: number, live = true): string {
  if (total === 1) return up ? "Local service running" : "Local service not running";
  if (!live) return `Local service running for ${up} of ${total} routes`;
  if (up === total) return `All ${total} routes reachable`;
  if (up === 0) return `None of ${total} routes reachable`;
  return `${up} of ${total} routes reachable`;
}

/** Public URL a user most likely wants: the quick-tunnel URL or the first concrete hostname. */
/** The tunnel is under supervision: starting, up, or crashed-and-retrying (so Stop applies). */
export function isOn(t: Pick<Tunnel, "state">): boolean {
  return t.state.supervised || ["starting", "connected", "degraded"].includes(t.state.status);
}

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

export type JsonToken = { kind: "key" | "string" | "number" | "literal" | "punct"; text: string };

/** Split prettyJson output into tokens for syntax colouring; joined back, they equal the input. */
export function jsonTokens(pretty: string): JsonToken[] {
  const out: JsonToken[] = [];
  const re = /("(?:[^"\\]|\\.)*")(?=:)|("(?:[^"\\]|\\.)*")|(-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)|(true|false|null)/g;
  let last = 0;
  for (const m of pretty.matchAll(re)) {
    if (m.index > last) out.push({ kind: "punct", text: pretty.slice(last, m.index) });
    const kind = m[1] ? "key" : m[2] ? "string" : m[3] ? "number" : "literal";
    out.push({ kind, text: m[0] });
    last = m.index + m[0].length;
  }
  if (last < pretty.length) out.push({ kind: "punct", text: pretty.slice(last) });
  return out;
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
  "host", "content-length", "connection", "accept-encoding", "content-encoding", "transfer-encoding", "keep-alive", "te", "upgrade", "cdn-loop", "cf-connecting-ip", "cf-ew-via", "cf-ipcountry",
  "cf-ray", "cf-visitor", "cf-warp-tag-id", "cf-worker", "x-forwarded-for", "x-forwarded-proto",
]);

/** Rebuild a request as a curl command against its public URL. Redacted values stay redacted;
 * every captured value is shell-quoted (methods and headers come from untrusted clients). */
export function toCurl(r: Pick<RequestDetail, "method" | "host" | "path" | "req_headers" | "req">): string {
  const parts = ["curl"];
  if (r.method === "HEAD") parts.push("-I");
  else if (r.method !== "GET") parts.push("-X", shellQuote(r.method));
  parts.push(shellQuote(`https://${r.host}${r.path}`));
  for (const [k, v] of r.req_headers) {
    if (SKIP_CURL_HEADERS.has(k.toLowerCase())) continue;
    parts.push("-H", shellQuote(`${k}: ${v}`));
  }
  if (r.req.stored > 0) {
    if (r.req.is_text && r.req.text != null) parts.push("--data-raw", shellQuote(r.req.text));
    else parts.push("--data-binary", "@body.bin");
  }
  const cmd = parts.join(" ");
  return r.req.truncated ? `# Request body was truncated to ${r.req.stored} of ${r.req.size} bytes\n${cmd}` : cmd;
}

export function statusClass(code: number | null): "ok" | "redirect" | "client" | "server" | "none" {
  if (code == null) return "none";
  if (code >= 500) return "server";
  if (code >= 400) return "client";
  if (code >= 300) return "redirect";
  return "ok";
}

/** The text colour of a method, shared by the Requests list and the detail pane. */
export const methodText = (m: string) =>
  m === "DELETE"
    ? "text-bad-text"
    : m === "POST" || m === "PUT" || m === "PATCH"
      ? // A darkened accent, as before the port (the raw accent is under 4.5:1 on white).
        "text-[color-mix(in_srgb,var(--tint)_80%,var(--foreground))]"
      : "text-muted-foreground";

export const statusLabel: Record<string, string> = {
  stopped: "Stopped",
  starting: "Starting",
  connected: "Connected",
  degraded: "Degraded",
  crashed: "Crashed",
};

/** What each tunnel status means, for the status dot's tooltip. */
export const statusHint: Record<string, string> = {
  stopped: "Stopped. cloudflared isn't running, so no traffic reaches these routes.",
  starting: "Starting. cloudflared is connecting to Cloudflare.",
  connected: "Connected. Cloudflare is routing traffic through this tunnel.",
  degraded: "Degraded. Lost the connection to Cloudflare; cloudflared is reconnecting.",
  crashed: "Crashed. cloudflared exited unexpectedly. Check Logs for the error.",
};
