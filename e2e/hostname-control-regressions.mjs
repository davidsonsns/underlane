// Real row/DNS components with synthetic IPC, including unknown DNS for disabled siblings.
import assert from "node:assert/strict";
import { mkdir } from "node:fs/promises";
import {
  browserPage,
  installHostnameMock,
  installPageMock,
} from "./lib/browser.mjs";

function installRegressionMock() {
  const mock = window.__hostnameRegression;
  const [tunnel] = mock.overview.tunnels;
  tunnel.connection = null;
  Object.assign(tunnel.state, {
    connections: 0,
    pid: null,
    status: "stopped",
    supervised: false,
  });
  for (const route of tunnel.state.mappings) {
    route.control_supported = Boolean(route.hostname);
    route.enabled = true;
    route.route_key = route.hostname
      ? { hostname: route.hostname, path: route.path, service: route.service }
      : null;
  }
  const original = window.__mockDispatch;
  window.__mockDispatch = (command, args) => {
    if (command === "check_connection") {
      mock.calls.push({ args, cmd: command });
      return structuredClone(tunnel.connection);
    }
    if (command !== "start_hostname" && command !== "stop_hostname") {
      return original(command, args);
    }
    mock.calls.push({ args, cmd: command });
    if (!tunnel.state.supervised) {
      for (const route of tunnel.state.mappings) {
        route.enabled = false;
      }
      Object.assign(tunnel.state, {
        connections: 4,
        pid: 123,
        run: tunnel.state.run + 1,
        status: "connected",
        supervised: true,
      });
    }
    const target = tunnel.state.mappings.find(
      (route) => route.hostname === args.target.key.hostname
    );
    target.enabled = command === "start_hostname";
    if (
      !tunnel.state.mappings.some((route) => route.hostname && route.enabled)
    ) {
      Object.assign(tunnel.state, {
        connections: 0,
        pid: null,
        status: "stopped",
        supervised: false,
      });
    }
    // Reproduce a backend refresh publishing unknown DNS for every configured hostname.
    tunnel.connection = {
      checked_at: Date.now(),
      checking: false,
      config_changed: false,
      config_revision: tunnel.state.config_revision,
      error: null,
      expected_tunnel_id: "a13e5877-0291-42f3-8922-3476e78ad90f",
      generation: (tunnel.connection?.generation ?? 0) + 1,
      routes: tunnel.state.mappings.flatMap((route, rule_index) =>
        route.hostname
          ? [
              {
                checking: false,
                dns: {
                  can_create: false,
                  checked_at: Date.now(),
                  message: "DNS target has not been verified.",
                  needs_dns_access: false,
                  source: "cloudflare",
                  status: "unknown",
                },
                hostname: route.hostname,
                local: {
                  checked_at: Date.now(),
                  message: "Listening",
                  status: "reachable",
                },
                path: route.path,
                public: {
                  checked_at: 0,
                  cloudflare_error: null,
                  http_status: null,
                  message: "Not checked",
                  status: "not_checked",
                  test_url: null,
                },
                rule_index,
                service: route.service,
                test_path: null,
              },
            ]
          : []
      ),
      run: tunnel.state.run,
    };
    window.__mockListeners["tunnel-state"]?.({
      payload: structuredClone(tunnel.state),
    });
    window.__mockListeners["connection-changed"]?.({
      payload: {
        connection: structuredClone(tunnel.connection),
        id: tunnel.id,
      },
    });
    window.__mockListeners["tunnels-changed"]?.({ payload: null });
    return null;
  };
}

const { baseUrl, browser, page, errors } = await browserPage({
  webkit: process.env.BROWSER === "webkit",
});
const failures = [];
const row = (host) =>
  page.locator(`[data-route-hostname="${host}.example.com"]`);
const action = (host, verb) =>
  row(host).getByRole("button", {
    exact: true,
    name: `${verb} ${host}.example.com`,
  });
try {
  await page.setViewportSize({ height: 780, width: 1000 });
  await installPageMock(
    page,
    installHostnameMock,
    { delay: 0, mockName: "__hostnameRegression" },
    installRegressionMock
  );
  await page.goto(`${baseUrl}/#tunnels:synthetic`);
  await action("app", "Start").waitFor();
  if (await page.getByText("Stopped", { exact: true }).count()) {
    failures.push("Stopped badges are rendered before any hostname starts");
  }
  await action("app", "Start").click();
  await action("app", "Stop").waitFor();
  assert.equal(
    await row("app")
      .getByText("DNS target unverified", { exact: true })
      .count(),
    0,
    "Starting a hostname must not add an unverified DNS badge to its row"
  );
  const flags = await page.evaluate(() =>
    window.__hostnameRegression.overview.tunnels[0].state.mappings
      .filter((route) => route.hostname)
      .map((route) => route.enabled)
  );
  assert.deepEqual(
    flags,
    [true, false, false, false],
    "Only the clicked hostname should be enabled"
  );
  const inactiveHosts = ["api", "docs", "checkout"];
  const inactiveCounts = await Promise.all(
    inactiveHosts.map((host) =>
      row(host).getByText("DNS target unverified", { exact: true }).count()
    )
  );
  for (const [index, host] of inactiveHosts.entries()) {
    if (inactiveCounts[index]) {
      failures.push(
        `Starting app displays DNS target unverified on stopped ${host}`
      );
    }
  }
  if (await page.getByText("Stopped", { exact: true }).count()) {
    failures.push("Stopped badges remain on disabled siblings");
  }
  await mkdir("e2e/out", { recursive: true });
  await row("app").hover();
  await page.screenshot({
    animations: "disabled",
    path: "e2e/out/hostname-regressions-one-active-dark.png",
  });
  await action("app", "Stop").click();
  await page.getByRole("button", { exact: true, name: "Start" }).waitFor();
  assert.equal(
    await page.getByRole("button", { exact: true, name: "Stop" }).count(),
    0
  );
  if (await page.getByText("DNS target unverified", { exact: true }).count()) {
    failures.push("Stopped project still displays cached DNS badges");
  }
  await row("app").hover();
  await page.screenshot({
    animations: "disabled",
    path: "e2e/out/hostname-regressions-all-stopped-dark.png",
  });
  await row("api")
    .getByRole("button", {
      exact: true,
      name: "DNS actions for api.example.com",
    })
    .click();
  await page.getByRole("menuitem", { exact: true, name: "Check DNS" }).click();
  await row("api")
    .getByRole("button", {
      exact: true,
      name: "DNS actions for api.example.com",
    })
    .click();
  await page
    .getByRole("menu")
    .getByText("DNS target unverified", { exact: true })
    .waitFor();
  assert.equal(
    await row("api")
      .getByText("DNS target unverified", { exact: true })
      .count(),
    0,
    "A manual DNS check keeps unverified status in the menu instead of adding a row badge"
  );
  await page.keyboard.press("Escape");
  const checks = await page.evaluate(() =>
    window.__hostnameRegression.calls.filter(
      (call) => call.cmd === "check_connection"
    )
  );
  assert.equal(checks.length, 1);
  assert.equal(checks[0].args.hostname, "api.example.com");
  assert.equal(checks[0].args.ruleIndex, 1);
  assert.deepEqual(errors, []);
  assert.deepEqual(failures, [], failures.join("\n"));
  console.log(
    "Hostname regressions passed: no stopped or unverified row badges; DNS details remain in the menu; last stop returns project to Start."
  );
} finally {
  await browser.close();
}
