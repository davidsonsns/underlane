import type { HostnameDns } from "./types";

export const DNS_CHECK_INTERVAL_MS = 24 * 60 * 60 * 1000;

interface Entry {
  lastAttempt: number;
  result: HostnameDns | null;
}

const PREFIX = "underlane.dns.";
const memory = new Map<string, Entry>();

export function clearDnsCache(): void {
  memory.clear();
}

function validDnsStatus(value: unknown): value is HostnameDns["status"] {
  return (
    typeof value === "string" &&
    ["connected", "missing", "conflict", "unknown"].includes(value)
  );
}

function dnsResult(value: unknown): HostnameDns | null {
  if (!value || typeof value !== "object") {
    return null;
  }
  const result = value as Record<string, unknown>;
  if (
    !validDnsStatus(result.status) ||
    typeof result.message !== "string" ||
    typeof result.can_create !== "boolean" ||
    typeof result.needs_dns_access !== "boolean"
  ) {
    return null;
  }
  return {
    can_create: result.can_create,
    message: result.message,
    needs_dns_access: result.needs_dns_access,
    status: result.status,
  };
}

function validAttempt(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value) && value >= 0;
}

function parseEntry(value: unknown): Entry | null {
  if (!value || typeof value !== "object") {
    return null;
  }
  const entry = value as Record<string, unknown>;
  if (!validAttempt(entry.lastAttempt)) {
    return null;
  }
  const result = dnsResult(entry.result);
  if (entry.result !== null && result === null) {
    return null;
  }
  return { lastAttempt: entry.lastAttempt, result };
}

function load(key: string): Entry | null {
  const cached = memory.get(key);
  if (cached) {
    return cached;
  }
  try {
    const parsed = parseEntry(
      JSON.parse(localStorage.getItem(PREFIX + key) ?? "null")
    );
    if (parsed) {
      memory.set(key, parsed);
    }
    return parsed;
  } catch {
    // Ignore corrupt or unavailable storage and allow a fresh DNS check.
    return null;
  }
}

function store(key: string, entry: Entry): void {
  memory.set(key, entry);
  try {
    localStorage.setItem(PREFIX + key, JSON.stringify(entry));
  } catch {
    // Storage may be unavailable; keep the in-memory value.
  }
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

export function saveDnsResult(
  key: string,
  result: HostnameDns,
  expectedLastAttempt?: number
): void {
  const entry = load(key);
  if (
    expectedLastAttempt !== undefined &&
    entry?.lastAttempt !== expectedLastAttempt
  ) {
    return;
  }
  store(key, {
    lastAttempt: entry?.lastAttempt ?? Date.now(),
    result: dnsResult(result),
  });
}
