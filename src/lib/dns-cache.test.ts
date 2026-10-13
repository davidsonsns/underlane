import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { HostnameDns } from "./types";

const KEY = JSON.stringify(["project", "app.example.com"]);
const STORAGE_KEY = "underlane.dns." + KEY;
const NOW = 1_000_000_000;
const RESULT: HostnameDns = {
  status: "connected",
  message: "Cloudflare confirms this hostname.",
  can_create: false,
  needs_dns_access: false,
};
let values: Map<string, string>;

beforeEach(() => {
  vi.resetModules();
  values = new Map();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => values.set(key, value),
  });
});
afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("DNS cache", () => {
  it("checks at most once per 24 hours, including the exact boundary", async () => {
    const cache = await import("./dns-cache");
    expect(cache.dnsCheckDue(KEY, NOW)).toBe(true);
    cache.markDnsCheck(KEY, NOW);
    expect(cache.dnsCheckDue(KEY, NOW + cache.DNS_CHECK_INTERVAL_MS - 1)).toBe(false);
    expect(cache.dnsCheckDue(KEY, NOW + cache.DNS_CHECK_INTERVAL_MS)).toBe(true);
    expect(cache.dnsCheckDue("another hostname", NOW)).toBe(true);
  });

  it("preserves attempt time when a result arrives and reloads it after restart", async () => {
    let cache = await import("./dns-cache");
    cache.markDnsCheck(KEY, NOW);
    vi.spyOn(Date, "now").mockReturnValue(NOW + 5_000);
    cache.saveDnsResult(KEY, RESULT);
    vi.resetModules();
    cache = await import("./dns-cache");
    expect(cache.readDnsCache(KEY)).toEqual(RESULT);
    expect(cache.dnsCheckDue(KEY, NOW + cache.DNS_CHECK_INTERVAL_MS - 1)).toBe(false);
    expect(cache.dnsCheckDue(KEY, NOW + cache.DNS_CHECK_INTERVAL_MS)).toBe(true);
  });

  it("a manual check reserves a new day while retaining the previous result", async () => {
    const cache = await import("./dns-cache");
    cache.markDnsCheck(KEY, NOW);
    cache.saveDnsResult(KEY, RESULT);
    const manual = NOW + 60_000;
    expect(cache.dnsCheckDue(KEY, manual)).toBe(false);
    cache.markDnsCheck(KEY, manual);
    expect(cache.readDnsCache(KEY)).toEqual(RESULT);
    expect(cache.dnsCheckDue(KEY, NOW + cache.DNS_CHECK_INTERVAL_MS)).toBe(false);
    expect(cache.dnsCheckDue(KEY, manual + cache.DNS_CHECK_INTERVAL_MS)).toBe(true);
  });

  it("failed attempts reserve the interval even without a status and across restarts", async () => {
    let cache = await import("./dns-cache");
    cache.markDnsCheck(KEY, NOW);
    vi.resetModules();
    cache = await import("./dns-cache");
    expect(cache.readDnsCache(KEY)).toBeNull();
    expect(cache.dnsCheckDue(KEY, NOW + 1)).toBe(false);
    expect(cache.dnsCheckDue(KEY, NOW + cache.DNS_CHECK_INTERVAL_MS)).toBe(true);
  });

  it("ignores a stale completion after a newer check reserved the same hostname", async () => {
    const cache = await import("./dns-cache");
    cache.markDnsCheck(KEY, NOW);
    cache.markDnsCheck(KEY, NOW + 1);
    cache.saveDnsResult(KEY, RESULT, NOW + 1);
    cache.saveDnsResult(KEY, { ...RESULT, status: "unknown", message: "Older failed check" }, NOW);
    expect(cache.readDnsCache(KEY)).toEqual(RESULT);
    expect(cache.dnsCheckDue(KEY, NOW + cache.DNS_CHECK_INTERVAL_MS)).toBe(false);
    cache.saveDnsResult("unreserved", RESULT, NOW);
    expect(cache.readDnsCache("unreserved")).toBeNull();
  });

  it("ignores corrupt persisted entries instead of trusting their status or timestamp", async () => {
    for (const value of [
      "invalid json",
      "null",
      JSON.stringify({ lastAttempt: -1, result: RESULT }),
      '{"lastAttempt":1e400,"result":null}',
      JSON.stringify({ lastAttempt: "1000", result: RESULT }),
      JSON.stringify({ lastAttempt: NOW, result: { ...RESULT, status: "broken" } }),
      JSON.stringify({ lastAttempt: NOW, result: { ...RESULT, message: 2 } }),
      JSON.stringify({ lastAttempt: NOW, result: { ...RESULT, can_create: "false" } }),
      JSON.stringify({ lastAttempt: NOW, result: { ...RESULT, needs_dns_access: null } }),
      JSON.stringify({ lastAttempt: NOW }),
    ]) {
      values.set(STORAGE_KEY, value);
      vi.resetModules();
      const cache = await import("./dns-cache");
      expect(cache.readDnsCache(KEY)).toBeNull();
      expect(cache.dnsCheckDue(KEY, NOW)).toBe(true);
    }
  });

  it("keeps the reservation and status in memory if storage is unavailable", async () => {
    vi.stubGlobal("localStorage", {
      getItem: () => { throw new Error("unavailable"); },
      setItem: () => { throw new Error("unavailable"); },
    });
    const cache = await import("./dns-cache");
    cache.markDnsCheck(KEY, NOW);
    cache.saveDnsResult(KEY, RESULT);
    expect(cache.readDnsCache(KEY)).toEqual(RESULT);
    expect(cache.dnsCheckDue(KEY, NOW + 1)).toBe(false);
    expect(cache.dnsCheckDue(KEY, NOW + cache.DNS_CHECK_INTERVAL_MS)).toBe(true);
  });

  it("suppresses automatic retries when the clock moves behind the last attempt", async () => {
    const cache = await import("./dns-cache");
    cache.markDnsCheck(KEY, NOW);
    expect(cache.dnsCheckDue(KEY, NOW - cache.DNS_CHECK_INTERVAL_MS)).toBe(false);
    expect(cache.dnsCheckDue(KEY, NOW)).toBe(false);
  });

  it("persists only DNS status fields", async () => {
    const cache = await import("./dns-cache");
    const result = { ...RESULT, api_token: "secret" };
    cache.markDnsCheck(KEY, NOW);
    cache.saveDnsResult(KEY, result);
    expect(values.get(STORAGE_KEY)).not.toContain("secret");
    expect(cache.readDnsCache(KEY)).toEqual(RESULT);
  });
});
