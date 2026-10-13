import type { HostnameDns } from "./types";

export const DNS_CHECK_INTERVAL_MS = 24 * 60 * 60 * 1000;

interface Entry {
  lastAttempt: number;
  result: HostnameDns | null;
}

const PREFIX = "underlane.dns.";
const memory = new Map<string, Entry>();

function dnsResult(value: unknown): HostnameDns | null {
  if (!value || typeof value !== "object") return null;
  const result = value as Record<string, unknown>;
  if (
    (result.status !== "connected" && result.status !== "missing" && result.status !== "conflict" && result.status !== "unknown") ||
    typeof result.message !== "string" ||
    typeof result.can_create !== "boolean" ||
    typeof result.needs_dns_access !== "boolean"
  ) return null;
  return {
    status: result.status,
    message: result.message,
    can_create: result.can_create,
    needs_dns_access: result.needs_dns_access,
  };
}

function load(key: string): Entry | null {
  const cached = memory.get(key);
  if (cached) return cached;
  try {
    const value: unknown = JSON.parse(localStorage.getItem(PREFIX + key) ?? "null");
    if (!value || typeof value !== "object") return null;
    const entry = value as Record<string, unknown>;
    if (typeof entry.lastAttempt !== "number" || !Number.isFinite(entry.lastAttempt) || entry.lastAttempt < 0) return null;
    const result = dnsResult(entry.result);
    if (entry.result !== null && result === null) return null;
    const parsed = { lastAttempt: entry.lastAttempt, result };
    memory.set(key, parsed);
    return parsed;
  } catch {}
  return null;
}

function store(key: string, entry: Entry): void {
  memory.set(key, entry);
  try {
    localStorage.setItem(PREFIX + key, JSON.stringify(entry));
  } catch {}
}

export function readDnsCache(key: string): HostnameDns | null {
  return load(key)?.result ?? null;
}

export function dnsCheckDue(key: string, now = Date.now()): boolean {
  const entry = load(key);
  // A clock rollback keeps the reservation until the recorded attempt is a full day old.
  return entry === null || now - entry.lastAttempt >= DNS_CHECK_INTERVAL_MS;
}

export function markDnsCheck(key: string, now = Date.now()): void {
  store(key, { lastAttempt: now, result: readDnsCache(key) });
}

export function saveDnsResult(key: string, result: HostnameDns, expectedLastAttempt?: number): void {
  const entry = load(key);
  if (expectedLastAttempt !== undefined && entry?.lastAttempt !== expectedLastAttempt) return;
  store(key, { lastAttempt: entry?.lastAttempt ?? Date.now(), result: dnsResult(result) });
}
