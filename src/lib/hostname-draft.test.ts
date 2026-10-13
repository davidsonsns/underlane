import { describe, expect, it } from "vitest";
import { hostnameChanged, hostnamePayload } from "./hostname-draft";
import type { HostnameConfig } from "./types";

const config: HostnameConfig = {
  revision: "revision",
  routes: [
    { index: 0, hostname: "example.com", service: "https://localhost:3000", path: "/api/.*" },
    { index: 1, hostname: "example.com", service: "tcp://localhost:5432", path: null },
  ],
  fallback_service: "http_status:404",
};

describe("hostname drafts", () => {
  it("preserves indices, order, paths and service schemes without changing a loaded draft", () => {
    const payload = hostnamePayload(config.routes, true, "");
    expect(payload.routes).toEqual(config.routes);
    expect(hostnameChanged(config, payload)).toBe(false);
  });

  it("stages removal, edits and new routes without mutating the loaded config", () => {
    const payload = hostnamePayload([
      { ...config.routes[1], service: " tcp://localhost:5433 " },
      { index: null, hostname: " new.example.com ", service: " localhost:4000 ", path: null },
    ], true, "");
    expect(payload.routes).toEqual([
      { index: 1, hostname: "example.com", service: "tcp://localhost:5433", path: null },
      { index: null, hostname: "new.example.com", service: "localhost:4000", path: null },
    ]);
    expect(hostnameChanged(config, payload)).toBe(true);
    expect(config.routes).toHaveLength(2);
    expect(config.routes[1].service).toBe("tcp://localhost:5432");
  });

  it("always sends an explicit fallback and preserves custom text across checkbox toggles", () => {
    const custom = " https://fallback.example.com ";
    expect(hostnamePayload([], false, custom).fallbackService).toBe("https://fallback.example.com");
    expect(hostnamePayload([], true, custom).fallbackService).toBe("http_status:404");
    expect(hostnamePayload([], false, custom).fallbackService).toBe("https://fallback.example.com");
    expect(hostnamePayload([], false, "").fallbackService).toBe("");
  });
});
