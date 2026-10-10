import { describe, expect, it } from "vitest";
import { formatBytes, formatDuration, hexDump, prettyJson, publicUrl, shortService, toCurl } from "./format";

describe("format", () => {
  it("formats sizes", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(1024 * 1024 * 20)).toBe("20 MB");
  });
  it("formats durations", () => {
    expect(formatDuration(null)).toBe("open");
    expect(formatDuration(42.4)).toBe("42 ms");
    expect(formatDuration(1234)).toBe("1.23 s");
  });
  it("pretty-prints JSON only when it is JSON", () => {
    expect(prettyJson('{"a":1}')).toBe('{\n  "a": 1\n}');
    expect(prettyJson("hello")).toBeNull();
    expect(prettyJson("{broken")).toBeNull();
  });
  it("hex dumps with an ASCII gutter", () => {
    const d = hexDump(new Uint8Array([0x48, 0x69, 0x00, 0xff]));
    expect(d).toBe(`00000000  ${"48 69 00 ff".padEnd(47)}  Hi..`);
  });
  it("picks the public URL", () => {
    const base = { kind: "config" as const, state: { public_url: null, mappings: [
      { hostname: "*.example.com", path: null, service: "http://x", inspected: false, note: null },
      { hostname: "app.example.com", path: null, service: "http://x", inspected: false, note: null },
    ] } } as never;
    expect(publicUrl(base)).toBe("https://app.example.com");
    expect(shortService("http://localhost:4000/")).toBe("localhost:4000");
    expect(shortService("http://127.0.0.1:51112", true)).toBe(":51112");
    expect(shortService("https://api.internal:8443", true)).toBe("api.internal:8443");
  });
  it("builds curl commands with quoting and without edge headers", () => {
    const cmd = toCurl({
      method: "POST",
      host: "x.trycloudflare.com",
      path: "/api?q=1",
      req_headers: [["Content-Type", "application/json"], ["Cf-Ray", "abc"], ["Authorization", "[redacted]"]],
      req: { stored: 11, is_text: true, text: `{"a":"it's"}` } as never,
    });
    expect(cmd).toBe(
      `curl -X POST 'https://x.trycloudflare.com/api?q=1' -H 'Content-Type: application/json' -H 'Authorization: [redacted]' --data-raw '{"a":"it'\\''s"}'`,
    );
  });
});
