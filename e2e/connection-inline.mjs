// Synthetic bridge only: no hostname or Cloudflare account requests.
import assert from "node:assert/strict";
import { mkdir } from "node:fs/promises";
import {
  browserPage,
  installHostnameMock,
  installPageMock,
} from "./lib/browser.mjs";

function installConnectionMock() {
  const m = window.__connectionMock;
  const [tunnel] = m.overview.tunnels;
  const expected = "a13e5877-0291-42f3-8922-3476e78ad90f";
  const observed = "5b1370f1-22bd-4c20-8c3e-234596cd6789";
  tunnel.state.mappings = [tunnel.state.mappings[0]];
  tunnel.connection = {
    checked_at: Date.now(),
    checking: false,
    config_changed: false,
    config_revision: 1,
    error: null,
    expected_tunnel_id: expected,
    routes: [
      {
        checking: false,
        dns: {
          actual_target: `${observed}.cfargotunnel.com`,
          can_create: false,
          can_repair: true,
          checked_at: Date.now(),
          expected_tunnel_id: expected,
          message:
            "Cloudflare confirmed that this hostname routes to a different tunnel.",
          needs_dns_access: false,
          observed_tunnel_id: observed,
          reason: "wrong_tunnel",
          source: "cloudflare",
          status: "conflict",
        },
        hostname: "app.example.com",
        local: {
          checked_at: Date.now(),
          message: "Local service is listening.",
          status: "reachable",
        },
        path: null,
        public: {
          checked_at: Date.now(),
          cloudflare_error: 1033,
          http_status: 530,
          message: "Cloudflare cannot reach the connector.",
          status: "cloudflare_error",
          test_url: "https://app.example.com/",
        },
        rule_index: 0,
        service: "http://localhost:4000",
        test_path: null,
      },
    ],
    run: 1,
  };
  m.emit = () =>
    window.__mockListeners["connection-changed"]?.({
      payload: {
        connection: structuredClone(tunnel.connection),
        id: tunnel.id,
      },
    });
  const dispatch = window.__mockDispatch;
  // biome-ignore lint/complexity/noExcessiveCognitiveComplexity: A single synthetic IPC dispatcher keeps the fixture self-contained.
  window.__mockDispatch = async (cmd, args) => {
    if (
      [
        "check_connection",
        "preview_dns_repair",
        "repair_hostname_dns",
        "create_hostname_dns",
        "start_tunnel",
      ].includes(cmd)
    ) {
      m.calls.push({ args, cmd });
      if (cmd === "preview_dns_repair") {
        return {
          current_project_name: "Previous project",
          current_target: `${observed}.cfargotunnel.com`,
          current_tunnel_id: observed,
          expected_target: `${expected}.cfargotunnel.com`,
          expected_tunnel_id: expected,
          expires_at: Date.now() + 60_000,
          hostname: "app.example.com",
          review_id: "review-1",
        };
      }
      if (cmd === "repair_hostname_dns") {
        assertReview(args.reviewId);
        tunnel.connection.routes[0].dns = {
          ...tunnel.connection.routes[0].dns,
          can_repair: false,
          message: "Cloudflare confirmed this hostname routes to this project.",
          status: "connected",
        };
        return structuredClone(tunnel.connection.routes[0].dns);
      }
      if (cmd === "create_hostname_dns") {
        tunnel.connection.routes[0].dns = {
          ...tunnel.connection.routes[0].dns,
          can_create: false,
          status: "connected",
        };
        return structuredClone(tunnel.connection.routes[0].dns);
      }
      if (cmd === "start_tunnel") {
        tunnel.state.status = "connected";
        return null;
      }
      if (cmd === "check_connection") {
        if (m.delayCheck) {
          await new Promise((resolve) => {
            m.resolveCheck = resolve;
          });
        }
        tunnel.connection.checked_at = Date.now();
        if (args.testPath) {
          tunnel.connection.routes[0].test_path = args.testPath;
        }
        return structuredClone(tunnel.connection);
      }
    }
    return dispatch(cmd, args);
  };
  function assertReview(review) {
    if (review !== "review-1") {
      throw new Error("Missing confirmed review");
    }
  }
}

const { baseUrl, browser, page, errors } = await browserPage({
  webkit: process.env.BROWSER === "webkit",
});
try {
  await page.setViewportSize({ height: 708, width: 1092 });
  await installPageMock(
    page,
    installHostnameMock,
    { delay: 0, mockName: "__connectionMock" },
    installConnectionMock
  );
  await page.goto(`${baseUrl}/#tunnels:synthetic`);
  const panel = page.locator('[data-connection-hostname="app.example.com"]');
  await panel.getByText("This hostname points to a different tunnel").waitFor();
  await page
    .getByRole("button", {
      exact: true,
      name: "Wrong tunnel for app.example.com",
    })
    .waitFor();
  await mkdir("e2e/out", { recursive: true });
  await page.screenshot({
    animations: "disabled",
    path: "e2e/out/connection-inline-dark.png",
  });
  await page.evaluate(() => {
    document.documentElement.dataset.theme = "light";
  });
  await page.screenshot({
    animations: "disabled",
    path: "e2e/out/connection-inline-light.png",
  });
  assert.equal(
    await page.evaluate(
      () =>
        window.__connectionMock.calls.filter((call) =>
          [
            "check_hostname_dns",
            "probe_services",
            "repair_hostname_dns",
          ].includes(call.cmd)
        ).length
    ),
    0
  );
  const repair = panel.getByRole("button", {
    exact: true,
    name: "Repair DNS…",
  });
  await repair.focus();
  await page.keyboard.press("Enter");
  const dialog = page.getByRole("dialog");
  await dialog.waitFor();
  await page.screenshot({
    animations: "disabled",
    path: "e2e/out/connection-repair-light.png",
  });
  const dialogText = await dialog.innerText();
  for (const text of [
    "a13e5877-0291-42f3-8922-3476e78ad90f",
    "5b1370f1-22bd-4c20-8c3e-234596cd6789",
    "Traffic for this hostname will move",
  ]) {
    assert.ok(dialogText.includes(text));
  }
  assert.ok((await dialog.innerText()).includes("Previous project"));
  await page.keyboard.press("Escape");
  await dialog.waitFor({ state: "hidden" });
  assert.equal(
    await repair.evaluate((element) => document.activeElement === element),
    true
  );
  assert.equal(
    await page.evaluate(
      () =>
        window.__connectionMock.calls.filter(
          (call) => call.cmd === "repair_hostname_dns"
        ).length
    ),
    0
  );
  await repair.click();
  await dialog.waitFor();
  await dialog.getByRole("button", { exact: true, name: "Cancel" }).click();
  await repair.click();
  await dialog.getByRole("button", { exact: true, name: "Repair DNS" }).click();
  await dialog.waitFor({ state: "hidden" });
  assert.equal(
    await page.evaluate(
      () => window.__connectionMock.overview.tunnels[0].state.status
    ),
    "connected"
  );
  assert.equal(
    await page.evaluate(
      () =>
        window.__connectionMock.calls.filter((call) =>
          ["start_tunnel", "stop_tunnel", "restart_tunnel"].includes(call.cmd)
        ).length
    ),
    0
  );
  await panel.waitFor({ state: "hidden" });
  assert.equal(
    await page.evaluate(
      () =>
        window.__connectionMock.calls.filter(
          (call) => call.cmd === "repair_hostname_dns"
        ).length
    ),
    1
  );

  // Expected lifecycle states do not add a banner or disclosure under every hostname.
  await page.evaluate(() => {
    const m = window.__connectionMock;
    const [tunnel] = m.overview.tunnels;
    m.baseMapping = structuredClone(tunnel.state.mappings[0]);
    m.baseRoute = structuredClone(tunnel.connection.routes[0]);
    tunnel.state.status = "stopped";
    tunnel.state.supervised = false;
    tunnel.state.pid = null;
    tunnel.state.connections = 0;
    const hosts = [
      "app.example.com",
      "api.example.com",
      "docs.example.com",
      "checkout.example.com",
    ];
    tunnel.state.mappings = hosts.map((hostname) => ({
      ...m.baseMapping,
      hostname,
    }));
    tunnel.connection.routes = hosts.map((hostname, rule_index) => ({
      ...structuredClone(m.baseRoute),
      hostname,
      rule_index,
    }));
    window.__mockListeners["tunnel-state"]?.({
      payload: structuredClone(tunnel.state),
    });
    m.emit();
  });
  await page
    .getByRole("button", {
      exact: true,
      name: "DNS actions for checkout.example.com",
    })
    .waitFor();
  assert.equal(await page.locator("[data-connection-hostname]").count(), 0);
  assert.equal(
    await page
      .getByRole("button", { exact: true, name: "Start tunnel" })
      .count(),
    0
  );
  await page.getByRole("button", { exact: true, name: "Start" }).waitFor();
  assert.equal(
    await page.getByRole("button", { name: /^DNS verified for / }).count(),
    0
  );
  await page
    .getByRole("button", {
      exact: true,
      name: "DNS actions for app.example.com",
    })
    .scrollIntoViewIfNeeded();
  await page.screenshot({
    animations: "disabled",
    path: "e2e/out/connection-compact-stopped.png",
  });
  await page.evaluate(() => {
    const m = window.__connectionMock;
    const [tunnel] = m.overview.tunnels;
    tunnel.state.status = "connected";
    tunnel.state.supervised = true;
    tunnel.state.pid = 123;
    tunnel.state.connections = 4;
    tunnel.state.mappings = [m.baseMapping];
    tunnel.connection.routes = [m.baseRoute];
    window.__mockListeners["tunnel-state"]?.({
      payload: structuredClone(tunnel.state),
    });
    m.emit();
  });
  await page
    .getByRole("button", {
      exact: true,
      name: "DNS actions for app.example.com",
    })
    .waitFor();

  for (const [local, status, code] of [
    ["unreachable", "origin_error", 502],
    ["reachable", "cloudflare_error", 530],
    ["reachable", "transport_error", null],
    ["reachable", "auth_required", 403],
    ["reachable", "responded", 404],
    ["reachable", "verified", 200],
    ["reachable", "not_checked", null],
  ]) {
    // biome-ignore lint/performance/noAwaitInLoops: Each state must render before checking visibility.
    await page.evaluate(
      ([localState, publicState, httpCode]) => {
        const m = window.__connectionMock;
        const [route] = m.overview.tunnels[0].connection.routes;
        route.local.status = localState;
        route.public.status = publicState;
        route.public.http_status = httpCode;
        route.public.checked_at = Date.now();
        m.emit();
      },
      [local, status, code]
    );
    await panel.waitFor({ state: "hidden" });
  }

  await page.evaluate(() => {
    const m = window.__connectionMock;
    const [route] = m.overview.tunnels[0].connection.routes;
    route.dns.status = "missing";
    route.dns.can_create = true;
    route.dns.needs_dns_access = false;
    m.emit();
  });
  await panel
    .getByRole("button", { exact: true, name: "Create DNS record" })
    .click();
  await panel.waitFor({ state: "hidden" });
  assert.equal(
    await page.evaluate(
      () =>
        window.__connectionMock.calls.filter(
          (call) => call.cmd === "create_hostname_dns"
        ).length
    ),
    1
  );
  await mkdir("e2e/out", { recursive: true });
  // Capture the chosen warning state in both themes at a small laptop size.
  await page.evaluate(() => {
    const m = window.__connectionMock;
    m.overview.tunnels[0].connection.routes[0].dns.status = "conflict";
    m.overview.tunnels[0].connection.routes[0].dns.can_repair = true;
    m.overview.tunnels[0].connection.routes[0].dns.message =
      "Cloudflare confirmed that this hostname routes to a different tunnel.";
    m.emit();
  });
  await panel
    .getByText("This hostname points to a different tunnel", { exact: true })
    .waitFor();
  await panel
    .getByRole("button", { exact: true, name: "Connection details" })
    .click();
  for (const label of [
    "DNS record",
    "Connector",
    "Local service",
    "Public endpoint",
  ]) {
    // biome-ignore lint/performance/noAwaitInLoops: Check visible evidence in display order.
    await panel.getByText(label, { exact: true }).waitFor();
  }
  await panel
    .getByLabel("Public test path", { exact: true })
    .fill("//other.example.com");
  await panel
    .getByRole("button", { exact: true, name: "Check connection" })
    .click();
  await panel.getByRole("alert").filter({ hasText: "Enter a path" }).waitFor();
  await panel.getByLabel("Public test path", { exact: true }).fill("/health");
  await panel
    .getByRole("button", { exact: true, name: "Check connection" })
    .click();
  await page
    .getByRole("heading", { exact: true, name: "Hostnames" })
    .scrollIntoViewIfNeeded();
  await page.screenshot({
    animations: "disabled",
    path: "e2e/out/connection-details-light.png",
  });
  await repair.click();
  await dialog.waitFor();
  await page.keyboard.press("Escape");
  assert.ok(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= window.innerWidth
    )
  );
  // A delayed request must not reopen a dialog or overwrite the next view.
  await page.evaluate(() => {
    window.__connectionMock.delayCheck = true;
  });
  await panel
    .getByRole("button", { exact: true, name: "Check connection" })
    .click();
  await page.waitForFunction(() =>
    Boolean(window.__connectionMock.resolveCheck)
  );
  await page.getByRole("button", { exact: true, name: "Settings" }).click();
  await panel.waitFor({ state: "hidden" });
  await page.evaluate(() => window.__connectionMock.resolveCheck());
  assert.equal(await page.getByRole("dialog").count(), 0);
  await page.evaluate(() => {
    window.__connectionMock.overview.tunnels[0].connection = null;
    window.__mockListeners["tunnels-changed"]?.({ payload: null });
  });
  await page.locator('[data-tunnel-id="synthetic"]').click();
  await page
    .getByRole("button", {
      exact: true,
      name: "DNS actions for app.example.com",
    })
    .waitFor();
  await panel.waitFor({ state: "hidden" });
  assert.equal(
    await page.evaluate(
      () =>
        window.__connectionMock.calls.filter((call) =>
          ["check_hostname_dns", "probe_services"].includes(call.cmd)
        ).length
    ),
    0
  );
  assert.deepEqual(errors, []);
  console.log(
    "Connection inline passed: repair/create banners only; four stopped hostnames and informational states stay compact; confirmation, details, safe paths, focus/Escape, navigation, no automatic legacy probes."
  );
} finally {
  await browser.close();
}
