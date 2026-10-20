// Synthetic bridge only: no Cloudflare account or source YAML changes.
import assert from "node:assert/strict";
import { mkdir } from "node:fs/promises";
import {
  browserPage,
  installHostnameMock,
  installPageMock,
} from "./lib/browser.mjs";

function installControlsMock() {
  const m = window.__hostnameControls;
  const [tunnel] = m.overview.tunnels;
  tunnel.connection = null;
  tunnel.state.status = "stopped";
  tunnel.state.supervised = false;
  tunnel.state.connections = 0;
  tunnel.state.pid = null;
  for (const mapping of tunnel.state.mappings) {
    mapping.enabled = true;
    mapping.control_supported = Boolean(mapping.hostname);
    mapping.route_key = mapping.hostname
      ? {
          hostname: mapping.hostname,
          path: mapping.path,
          service: mapping.service,
        }
      : null;
  }
  tunnel.connection = {
    checked_at: Date.now(),
    checking: false,
    config_changed: false,
    config_revision: tunnel.state.config_revision,
    error: null,
    expected_tunnel_id: "a13e5877-0291-42f3-8922-3476e78ad90f",
    generation: 1,
    routes: tunnel.state.mappings.flatMap((mapping, rule_index) =>
      mapping.hostname
        ? [
            {
              checking: false,
              dns: {
                can_create: false,
                checked_at: Date.now(),
                message: "DNS points to this project.",
                needs_dns_access: false,
                source: "cloudflare",
                status: "connected",
              },
              hostname: mapping.hostname,
              local: {
                checked_at: Date.now(),
                message: rule_index < 2 ? "Not listening" : "Listening",
                status: rule_index < 2 ? "unreachable" : "reachable",
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
              rule_index,
              service: mapping.service,
              test_path: null,
            },
          ]
        : []
    ),
    run: tunnel.state.run,
  };
  m.emit = () => {
    tunnel.connection.run = tunnel.state.run;
    tunnel.connection.config_revision = tunnel.state.config_revision;
    tunnel.connection.generation += 1;
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
  };
  async function setHostname(cmd, args) {
    if (m.delay) {
      await new Promise((resolve) => {
        m.resolve = resolve;
      });
    }
    if (m.fail) {
      throw new Error("Synthetic hostname update failed");
    }
    const mapping = tunnel.state.mappings.find(
      (route) =>
        JSON.stringify(route.route_key) === JSON.stringify(args.target.key)
    );
    if (!mapping || args.id !== tunnel.id) {
      throw new Error("Wrong hostname target");
    }
    if (tunnel.state.status === "stopped") {
      for (const route of tunnel.state.mappings) {
        route.enabled = false;
      }
      tunnel.state.run = 7;
      tunnel.state.config_revision = 9;
      tunnel.state.status = "connected";
      tunnel.state.supervised = true;
      tunnel.state.pid = 123;
      tunnel.state.connections = 4;
    }
    mapping.enabled = cmd === "start_hostname";
  }
  const dispatch = window.__mockDispatch;
  window.__mockDispatch = async (cmd, args) => {
    if (cmd === "overview" && m.delayOverview) {
      await new Promise((resolve) => {
        m.resolveOverview = resolve;
      });
    }
    if (
      ![
        "start_hostname",
        "stop_hostname",
        "start_tunnel",
        "stop_tunnel",
      ].includes(cmd)
    ) {
      return dispatch(cmd, args);
    }
    m.calls.push({ args: structuredClone(args), cmd });
    if (cmd.endsWith("_hostname")) {
      await setHostname(cmd, args);
    } else if (cmd === "stop_tunnel") {
      tunnel.state.status = "stopped";
      tunnel.state.supervised = false;
      tunnel.state.pid = null;
      tunnel.state.connections = 0;
    } else {
      tunnel.state.run += 1;
      tunnel.state.status = "connected";
      tunnel.state.supervised = true;
      tunnel.state.pid = 123;
      tunnel.state.connections = 4;
      for (const mapping of tunnel.state.mappings) {
        mapping.enabled = true;
      }
    }
    m.emit();
    return null;
  };
}

const { baseUrl, browser, page, errors } = await browserPage({
  webkit: process.env.BROWSER === "webkit",
});
const hosts = [
  "app.example.com",
  "api.example.com",
  "docs.example.com",
  "checkout.example.com",
];
const row = (index) => page.locator(`[data-route-hostname="${hosts[index]}"]`);
const action = (index, verb) =>
  row(index).getByRole("button", {
    exact: true,
    name: `${verb} ${hosts[index]}`,
  });
const copyAction = (index) =>
  row(index).getByRole("button", {
    exact: true,
    name: `Copy https://${hosts[index]}`,
  });
const ROW_ACTION_NAME = /^(Copy https:\/\/|Start |Stop )/;
async function assertActionsVisible() {
  await page.mouse.move(0, 0);
  assert.deepEqual(
    await Promise.all(
      hosts.map((_, index) =>
        row(index)
          .getByRole("button", { name: ROW_ACTION_NAME })
          .evaluateAll((buttons) =>
            buttons.map((button) => getComputedStyle(button).opacity)
          )
      )
    ),
    hosts.map(() => ["1", "1"]),
    "Copy and start/stop stay visible without hover"
  );
}
const enabled = () =>
  page.evaluate(() =>
    window.__hostnameControls.overview.tunnels[0].state.mappings
      .filter((mapping) => mapping.hostname)
      .map((mapping) => mapping.enabled)
  );
const stoppedCount = (index) =>
  row(index).getByText("Stopped", { exact: true }).count();
const hostnameCalls = () =>
  page.evaluate(() =>
    window.__hostnameControls.calls.filter((call) =>
      call.cmd.endsWith("_hostname")
    )
  );

try {
  await page.setViewportSize({ height: 780, width: 1000 });
  await mkdir("e2e/out", { recursive: true });
  await installPageMock(
    page,
    installHostnameMock,
    { delay: 0, mockName: "__hostnameControls" },
    installControlsMock
  );
  await page.goto(`${baseUrl}/#tunnels:synthetic`);
  await action(0, "Start").waitFor();
  assert.equal(await stoppedCount(0), 0);
  assert.equal(
    await row(0).getByRole("link", { exact: true, name: hosts[0] }).count(),
    0
  );

  // Copy and start/stop remain visible without hover or keyboard focus.
  await page.getByRole("heading", { exact: true, name: "Hostnames" }).click();
  await assertActionsVisible();
  await copyAction(0).click();
  assert.equal(
    await page.evaluate(
      () =>
        window.__hostnameControls.calls.find(
          (call) => call.cmd === "plugin:clipboard-manager|write_text"
        )?.args.text
    ),
    `https://${hosts[0]}`
  );
  await page.getByRole("heading", { exact: true, name: "Hostnames" }).click();
  await page.screenshot({
    animations: "disabled",
    path: "e2e/out/hostname-controls-stopped-dark.png",
  });
  await page.evaluate(() => {
    document.documentElement.dataset.theme = "light";
  });
  await page.screenshot({
    animations: "disabled",
    path: "e2e/out/hostname-controls-stopped-light.png",
  });
  await page.evaluate(() => {
    document.documentElement.dataset.theme = "dark";
  });
  await page.mouse.move(0, 0);
  await page
    .getByRole("button", { exact: true, name: "Manage hostnames" })
    .focus();
  await page.keyboard.press("Tab");
  assert.equal(
    await copyAction(0).evaluate(
      (element) => element === document.activeElement
    ),
    true
  );
  await page.keyboard.press("Tab");
  assert.equal(
    await action(0, "Start").evaluate(
      (element) => element === document.activeElement
    ),
    true
  );

  // Flow 1: A starts from a stopped project; C joins without enabling B or D.
  await page.keyboard.press("Enter");
  await action(0, "Stop").waitFor();
  assert.deepEqual(await enabled(), [true, false, false, false]);
  assert.equal(await stoppedCount(0), 0);
  assert.equal(
    await row(0).getByRole("link", { exact: true, name: hosts[0] }).count(),
    1
  );
  assert.equal(await stoppedCount(1), 0);
  assert.equal(
    await row(1).getByRole("link", { exact: true, name: hosts[1] }).count(),
    0
  );
  const [first] = await hostnameCalls();
  assert.equal(first.cmd, "start_hostname");
  assert.equal(first.args.target.run, 0);
  assert.equal(first.args.target.config_revision, 0);
  await action(2, "Start").click();
  await action(2, "Stop").waitFor();
  assert.deepEqual(await enabled(), [true, false, true, false]);
  const [, second] = await hostnameCalls();
  assert.equal(second.args.target.run, 7);
  assert.equal(second.args.target.config_revision, 9);
  assert.equal(second.args.target.key.hostname, hosts[2]);

  // Flow 2: project Start enables all; stopping B and D preserves A and C.
  await page.getByRole("button", { exact: true, name: "Stop" }).click();
  await page.getByRole("button", { exact: true, name: "Start" }).click();
  await action(1, "Stop").waitFor();
  assert.deepEqual(await enabled(), [true, true, true, true]);
  assert.equal(await row(1).locator('[role="img"].bg-bad').count(), 1);
  assert.equal(await row(3).locator('[role="img"].bg-ok').count(), 1);
  await action(1, "Stop").click();
  await action(1, "Start").waitFor();
  assert.equal(
    await row(1).locator('[role="img"].bg-bad').count(),
    0,
    "a stopped hostname must not retain its red health color"
  );
  assert.equal(await row(1).locator('[role="img"].border-off').count(), 1);
  await action(3, "Stop").click();
  await action(3, "Start").waitFor();
  assert.equal(
    await row(3).locator('[role="img"].bg-ok').count(),
    0,
    "a stopped hostname must not retain its green health color"
  );
  assert.equal(await row(3).locator('[role="img"].border-off').count(), 1);
  assert.equal(
    await row(0).locator('[role="img"].bg-bad').count(),
    1,
    "the active sibling keeps its red health color"
  );
  assert.equal(
    await row(2).locator('[role="img"].bg-ok').count(),
    1,
    "the active sibling keeps its green health color"
  );
  assert.deepEqual(await enabled(), [true, false, true, false]);
  assert.equal(await stoppedCount(0), 0);
  assert.equal(await stoppedCount(2), 0);
  assert.equal(
    await row(0).getByRole("link", { exact: true, name: hosts[0] }).count(),
    1
  );
  assert.equal(
    await row(2).getByRole("link", { exact: true, name: hosts[2] }).count(),
    1
  );
  await page.getByRole("heading", { exact: true, name: "Hostnames" }).click();
  await page.keyboard.press("Escape");
  await assertActionsVisible();
  await page.screenshot({
    animations: "disabled",
    path: "e2e/out/hostname-controls-running-dark.png",
  });
  await page.evaluate(() => {
    document.documentElement.dataset.theme = "light";
  });
  await page.screenshot({
    animations: "disabled",
    path: "e2e/out/hostname-controls-running-light.png",
  });
  await page.evaluate(() => {
    document.documentElement.dataset.theme = "dark";
  });

  // Pending requests stay visible, block repeated mutations, and keep adjacent status.
  await page.evaluate(() => {
    window.__hostnameControls.delay = true;
    window.__hostnameControls.fail = true;
  });
  await action(1, "Start").click();
  await page.waitForFunction(() => Boolean(window.__hostnameControls.resolve));
  assert.equal(await action(1, "Start").getAttribute("aria-busy"), "true");
  assert.equal(await action(1, "Start").isDisabled(), true);
  assert.equal(await action(2, "Stop").isDisabled(), true);
  await page.mouse.move(0, 0);
  await page.waitForFunction(
    () =>
      Number(
        getComputedStyle(
          document.querySelector('[aria-label="Start api.example.com"]')
        ).opacity
      ) >= 0.4
  );
  assert.equal(await stoppedCount(1), 0);
  assert.equal(await stoppedCount(0), 0);
  await page.evaluate(() => window.__hostnameControls.resolve());
  await page
    .getByText("Synthetic hostname update failed", { exact: false })
    .waitFor();
  await page.waitForFunction(
    () =>
      document
        .querySelector('[aria-label="Start api.example.com"]')
        .getAttribute("aria-disabled") === "false"
  );
  assert.deepEqual(await enabled(), [true, false, true, false]);
  await page.evaluate(() => {
    window.__hostnameControls.delay = false;
    window.__hostnameControls.fail = false;
    window.__hostnameControls.delayOverview = true;
  });
  const startedAt = performance.now();
  await action(1, "Start").click();
  await action(1, "Stop").waitFor();
  await page.waitForFunction(
    () =>
      document
        .querySelector('[aria-label="Stop api.example.com"]')
        .getAttribute("aria-busy") === "false",
    undefined,
    { timeout: 750 }
  );
  console.log(
    `Hostname action released in ${Math.round(performance.now() - startedAt)} ms while overview was blocked`
  );
  await page.waitForFunction(() =>
    Boolean(window.__hostnameControls.resolveOverview)
  );
  await page.evaluate(() => {
    window.__hostnameControls.delayOverview = false;
    window.__hostnameControls.resolveOverview();
  });
  assert.deepEqual(await enabled(), [true, true, true, false]);

  // Unsupported controls stay keyboard discoverable and explain why they cannot run.
  const beforeUnsupported = (await hostnameCalls()).length;
  await page.evaluate(() => {
    const m = window.__hostnameControls;
    const [, , , mapping] = m.overview.tunnels[0].state.mappings;
    mapping.control_supported = false;
    mapping.note = "Individual controls are unavailable for this configuration";
    m.emit();
  });
  await page.waitForFunction(
    () =>
      document
        .querySelector('[aria-label="Start checkout.example.com"]')
        .getAttribute("aria-disabled") === "true"
  );
  await page.keyboard.press("Tab");
  await action(3, "Start").focus();
  await page.keyboard.press("Tab");
  await page.keyboard.press("Shift+Tab");
  assert.equal(
    await action(3, "Start").evaluate(
      (element) => element === document.activeElement
    ),
    true
  );
  await page
    .locator("[data-slot=tooltip-content]:not([data-state=closed])")
    .filter({
      hasText: "Individual controls are unavailable for this configuration",
    })
    .waitFor();
  await page.keyboard.press("Enter");
  assert.equal((await hostnameCalls()).length, beforeUnsupported);

  // The first connector generation must be prepared before another route can join.
  await page.evaluate(() => {
    const m = window.__hostnameControls;
    const [tunnel] = m.overview.tunnels;
    const [, , , mapping] = tunnel.state.mappings;
    mapping.control_supported = true;
    tunnel.state.run += 1;
    tunnel.state.status = "starting";
    tunnel.state.config_revision = 0;
    tunnel.state.pid = null;
    tunnel.state.connections = 0;
    m.emit();
  });
  await action(3, "Start").focus();
  await page.keyboard.press("Tab");
  await page.keyboard.press("Shift+Tab");
  await page
    .locator("[data-slot=tooltip-content]:not([data-state=closed])")
    .filter({ hasText: "Waiting for the connector to start…" })
    .waitFor();
  assert.equal(await action(3, "Start").isDisabled(), true);
  await page.keyboard.press("Enter");
  assert.equal((await hostnameCalls()).length, beforeUnsupported);

  await page.evaluate(() => {
    const m = window.__hostnameControls;
    const [tunnel] = m.overview.tunnels;
    tunnel.state.status = "connected";
    tunnel.state.config_revision = 9;
    tunnel.state.pid = 123;
    tunnel.state.connections = 4;
    m.emit();
  });
  await page
    .getByRole("button", { exact: true, name: "More tunnel actions" })
    .click();
  await page
    .getByRole("menuitem", { exact: true, name: "Start all hostnames" })
    .click();
  await action(3, "Stop").waitFor();
  assert.deepEqual(await enabled(), [true, true, true, true]);

  assert.equal(
    await page.evaluate(
      () =>
        window.__hostnameControls.calls.filter((call) =>
          ["restart_tunnel", "save_hostnames", "update_tunnel"].includes(
            call.cmd
          )
        ).length
    ),
    0
  );
  assert.deepEqual(errors, []);
  console.log(
    "Hostname controls passed: independent start/stop, always-visible copy and start/stop, clipboard, Tab, pending/error/retry, adjacent routes unchanged, no config write or restart."
  );
} finally {
  await browser.close();
}
