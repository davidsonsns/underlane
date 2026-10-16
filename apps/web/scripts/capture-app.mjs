#!/usr/bin/env node
// Run from the workspace root: pnpm --filter @underlane/web screenshots
// Captures the real desktop Svelte views with synthetic IPC, without account or disk access.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdir } from "node:fs/promises";
import { resolve } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { fileURLToPath } from "node:url";
// biome-ignore lint/correctness/noUndeclaredDependencies: Playwright is a shared workspace-root development dependency.
import { chromium } from "playwright";
import { installPageMock } from "../../../e2e/lib/browser.mjs";

const root = fileURLToPath(new URL("../../../", import.meta.url));
const output = fileURLToPath(
  new URL("../src/assets/screenshots/", import.meta.url)
);
const capturePort = 14_231;
const baseUrl =
  process.env.CAPTURE_BASE_URL ?? `http://127.0.0.1:${capturePort}`;
const server = process.env.CAPTURE_BASE_URL
  ? null
  : spawn(
      process.execPath,
      [
        resolve(root, "apps/desktop/node_modules/vite/bin/vite.js"),
        "--host",
        "127.0.0.1",
        "--port",
        String(capturePort),
        "--strictPort",
      ],
      {
        cwd: resolve(root, "apps/desktop"),
        stdio: "pipe",
      }
    );
let serverLog = "";
server?.stdout.on("data", (chunk) => {
  serverLog += chunk;
});
server?.stderr.on("data", (chunk) => {
  serverLog += chunk;
});

function fixture({ theme }) {
  const now = new Date("2026-10-15T17:42:18Z").getTime();
  const specs = [
    ["storefront", "Commerce", "commerce", 3000, "shop", true, true],
    ["payments-api", "Commerce", "commerce", 8080, "api", true, true],
    ["docs", "Workspace", "workspace", 4321, "docs", true, false],
    ["webhook-preview", "Workspace", "workspace", 8787, "hooks", false, true],
  ];
  const tunnels = specs.map(
    ([name, groupName, group, port, host, running, inspect], index) => {
      const hostname = `${host}.example.com`;
      const tunnelId = `00000000-0000-4000-8000-${String(index + 1).padStart(12, "0")}`;
      const inspecting = running && inspect;
      const processState = running
        ? {
            connections: 4,
            pid: 42_810 + index,
            run: 1,
            started_at: now - 4_622_000,
            status: "connected",
          }
        : {
            connections: 0,
            pid: null,
            run: 0,
            started_at: null,
            status: "stopped",
          };
      return {
        autostart: name === "storefront",
        cloudflare_tunnel_id: tunnelId,
        config_path: `~/Developer/${name}/.cloudflared/config.yml`,
        created_at: now - 86_400_000,
        group_id: group,
        group_name: groupName,
        has_token: false,
        id: name,
        inspect,
        kind: "config",
        loglevel: "info",
        name,
        origin_url: null,
        setup_needed: false,
        sort: index,
        state: {
          cloudflare_tunnel_id: tunnelId,
          config_revision: 1,
          detail: null,
          failure: null,
          hint: null,
          id: name,
          inspecting,
          mappings: [
            {
              hostname,
              inspected: inspecting,
              note: null,
              path: null,
              service: `http://localhost:${port}`,
            },
            ...(name === "storefront"
              ? [
                  {
                    hostname: "preview.example.com",
                    inspected: true,
                    note: null,
                    path: null,
                    service: "http://localhost:3000",
                  },
                ]
              : []),
            {
              hostname: null,
              inspected: false,
              note: null,
              path: null,
              service: "http_status:404",
            },
          ],
          metrics_addr: null,
          public_url: null,
          restarts: 0,
          supervised: running,
          ...processState,
        },
      };
    }
  );
  const overview = {
    active_count: 3,
    cloudflared: {
      path: "/opt/homebrew/bin/cloudflared",
      source: "path",
      version: "2026.9.1",
    },
    data_dir: "~/Library/Application Support/Underlane",
    dropped_captures: 0,
    groups: [
      { id: "commerce", name: "Commerce", sort: 0 },
      { id: "workspace", name: "Workspace", sort: 1 },
    ],
    last_launch: null,
    settings: {
      cloudflared_path: null,
      default_loglevel: "info",
      log_ttl_days: 7,
      max_body_kb: 1024,
      max_db_mb: 100,
      redact_headers: ["authorization", "cookie"],
      redact_query: [],
      theme,
    },
    tunnels,
  };
  const routes = [
    ["POST", "/api/orders", 201, 42, 486],
    ["GET", "/api/products?collection=summer", 200, 18, 3240],
    ["GET", "/api/cart", 200, 12, 842],
    ["POST", "/api/checkout/session", 200, 68, 364],
    ["GET", "/assets/storefront.css", 304, 4, 0],
    ["GET", "/api/products/linen-tote", 200, 16, 1248],
    ["PATCH", "/api/cart/items/2", 200, 24, 921],
    ["GET", "/api/health", 200, 3, 41],
    ["GET", "/api/products/not-found", 404, 8, 128],
    ["GET", "/api/collections", 200, 21, 1820],
    ["POST", "/api/cart/items", 201, 32, 946],
    ["GET", "/api/shipping/options", 200, 27, 680],
  ];
  const requests = routes.map(
    ([method, path, status, duration_ms, resp_size], index) => ({
      duration_ms,
      error: null,
      host: "shop.example.com",
      id: `request-${index}`,
      kind: "http",
      method,
      path,
      resp_size,
      status,
      ts: now - index * 3210,
      tunnel_id: "storefront",
    })
  );
  function body(value) {
    const text = JSON.stringify(value);
    return {
      base64: btoa(text),
      content_type: "application/json",
      decode_partial: false,
      decoded_from: null,
      is_text: true,
      size: text.length,
      stored: text.length,
      text,
      truncated: false,
    };
  }
  const detail = {
    ...requests[0],
    req: body({
      currency: "USD",
      items: [{ product: "linen-tote", quantity: 2 }],
      shipping: "standard",
    }),
    req_headers: [
      ["content-type", "application/json"],
      ["accept", "application/json"],
      ["x-request-id", "req_8f24c1"],
    ],
    resp: body({
      checkout_url: "/orders/1042",
      currency: "USD",
      id: "ord_2026_1042",
      items: [{ price: 24, product: "linen-tote", quantity: 2 }],
      status: "confirmed",
      total: 48,
    }),
    resp_headers: [
      ["content-type", "application/json; charset=utf-8"],
      ["cache-control", "no-store"],
      ["x-request-id", "req_8f24c1"],
    ],
  };
  const messages = [
    [
      "info",
      "storefront",
      "Starting tunnel",
      { protocol: "quic", version: "2026.9.1" },
    ],
    [
      "info",
      "storefront",
      "Generated connector ID",
      { connectorID: "00000000-0000-4000-8000-000000000102" },
    ],
    ["info", "storefront", "Initial protocol quic", {}],
    [
      "info",
      "storefront",
      "Registered tunnel connection",
      { connIndex: 0, location: "sjc01", protocol: "quic" },
    ],
    [
      "info",
      "storefront",
      "Registered tunnel connection",
      { connIndex: 1, location: "lax01", protocol: "quic" },
    ],
    [
      "info",
      "storefront",
      "Registered tunnel connection",
      { connIndex: 2, location: "sjc01", protocol: "quic" },
    ],
    [
      "info",
      "storefront",
      "Registered tunnel connection",
      { connIndex: 3, location: "lax01", protocol: "quic" },
    ],
    [
      "info",
      "storefront",
      "Updated ingress configuration",
      { hostname: "shop.example.com", service: "http://localhost:3000" },
    ],
    [
      "info",
      "payments-api",
      "Registered tunnel connection",
      { connIndex: 0, location: "sjc01", protocol: "quic" },
    ],
    [
      "info",
      "docs",
      "Registered tunnel connection",
      { connIndex: 0, location: "lax01", protocol: "quic" },
    ],
    [
      "debug",
      "storefront",
      "Request capture proxy listening",
      { address: "127.0.0.1:49152", bodyLimitKB: 1024 },
    ],
    [
      "info",
      "payments-api",
      "Updated ingress configuration",
      { hostname: "api.example.com", service: "http://localhost:8080" },
    ],
    [
      "info",
      "storefront",
      "POST /api/orders · 201 Created · 42ms",
      { bytes: 486, requestId: "req_8f24c1" },
    ],
    [
      "info",
      "storefront",
      "GET /api/products?collection=summer · 200 OK · 18ms",
      {},
    ],
    [
      "warn",
      "webhook-preview",
      "Local service is not running on localhost:8787",
      { retry: "Start your local server before connecting" },
    ],
    ["info", "storefront", "GET /api/cart · 200 OK · 12ms", {}],
    [
      "info",
      "payments-api",
      "POST /v1/payment-intents · 201 Created · 68ms",
      {},
    ],
    ["info", "docs", "GET /guides/quickstart · 200 OK · 7ms", {}],
    [
      "info",
      "storefront",
      "Tunnel healthy · 4 edge connections active",
      { connections: 4, protocol: "quic" },
    ],
  ];
  const logs = messages.map(([level, tunnel_id, message, fields], index) => ({
    fields: JSON.stringify(fields),
    id: index + 1,
    level,
    message,
    ts: now - (messages.length - index) * 1430,
    tunnel_id,
  }));
  window.__capture = { calls: [], logs, overview, requests };
  window.__mockDispatch = (cmd, args = {}) => {
    window.__capture.calls.push(cmd);
    const handlers = {
      check_config_change: () => ({
        config_revision: 1,
        message: null,
        run: 1,
        source_path: "~/Developer/storefront/.cloudflared/config.yml",
        status: "unchanged",
      }),
      check_hostname_dns: () => ({
        can_create: false,
        message: "Hostname points to this tunnel",
        needs_dns_access: false,
        status: "connected",
      }),
      e2e_report: () => null,
      get_request: () => structuredClone(detail),
      list_requests: () =>
        structuredClone(
          requests.filter(
            (r) => !args.tunnelId || r.tunnel_id === args.tunnelId
          )
        ),
      load_hostnames: () => ({
        fallback_service: "http_status:404",
        revision: "capture-1",
        routes: tunnels
          .find((t) => t.id === args.id)
          .state.mappings.filter((r) => r.hostname)
          .map((r, index) => ({ ...r, index })),
      }),
      login_item_status: () => ({
        enabled: false,
        message: null,
        status: "not_registered",
      }),
      onboarding_status: () => ({ account_connected: true, complete: true }),
      overview: () => structuredClone(overview),
      probe_services: () =>
        args.services.map((service) =>
          service.startsWith("http_status:") ? null : true
        ),
      query_logs: () =>
        structuredClone(
          logs.filter(
            (line) =>
              !args.args?.tunnelId || line.tunnel_id === args.args.tunnelId
          )
        ),
      request_capture_paused: () => false,
    };
    if (handlers[cmd]) {
      return handlers[cmd]();
    }
    if (cmd.startsWith("plugin:")) {
      return null;
    }
    throw new Error(`Unexpected capture command: ${cmd}`);
  };
}

let browser;
try {
  for (let attempt = 0; attempt < 100; attempt += 1) {
    try {
      // biome-ignore lint/performance/noAwaitInLoops: Poll the local server until startup completes.
      if ((await fetch(baseUrl)).ok) {
        break;
      }
    } catch {
      /* The local Vite server is starting. */
    }
    assert.notEqual(server?.exitCode, 1, serverLog);
    if (attempt === 99) {
      throw new Error(`App server did not start: ${serverLog}`);
    }
    await delay(200);
  }
  await mkdir(output, { recursive: true });
  browser = await chromium.launch({ headless: true });
  for (const theme of ["light", "dark"]) {
    // biome-ignore lint/performance/noAwaitInLoops: Finish each theme before reusing the same browser.
    const context = await browser.newContext({
      colorScheme: theme,
      deviceScaleFactor: 2,
      reducedMotion: "reduce",
      timezoneId: "America/Los_Angeles",
      viewport: { height: 760, width: 1200 },
    });
    // Enforce that the fixture cannot contact any external host.
    await context.route("**/*", (route) =>
      new URL(route.request().url()).origin === new URL(baseUrl).origin
        ? route.continue()
        : route.abort()
    );
    const page = await context.newPage();
    const errors = [];
    page.on("pageerror", (error) => errors.push(String(error)));
    await page.clock.setFixedTime(new Date("2026-10-15T17:42:18Z"));
    await installPageMock(page, fixture, { theme });
    for (const [name, view] of [
      ["projects", "tunnels:storefront"],
      ["requests", "requests:storefront"],
      ["logs", "logs"],
    ]) {
      // biome-ignore lint/performance/noAwaitInLoops: Navigation and capture must complete in sequence on the same page.
      await page.goto(`${baseUrl}/?capture=${name}#${view}`, {
        waitUntil: "networkidle",
      });
      await page.getByRole("navigation", { name: "Sections" }).waitFor();
      if (name === "projects") {
        await page
          .getByText("shop.example.com", { exact: true })
          .first()
          .waitFor();
      }
      if (name === "requests") {
        await page
          .getByRole("option", {
            exact: true,
            name: "POST /api/orders, status 201",
          })
          .click();
        await page
          .getByRole("tab", { exact: true, name: "Response" })
          .waitFor();
        await page.getByText('"ord_2026_1042"', { exact: true }).waitFor();
      }
      if (name === "logs") {
        await page
          .getByText("Tunnel healthy · 4 edge connections active", {
            exact: true,
          })
          .waitFor();
        await page
          .getByText("Registered tunnel connection", { exact: true })
          .first()
          .click();
      }
      await page.mouse.move(1199, 759);
      await delay(200);
      assert.deepEqual(errors, [], `${theme} ${name} rendering errors`);
      assert.equal(
        await page.locator("html").getAttribute("data-theme"),
        theme
      );
      await page.screenshot({ path: resolve(output, `${name}-${theme}.png`) });
      console.log(`Captured ${name}-${theme}.png (1200×760 @2x)`);
    }
    await page.setViewportSize({ height: 374, width: 360 });
    await page.goto(`${baseUrl}/popover.html`, { waitUntil: "networkidle" });
    await page
      .getByRole("button", { exact: true, name: "Open Dashboard" })
      .waitFor();
    await page
      .getByRole("switch", { exact: true, name: "Start webhook-preview" })
      .waitFor();
    assert.deepEqual(errors, [], `${theme} popover rendering errors`);
    await page.mouse.move(359, 373);
    await page.screenshot({
      omitBackground: true,
      path: resolve(output, `popover-${theme}.png`),
    });
    console.log(`Captured popover-${theme}.png (360×374 @2x)`);
    await context.close();
  }
  console.log(`Saved eight app screenshots to ${output}`);
} finally {
  await browser?.close();
  server?.kill("SIGTERM");
}
