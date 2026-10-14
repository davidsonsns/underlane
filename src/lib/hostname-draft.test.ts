import { describe, expect, it } from "vitest";
import { hostnameChanged, hostnamePayload } from "./hostname-draft";
import type { HostnameConfig } from "./types";

const config: HostnameConfig = {
  fallback_service: "http_status:404",
  revision: "revision",
  routes: [
    {
      hostname: "example.com",
      index: 0,
      path: "/api/.*",
      service: "https://localhost:3000",
    },
    {
      hostname: "example.com",
      index: 1,
      path: null,
      service: "tcp://localhost:5432",
    },
  ],
};

describe("hostname drafts", () => {
  it("preserves indices, order, paths and service schemes without changing a loaded draft", () => {
    const payload = hostnamePayload(config.routes, true, "");
    expect(payload.routes).toEqual(config.routes);
    expect(hostnameChanged(config, payload)).toBe(false);
  });

  it("stages removal, edits and new routes without mutating the loaded config", () => {
    const payload = hostnamePayload(
      [
        { ...config.routes[1], service: " tcp://localhost:5433 " },
        {
          hostname: " new.example.com ",
          index: null,
          path: null,
          service: " localhost:4000 ",
        },
      ],
      true,
      ""
    );
    expect(payload.routes).toEqual([
      {
        hostname: "example.com",
        index: 1,
        path: null,
        service: "tcp://localhost:5433",
      },
      {
        hostname: "new.example.com",
        index: null,
        path: null,
        service: "localhost:4000",
      },
    ]);
    expect(hostnameChanged(config, payload)).toBe(true);
    expect(config.routes).toHaveLength(2);
    expect(config.routes[1].service).toBe("tcp://localhost:5432");
  });

  it("always sends an explicit fallback and preserves custom text across checkbox toggles", () => {
    const custom = " https://fallback.example.com ";
    expect(hostnamePayload([], false, custom).fallbackService).toBe(
      "https://fallback.example.com"
    );
    expect(hostnamePayload([], true, custom).fallbackService).toBe(
      "http_status:404"
    );
    expect(hostnamePayload([], false, custom).fallbackService).toBe(
      "https://fallback.example.com"
    );
    expect(hostnamePayload([], false, "").fallbackService).toBe("");
  });
});
