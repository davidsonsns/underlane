// Run against Vite on port 1420; all Cloudflare and Tauri calls are synthetic.
import assert from "node:assert/strict";
const { chromium } = await import(process.env.PLAYWRIGHT_MODULE ?? "playwright");
const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
const errors = [];
page.on("pageerror", (error) => errors.push(String(error)));
page.setDefaultTimeout(7000);
await page.addInitScript(() => {
  const oldId = "00000000-0000-4000-8000-000000000001";
  const newId = "00000000-0000-4000-8000-000000000002";
  const route = { hostname: "newlycitizen.example.com", path: null, service: "http://localhost:8787", inspected: false, note: null };
  function project(id, name, running) {
    return {
      id, name, kind: "config", group_id: null, group_name: "", config_path: `/synthetic/${id}.yml`,
      origin_url: null, inspect: false, autostart: false, loglevel: "info", created_at: 1, sort: 0,
      has_token: false, setup_needed: false, cloudflare_tunnel_id: oldId,
      state: { id, status: running ? "connected" : "stopped", supervised: running,
        detail: null, hint: null, failure: null, tunnel_conflict: null,
        cloudflare_tunnel_id: running ? oldId : null, public_url: null, mappings: [route],
        pid: running ? 123 : null, metrics_addr: null, connections: running ? 4 : 0,
        restarts: 0, started_at: null, inspecting: false, run: 1, config_revision: 0 },
    };
  }
  const selected = project("newlycitizen", "newlycitizen", false);
  const overview = { groups: [], tunnels: [project("example", "example", true), selected],
    cloudflared: { path: "/synthetic/cloudflared", version: "2026.9.1", source: "mock" },
    settings: { cloudflared_path: null, log_ttl_days: 7, max_db_mb: 100, max_body_kb: 1024,
      redact_headers: [], redact_query: [], theme: "dark", default_loglevel: "info" },
    active_count: 1, dropped_captures: 0, data_dir: "/synthetic", last_launch: null };
  const callbacks = {}, listeners = {};
  let next = 1;
  window.__separateMock = { overview, calls: [], fail: true };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
  window.__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label: "dashboard" }, currentWebview: { label: "dashboard" } },
    transformCallback(cb) { const id = next++; callbacks[id] = cb; return id; },
    unregisterCallback(id) { delete callbacks[id]; },
    async invoke(cmd, args) {
      const mock = window.__separateMock;
      mock.calls.push({ cmd, args });
      if (cmd === "overview") return structuredClone(overview);
      if (cmd === "preview_separate_tunnel") return { revision: "review-1", old_tunnel_id: oldId,
        hostnames: [route.hostname], config_path: selected.config_path, resuming: !mock.fail };
      if (cmd === "create_separate_tunnel") {
        const emit = (stage) => listeners["project-separate-progress"]?.({ event: "project-separate-progress", id: 1,
          payload: { id: args.id, request_id: args.requestId, stage } });
        emit("dns");
        await new Promise((resolve) => setTimeout(resolve, 250));
        if (mock.fail) {
          selected.separate_tunnel_pending = true;
          throw "DNS update failed. Retry to finish using the already-created tunnel.";
        }
        selected.separate_tunnel_pending = false;
        selected.cloudflare_tunnel_id = newId;
        Object.assign(selected.state, { status: "starting", supervised: true, cloudflare_tunnel_id: newId });
        return { tunnel_id: newId, config_path: selected.config_path, moved_hostnames: [route.hostname], start_error: null };
      }
      if (cmd === "plugin:event|listen") { listeners[args.event] = callbacks[args.handler]; return next++; }
      if (cmd === "probe_services") return args.services.map(() => true);
      if (cmd === "scan_default") return [];
      if (cmd === "check_config_change") return null;
      if (cmd === "login_item_status") return { status: "unsupported", enabled: false, message: null };
      if (cmd.startsWith("plugin:")) return null;
      return null;
    },
  };
});
try {
  await page.goto("http://127.0.0.1:1420/#tunnels:newlycitizen", { waitUntil: "domcontentloaded", timeout: 30000 });
  await page.getByText("Cannot start while hw is running", { exact: true }).waitFor();
  await assert.equal(await page.getByRole("button", { name: "Start", exact: true }).isDisabled(), true);
  await page.locator("#app").getByRole("button", { name: "Create separate tunnel", exact: true }).click();
  const dialog = page.getByRole("alertdialog");
  await dialog.waitFor();
  assert.match(await dialog.innerText(), /newlycitizen\.example\.com/);
  assert.match(await dialog.innerText(), /moves the hostname DNS records/);
  await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
  await dialog.waitFor({ state: "detached" });
  assert.equal(await page.evaluate(() => window.__separateMock.calls.filter((c) => c.cmd === "create_separate_tunnel").length), 0);
  await page.locator("#app").getByRole("button", { name: "Create separate tunnel", exact: true }).click();
  await dialog.getByRole("button", { name: "Create separate tunnel", exact: true }).click();
  await page.getByText("Moving this project's hostname DNS records to the new tunnel…", { exact: true }).waitFor();
  await page.getByRole("main").getByText("DNS update failed. Retry to finish using the already-created tunnel.", { exact: true }).waitFor();
  await page.getByText("Separate tunnel setup is unfinished", { exact: true }).waitFor();
  assert.equal(await page.getByRole("button", { name: "Start", exact: true }).isDisabled(), true);
  await page.evaluate(() => { window.__separateMock.fail = false; });
  await page.locator("#app").getByRole("button", { name: "Create separate tunnel", exact: true }).click();
  await dialog.getByRole("button", { name: "Finish setup and start", exact: true }).click();
  await page.getByRole("button", { name: "Stop", exact: true }).waitFor();
  assert.equal(await page.getByText("Cannot start while hw is running", { exact: true }).count(), 0);
  const calls = await page.evaluate(() => window.__separateMock.calls.filter((c) => c.cmd === "create_separate_tunnel"));
  assert.equal(calls.length, 2);
  assert.ok(calls.every((call) => call.args.id === "newlycitizen" && call.args.revision === "review-1" && call.args.requestId));
  assert.deepEqual(errors, []);
  console.log("Separate tunnel UI passed: conflict, review, cancel, progress, failure, retry and start.");
} finally {
  await browser.close();
}
