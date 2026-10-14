import { describe, expect, it } from "vitest";
import {
  formatAt,
  formatBytes,
  formatDuration,
  hexDump,
  jsonTokens,
  prettyJson,
  publicUrl,
  reachSummary,
  serviceLabel,
  shortService,
  toCurl,
} from "./format";
import { logFields } from "./logfields";

describe("format", () => {
  it("labels services", () => {
    expect(serviceLabel("http_status:404")).toBe("Responds 404");
    expect(serviceLabel("hello_world")).toBe("Hello world page");
    expect(serviceLabel("http://localhost:4000/")).toBe("localhost:4000");
  });
  it("summarises local service reachability", () => {
    expect(reachSummary(1, 1)).toBe("Local service running");
    expect(reachSummary(0, 1)).toBe("Local service not running");
    expect(reachSummary(3, 3)).toBe("All 3 hostnames reachable");
    expect(reachSummary(0, 2)).toBe("None of 2 hostnames reachable");
    expect(reachSummary(2, 3)).toBe("2 of 3 hostnames reachable");
    expect(reachSummary(1, 4, false)).toBe(
      "Local service running for 1 of 4 hostnames"
    );
    expect(reachSummary(1, 1, false)).toBe("Local service running");
  });
  it("formats sizes", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(1024 * 1024 * 20)).toBe("20 MB");
  });
  it("puts the date in front of a request's time when it isn't today", () => {
    const now = new Date(2026, 9, 12, 13, 30).getTime();
    expect(formatAt(new Date(2026, 9, 12, 0, 0, 5).getTime(), now)).toBe(
      "00:00:05"
    );
    expect(formatAt(new Date(2026, 9, 11, 23, 59, 59).getTime(), now)).toBe(
      "Oct 11 23:59:59"
    );
    expect(formatAt(new Date(2025, 11, 31, 8, 4, 0).getTime(), now)).toBe(
      "Dec 31 2025 08:04:00"
    );
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
  it("tokenises pretty JSON for colouring", () => {
    const pretty = prettyJson('{"a":"x\\"y","n":-1.5e3,"b":[true,null]}') ?? "";
    const toks = jsonTokens(pretty);
    expect(toks.map((t) => t.text).join("")).toBe(pretty);
    expect(
      toks.filter((t) => t.kind !== "punct").map((t) => [t.kind, t.text])
    ).toEqual([
      ["key", '"a"'],
      ["string", '"x\\"y"'],
      ["key", '"n"'],
      ["number", "-1500"],
      ["key", '"b"'],
      ["literal", "true"],
      ["literal", "null"],
    ]);
  });
  it("hex dumps with an ASCII gutter", () => {
    const d = hexDump(new Uint8Array([0x48, 0x69, 0x00, 0xff]));
    expect(d).toBe(`00000000  ${"48 69 00 ff".padEnd(47)}  Hi..`);
  });
  it("picks the public URL", () => {
    const base = {
      kind: "config" as const,
      state: {
        mappings: [
          {
            hostname: "*.example.com",
            inspected: false,
            note: null,
            path: null,
            service: "http://x",
          },
          {
            hostname: "app.example.com",
            inspected: false,
            note: null,
            path: null,
            service: "http://x",
          },
        ],
        public_url: null,
      },
    } as never;
    expect(publicUrl(base)).toBe("https://app.example.com");
    expect(shortService("http://localhost:4000/")).toBe("localhost:4000");
    expect(shortService("http://127.0.0.1:51112", true)).toBe(":51112");
    expect(shortService("https://api.internal:8443", true)).toBe(
      "api.internal:8443"
    );
  });
  it("shell-quotes untrusted methods and handles HEAD", () => {
    const base = {
      host: "h.example.com",
      path: "/",
      req: { size: 0, stored: 0, truncated: false } as never,
      req_headers: [] as [string, string][],
    };
    expect(toCurl({ ...base, method: "X`touch /tmp/pwned`" })).toBe(
      "curl -X 'X`touch /tmp/pwned`' 'https://h.example.com/'"
    );
    expect(toCurl({ ...base, method: "HEAD" })).toBe(
      "curl -I 'https://h.example.com/'"
    );
  });
  it("drops content-encoding when replaying a decoded body", () => {
    const cmd = toCurl({
      host: "h",
      method: "POST",
      path: "/",
      req: {
        is_text: true,
        size: 2,
        stored: 2,
        text: "{}",
        truncated: false,
      } as never,
      req_headers: [
        ["Content-Encoding", "gzip"],
        ["Content-Type", "application/json"],
      ],
    });
    expect(cmd).not.toContain("gzip");
  });
  it("builds curl commands with quoting and without edge headers", () => {
    const cmd = toCurl({
      host: "x.trycloudflare.com",
      method: "POST",
      path: "/api?q=1",
      req: { is_text: true, stored: 11, text: `{"a":"it's"}` } as never,
      req_headers: [
        ["Content-Type", "application/json"],
        ["Cf-Ray", "abc"],
        ["Authorization", "[redacted]"],
      ],
    });
    expect(cmd).toBe(
      `curl -X 'POST' 'https://x.trycloudflare.com/api?q=1' -H 'Content-Type: application/json' -H 'Authorization: [redacted]' --data-raw '{"a":"it'\\''s"}'`
    );
  });
  it("reads a log line's fields only from a non-empty JSON object", () => {
    expect(logFields('{"connIndex":0,"event":"x","n":{"a":1}}')).toEqual([
      ["connIndex", "0"],
      ["event", "x"],
      ["n", '{"a":1}'],
    ]);
    for (const bad of [
      "{}",
      "",
      "not json",
      "{",
      "[1,2]",
      "null",
      "3",
      '"s"',
    ]) {
      expect(logFields(bad)).toEqual([]);
    }
  });
});
