// Synthetic IPC drives the actual Routes and popover components; no account access.
import assert from "node:assert/strict";
import {
  browserPage,
  installHostnameMock,
  installPageMock,
} from "./lib/browser.mjs";

function installSharedServiceMock() {
  const [tunnel] = window.__sharedService.overview.tunnels;
  tunnel.state.mappings[1].service = tunnel.state.mappings[0].service;
  for (const [index, mapping] of tunnel.state.mappings.entries()) {
    mapping.enabled = index === 1;
  }
  tunnel.connection = {
    checked_at: Date.now(),
    checking: false,
    config_changed: false,
    config_revision: tunnel.state.config_revision,
    error: null,
    expected_tunnel_id: "expected",
    generation: 1,
    routes: tunnel.state.mappings
      .filter((mapping) => mapping.hostname)
      .map((mapping, index) => ({
        checking: false,
        dns: {
          can_create: false,
          message: "DNS correct",
          needs_dns_access: false,
          source: "cloudflare",
          status: "connected",
        },
        hostname: mapping.hostname,
        local: {
          checked_at: Date.now(),
          message: index === 1 ? "Listening" : "Stopped",
          status: index === 1 ? "reachable" : "unsupported",
        },
        path: mapping.path,
        public: {
          checked_at: 0,
          cloudflare_error: null,
          http_status: null,
          message: "Not checked",
          status: "not_checked",
          test_url: null,
        },
        rule_index: index,
        service: mapping.service,
        test_path: null,
      })),
    run: tunnel.state.run,
  };
}

const { baseUrl, browser, page, errors } = await browserPage({
  webkit: process.env.BROWSER === "webkit",
});
try {
  await installPageMock(
    page,
    installHostnameMock,
    { delay: 0, mockName: "__sharedService" },
    installSharedServiceMock
  );
  await page.goto(`${baseUrl}/#tunnels:synthetic`);
  const active = page.locator('[data-route-hostname="api.example.com"]');
  await active.waitFor();
  await active.getByRole("img", { name: /Local service reachable/ }).waitFor();
  const pausedDot = page
    .locator('[data-route-hostname="app.example.com"]')
    .getByRole("img", { name: /has not been checked/ });
  await pausedDot.waitFor();
  assert.equal(
    await pausedDot.evaluate((element) =>
      element.classList.contains("invisible")
    ),
    false
  );
  assert.equal(
    await pausedDot.evaluate((element) => element.classList.contains("border")),
    true
  );
  assert.equal(
    await page
      .locator(".routes-row")
      .last()
      .locator('[role="img"]')
      .evaluate((element) => getComputedStyle(element).visibility),
    "hidden"
  );
  assert.equal(
    await page
      .locator('[data-route-hostname="app.example.com"]')
      .getByRole("img", { name: /Local service reachable/ })
      .count(),
    0
  );

  // A stale row with the same index and origin but a different path is not this row's evidence.
  await page.evaluate(() => {
    const m = window.__sharedService;
    const [tunnel] = m.overview.tunnels;
    tunnel.connection.generation += 1;
    tunnel.connection.routes[1].path = "/other";
    window.__mockListeners["connection-changed"]?.({
      payload: {
        connection: structuredClone(tunnel.connection),
        id: tunnel.id,
      },
    });
  });
  await active
    .getByRole("img", { name: /Local service reachable/ })
    .waitFor({ state: "hidden" });

  await page.goto(`${baseUrl}/popover.html`);
  await page.getByText("api.example.com", { exact: true }).waitFor();
  assert.equal(
    await page.getByText("app.example.com", { exact: true }).count(),
    0
  );
  await page
    .getByRole("button", { exact: true, name: "Open in browser" })
    .click();
  const calls = await page.evaluate(() => window.__sharedService.calls);
  assert.equal(
    calls.filter((call) => call.cmd === "plugin:opener|open_url").at(-1)?.args
      .url,
    "https://api.example.com"
  );
  assert.deepEqual(errors, []);
  console.log(
    "Hostname shared-service passed: row-specific reachability, stale path rejection, enabled popover description and public URL."
  );
} finally {
  await browser.close();
}
