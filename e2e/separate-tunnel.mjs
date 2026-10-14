// Run against Vite on port 1420; all Cloudflare and Tauri calls are synthetic.
import assert from "node:assert/strict";

import { browserPage, installPageMock } from "./lib/browser.mjs";

const { baseUrl, browser, page, errors } = await browserPage({
  height: 800,
  timeout: 7000,
});
await installPageMock(page, () => {
  const oldId = "00000000-0000-4000-8000-000000000001";
  const newId = "00000000-0000-4000-8000-000000000002";
  const route = {
    hostname: "newlycitizen.example.com",
    inspected: false,
    note: null,
    path: null,
    service: "http://localhost:8787",
  };
  const runningState = (running) =>
    running
      ? {
          cloudflare_tunnel_id: oldId,
          connections: 4,
          pid: 123,
          status: "connected",
        }
      : {
          cloudflare_tunnel_id: null,
          connections: 0,
          pid: null,
          status: "stopped",
        };
  function project(id, name, running) {
    return {
      autostart: false,
      cloudflare_tunnel_id: oldId,
      config_path: `/synthetic/${id}.yml`,
      created_at: 1,
      group_id: null,
      group_name: "",
      has_token: false,
      id,
      inspect: false,
      kind: "config",
      loglevel: "info",
      name,
      origin_url: null,
      setup_needed: false,
      sort: 0,
      state: {
        ...runningState(running),
        config_revision: 0,

        detail: null,
        failure: null,
        hint: null,
        id,
        inspecting: false,
        mappings: [route],
        metrics_addr: null,

        public_url: null,
        restarts: 0,
        run: 1,
        started_at: null,

        supervised: running,
        tunnel_conflict: null,
      },
    };
  }
  const selected = project("newlycitizen", "newlycitizen", false);
  const overview = {
    active_count: 1,
    cloudflared: {
      path: "/synthetic/cloudflared",
      source: "mock",
      version: "2026.9.1",
    },
    data_dir: "/synthetic",
    dropped_captures: 0,
    groups: [],
    last_launch: null,
    settings: {
      cloudflared_path: null,
      default_loglevel: "info",
      log_ttl_days: 7,
      max_body_kb: 1024,
      max_db_mb: 100,
      redact_headers: [],
      redact_query: [],
      theme: "dark",
    },
    tunnels: [project("example", "example", true), selected],
  };
  window.__separateMock = { calls: [], fail: true, overview };
  window.__mockDispatch = (cmd, args) => {
    const mock = window.__separateMock;
    mock.calls.push({ args, cmd });

    const handlers = {
      check_config_change: () => null,
      create_separate_tunnel: async () => {
        const emit = (stage) =>
          window.__mockListeners["project-separate-progress"]?.({
            event: "project-separate-progress",
            id: 1,
            payload: { id: args.id, request_id: args.requestId, stage },
          });
        emit("dns");
        await new Promise((resolve) => setTimeout(resolve, 250));
        if (mock.fail) {
          selected.separate_tunnel_pending = true;
          return Promise.reject(
            "DNS update failed. Retry to finish using the already-created tunnel."
          );
        }
        selected.separate_tunnel_pending = false;
        selected.cloudflare_tunnel_id = newId;
        Object.assign(selected.state, {
          cloudflare_tunnel_id: newId,
          status: "starting",
          supervised: true,
        });
        return {
          config_path: selected.config_path,
          moved_hostnames: [route.hostname],
          start_error: null,
          tunnel_id: newId,
        };
      },
      login_item_status: () => ({
        enabled: false,
        message: null,
        status: "unsupported",
      }),
      onboarding_status: () => ({ account_connected: false, complete: true }),
      overview: () => structuredClone(overview),
      preview_separate_tunnel: () => ({
        config_path: selected.config_path,
        hostnames: [route.hostname],
        old_tunnel_id: oldId,
        resuming: !mock.fail,
        revision: "review-1",
      }),
      probe_services: () => args.services.map(() => true),
      scan_default: () => [],
    };
    const handler = handlers[cmd];
    if (handler) {
      return handler();
    }
    if (!cmd.startsWith("plugin:")) {
      console.warn("Unmocked Tauri command", cmd);
    }
    return null;
  };
});
try {
  await page.goto(`${baseUrl}/#tunnels:newlycitizen`, {
    timeout: 30_000,
    waitUntil: "domcontentloaded",
  });
  await page
    .getByText("Cannot start while hw is running", { exact: true })
    .waitFor();
  await assert.equal(
    await page.getByRole("button", { exact: true, name: "Start" }).isDisabled(),
    true
  );
  await page
    .locator("#app")
    .getByRole("button", { exact: true, name: "Create separate tunnel" })
    .click();
  const dialog = page.getByRole("alertdialog");
  await dialog.waitFor();
  assert.match(await dialog.innerText(), /newlycitizen\.example\.com/);
  assert.match(await dialog.innerText(), /moves the hostname DNS records/);
  await dialog.getByRole("button", { exact: true, name: "Cancel" }).click();
  await dialog.waitFor({ state: "detached" });
  assert.equal(
    await page.evaluate(
      () =>
        window.__separateMock.calls.filter(
          (c) => c.cmd === "create_separate_tunnel"
        ).length
    ),
    0
  );
  await page
    .locator("#app")
    .getByRole("button", { exact: true, name: "Create separate tunnel" })
    .click();
  await dialog
    .getByRole("button", { exact: true, name: "Create separate tunnel" })
    .click();
  await page
    .getByText(
      "Moving this project's hostname DNS records to the new tunnel…",
      { exact: true }
    )
    .waitFor();
  await page
    .getByRole("main")
    .getByText(
      "DNS update failed. Retry to finish using the already-created tunnel.",
      { exact: true }
    )
    .waitFor();
  await page
    .getByText("Separate tunnel setup is unfinished", { exact: true })
    .waitFor();
  assert.equal(
    await page.getByRole("button", { exact: true, name: "Start" }).isDisabled(),
    true
  );
  await page.evaluate(() => {
    window.__separateMock.fail = false;
  });
  await page
    .locator("#app")
    .getByRole("button", { exact: true, name: "Create separate tunnel" })
    .click();
  await dialog
    .getByRole("button", { exact: true, name: "Finish setup and start" })
    .click();
  await page.getByRole("button", { exact: true, name: "Stop" }).waitFor();
  assert.equal(
    await page
      .getByText("Cannot start while hw is running", { exact: true })
      .count(),
    0
  );
  const calls = await page.evaluate(() =>
    window.__separateMock.calls.filter(
      (c) => c.cmd === "create_separate_tunnel"
    )
  );
  assert.equal(calls.length, 2);
  assert.ok(
    calls.every(
      (call) =>
        call.args.id === "newlycitizen" &&
        call.args.revision === "review-1" &&
        call.args.requestId
    )
  );
  assert.deepEqual(errors, []);
  console.log(
    "Separate tunnel UI passed: conflict, review, cancel, progress, failure, retry and start."
  );
} finally {
  await browser.close();
}
