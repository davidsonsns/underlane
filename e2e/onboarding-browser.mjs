// Vite preview with synthetic IPC; never installs a binary or authorizes a real account.
import assert from "node:assert/strict";
import { mkdir } from "node:fs/promises";
import { browserPage, installPageMock } from "./lib/browser.mjs";

const { baseUrl, browser, errors } = await browserPage({
  height: 760,
  timeout: 7000,
});

function fixture({
  installed = false,
  complete = false,
  existing = false,
  theme = "light",
} = {}) {
  const binary = {
    path: "/synthetic/cloudflared",
    source: "managed",
    version: "2026.9.1",
  };
  const overview = {
    active_count: 0,
    cloudflared: installed ? binary : null,
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
      theme,
    },
    tunnels: [],
  };
  const m = {
    calls: [],
    complete,
    existing,
    failures: new Set(),
    loginMode: "success",
    overview,
    pendingLogin: null,
  };
  window.__onboarding = m;
  window.__mockDispatch = (cmd, args) => {
    m.calls.push({ args, cmd });
    const handlers = {
      add_quick_tunnel: () => {
        if (!args.origin.includes("localhost")) {
          throw new Error("Enter a local HTTP address.");
        }
        overview.tunnels.push({
          autostart: false,
          config_path: null,
          created_at: 1,
          group_id: null,
          group_name: "",
          has_token: false,
          id: "first",
          inspect: false,
          kind: "quick",
          loglevel: "info",
          name: "localhost:3000",
          origin_url: args.origin,
          sort: 0,
          state: {
            config_revision: 0,
            connections: 0,
            detail: null,
            failure: null,
            hint: null,
            id: "first",
            inspecting: false,
            mappings: [],
            metrics_addr: null,
            pid: null,
            public_url: null,
            restarts: 0,
            run: 1,
            started_at: null,
            status: "stopped",
            supervised: false,
          },
        });
        return "first";
      },
      cancel_onboarding_login: () => {
        m.pendingLogin?.();
        return null;
      },
      check_config_change: () => null,
      complete_onboarding: () => {
        m.complete = true;
        return null;
      },
      connect_cloudflare: () => {
        if (m.loginMode === "failure") {
          throw new Error("Could not verify Cloudflare access.");
        }
        if (m.loginMode === "pending") {
          return new Promise((resolve) => {
            m.pendingLogin = resolve;
          });
        }
        return null;
      },
      detect_cloudflared: () => overview.cloudflared,
      install_cloudflared: () => {
        if (m.failures.has("install")) {
          throw new Error("Download failed. Retry installation.");
        }
        overview.cloudflared = binary;
        return binary;
      },
      login_item_status: () => ({
        enabled: false,
        message: null,
        status: "unsupported",
      }),
      onboarding_status: () => ({
        account_connected: m.existing,
        complete: m.complete,
      }),
      overview: () => structuredClone(overview),
      probe_services: () => args.services.map(() => false),
      project_directory: () => "/synthetic/projects",
      reset_app: async () => {
        if (m.failures.has("reset")) {
          throw new Error(
            "Finish the current Cloudflare operation before resetting Underlane."
          );
        }
        await new Promise((resolve) => setTimeout(resolve, 150));
        m.complete = false;
        overview.tunnels = [];
        overview.groups = [];
        overview.cloudflared = null;
        overview.settings.theme = "system";
        window.__mockListeners["app-reset"]?.({ payload: null });
        return null;
      },
      scan_default: () => [],
      select_cloudflared: () => {
        overview.cloudflared = binary;
        return binary;
      },
      start_tunnel: () => {
        if (m.failures.has("start")) {
          throw new Error("Could not start. Try again.");
        }
        Object.assign(overview.tunnels[0].state, {
          connections: 1,
          public_url: "https://synthetic.trycloudflare.com",
          status: "connected",
          supervised: true,
        });
        return null;
      },
      stop_tunnel: () => {
        Object.assign(overview.tunnels[0].state, {
          public_url: null,
          status: "stopped",
          supervised: false,
        });
        return null;
      },
    };
    if (handlers[cmd]) {
      return handlers[cmd]();
    }
    if (cmd.startsWith("plugin:")) {
      return null;
    }
    throw new Error(`Unmocked command: ${cmd}`);
  };
}

async function fresh(options) {
  const p = await browser.newPage({ viewport: { height: 760, width: 1000 } });
  p.setDefaultTimeout(7000);
  p.on("pageerror", (error) => errors.push(String(error)));
  await installPageMock(p, fixture, options ?? {});
  await p.goto(baseUrl);
  return p;
}

try {
  await mkdir("e2e/out/onboarding", { recursive: true });
  const quick = await fresh();
  await quick
    .getByRole("heading", { name: "Your localhost, one link away." })
    .waitFor();
  await quick.screenshot({ path: "e2e/out/onboarding/welcome-light.png" });
  await quick.getByRole("button", { name: "Get started" }).click();
  await quick.getByText("cloudflared wasn’t found", { exact: true }).waitFor();
  assert.equal(
    await quick.getByRole("button", { exact: true, name: "Continue" }).count(),
    0
  );
  await quick.evaluate(() => {
    window.__onboarding.failures.add("install");
  });
  await quick.getByRole("button", { name: "Install for Underlane" }).click();
  await quick
    .getByRole("alert")
    .filter({ hasText: "Download failed" })
    .waitFor();
  await quick.evaluate(() => {
    window.__onboarding.failures.delete("install");
  });
  await quick.getByRole("button", { name: "Install for Underlane" }).click();
  await quick.getByText("cloudflared is ready", { exact: true }).waitFor();
  await quick.getByRole("button", { exact: true, name: "Continue" }).click();
  await quick
    .getByRole("button", { exact: true, name: "Use temporary link" })
    .click();
  await quick.getByLabel("Local service URL").fill("invalid");
  await quick.getByRole("button", { name: "Start temporary tunnel" }).click();
  await quick.getByRole("alert").filter({ hasText: "Enter a local" }).waitFor();
  await quick.getByLabel("Local service URL").fill("http://localhost:3000");
  await quick.evaluate(() => {
    window.__onboarding.failures.add("start");
  });
  await quick.getByRole("button", { name: "Start temporary tunnel" }).click();
  await quick
    .getByRole("alert")
    .filter({ hasText: "Could not start" })
    .waitFor();
  await quick.evaluate(() => {
    window.__onboarding.failures.delete("start");
  });
  await quick.getByRole("button", { exact: true, name: "Try again" }).click();
  await quick.getByRole("heading", { name: "Your link is ready." }).waitFor();
  assert.equal(
    await quick.evaluate(
      () =>
        window.__onboarding.calls.filter((c) => c.cmd === "add_quick_tunnel")
          .length
    ),
    2
  );
  assert.equal(
    await quick.evaluate(
      () =>
        window.__onboarding.calls.filter((c) => c.cmd === "connect_cloudflare")
          .length
    ),
    0
  );
  await quick.screenshot({ path: "e2e/out/onboarding/quick-success.png" });
  await quick
    .getByRole("button", { exact: true, name: "Open dashboard" })
    .click();
  await quick.getByRole("navigation", { name: "Sections" }).waitFor();
  assert.equal(await quick.evaluate(() => window.__onboarding.complete), true);
  await quick.evaluate(async () => {
    await window.__TAURI_INTERNALS__.invoke("overview", {});
  });

  await quick.getByRole("button", { exact: true, name: "Settings" }).click();
  await quick.evaluate(() => {
    localStorage.setItem("reset-check", "keep until success");
    sessionStorage.setItem("reset-check", "keep until success");
  });
  await quick.getByRole("button", { name: "Reset app…" }).click();
  const resetDialog = quick.getByRole("alertdialog");
  await resetDialog.waitFor();
  await quick.screenshot({
    animations: "disabled",
    path: "e2e/out/onboarding/reset-confirmation.png",
  });
  assert.equal(await quick.evaluate(() => window.__onboarding.complete), true);
  await resetDialog
    .getByRole("button", { exact: true, name: "Cancel" })
    .click();
  assert.equal(
    await quick.evaluate(
      () =>
        window.__onboarding.calls.filter((c) => c.cmd === "reset_app").length
    ),
    0
  );
  assert.equal(
    await quick.evaluate(() => localStorage.getItem("reset-check")),
    "keep until success"
  );
  await quick.getByRole("button", { name: "Reset app…" }).click();
  await quick.evaluate(() => window.__onboarding.failures.add("reset"));
  await resetDialog
    .getByRole("button", { name: "Erase data and reset" })
    .click();
  await resetDialog
    .getByRole("alert")
    .filter({ hasText: "Finish the current Cloudflare operation" })
    .waitFor();
  assert.equal(
    await quick.evaluate(() => window.__onboarding.overview.tunnels.length),
    1
  );
  assert.deepEqual(
    await quick.evaluate(() => [
      localStorage.getItem("reset-check"),
      sessionStorage.getItem("reset-check"),
      window.__onboarding.complete,
    ]),
    ["keep until success", "keep until success", true]
  );
  await quick.evaluate(() => window.__onboarding.failures.delete("reset"));
  let resetNavigations = 0;
  quick.on("request", (request) => {
    if (
      request.isNavigationRequest() &&
      request.frame() === quick.mainFrame()
    ) {
      resetNavigations += 1;
    }
  });
  await resetDialog
    .getByRole("button", { name: "Erase data and reset" })
    .click();
  await resetDialog.getByRole("button", { name: "Resetting…" }).waitFor();
  assert.equal(
    await resetDialog
      .getByRole("button", { exact: true, name: "Cancel" })
      .isDisabled(),
    true
  );
  await quick
    .getByRole("heading", { name: "Your localhost, one link away." })
    .waitFor();
  assert.equal(resetNavigations, 0, "Reset must keep the current page alive");
  assert.deepEqual(
    await quick.evaluate(() => [
      window.__onboarding.overview.tunnels.length,
      window.__onboarding.complete,
      localStorage.length,
      sessionStorage.length,
    ]),
    [0, false, 0, 0]
  );
  await quick.getByRole("button", { name: "Get started" }).click();
  await quick.getByText("cloudflared wasn’t found", { exact: true }).waitFor();
  await quick.getByRole("button", { name: "Install for Underlane" }).click();
  await quick.getByRole("button", { exact: true, name: "Continue" }).click();
  await quick
    .getByRole("button", { exact: true, name: "Use temporary link" })
    .click();
  await quick.getByLabel("Local service URL").fill("http://localhost:3000");
  await quick.getByRole("button", { name: "Start temporary tunnel" }).click();
  await quick.getByRole("button", { name: "Open dashboard" }).click();
  await quick.getByRole("navigation", { name: "Sections" }).waitFor();
  assert.equal(await quick.evaluate(() => window.__onboarding.complete), true);
  assert.equal(resetNavigations, 0);

  const installed = await fresh({ installed: true, theme: "dark" });
  await installed.getByRole("button", { name: "Get started" }).click();
  await installed.getByText("cloudflared is ready", { exact: true }).waitFor();
  assert.equal(
    await installed
      .getByRole("button", { name: "Install for Underlane" })
      .count(),
    0
  );
  await installed.screenshot({ path: "e2e/out/onboarding/connector-dark.png" });
  await installed
    .getByRole("button", { exact: true, name: "Continue" })
    .click();
  await installed
    .getByRole("button", { exact: true, name: "Connect Cloudflare" })
    .click();
  await installed.evaluate(() => {
    window.__onboarding.loginMode = "failure";
  });
  await installed.getByRole("button", { name: "Connect in browser" }).click();
  await installed
    .getByRole("alert")
    .filter({ hasText: "Could not verify" })
    .waitFor();
  await installed.evaluate(() => {
    window.__onboarding.loginMode = "pending";
  });
  await installed.getByRole("button", { name: "Connect in browser" }).click();
  await installed.getByRole("button", { name: "Cancel authorization" }).click();
  await installed
    .getByRole("alert")
    .filter({ hasText: "Authorization cancelled" })
    .waitFor();
  assert.equal(
    await installed
      .getByRole("heading", { name: "Cloudflare connected." })
      .count(),
    0
  );
  await installed
    .getByRole("button", { name: "Use a temporary link instead" })
    .click();
  await installed.getByLabel("Local service URL").waitFor();
  await installed.getByRole("button", { exact: true, name: "Back" }).click();
  await installed
    .getByRole("button", { exact: true, name: "Connect Cloudflare" })
    .click();
  await installed.evaluate(() => {
    window.__onboarding.loginMode = "success";
  });
  await installed.getByRole("button", { name: "Connect in browser" }).click();
  await installed
    .getByRole("heading", { name: "Cloudflare connected." })
    .waitFor();
  await installed.getByRole("button", { name: "Create a project" }).click();
  await installed.getByRole("dialog").waitFor();

  const external = await fresh();
  await external.getByRole("button", { name: "Get started" }).click();
  await external
    .getByText("cloudflared wasn’t found", { exact: true })
    .waitFor();
  await external.evaluate(() => {
    window.__onboarding.overview.cloudflared = {
      path: "/synthetic/external",
      source: "path",
      version: "2026.9.1",
    };
  });
  await external.evaluate(() => window.dispatchEvent(new Event("focus")));
  await external.getByText("cloudflared is ready", { exact: true }).waitFor();
  await external.setViewportSize({ height: 614, width: 620 });
  await external.screenshot({ path: "e2e/out/onboarding/connector-small.png" });
  assert.equal(
    await external.evaluate(
      () => document.documentElement.scrollWidth > innerWidth
    ),
    false
  );
  const legacy = await fresh({ complete: true });
  await legacy.getByRole("navigation", { name: "Sections" }).waitFor();
  assert.equal(
    await legacy.getByRole("button", { name: "Get started" }).count(),
    0
  );
  const existing = await fresh({ existing: true, installed: true });
  await existing.getByRole("button", { name: "Get started" }).click();
  await existing.getByRole("button", { exact: true, name: "Continue" }).click();
  await existing
    .getByRole("button", { exact: true, name: "Connect Cloudflare" })
    .click();
  await existing
    .getByText(
      "Existing authorization found. We’ll verify it before continuing."
    )
    .waitFor();
  await existing
    .getByRole("button", { name: "Verify existing authorization" })
    .waitFor();
  assert.deepEqual(errors, []);
  console.log(
    "ONBOARDING BROWSER PASSED: detection, installation retry, quick start/retry, optional login/cancel, existing authorization, completion, in-process reset confirmation/cancel/error/retry and repeat onboarding without reload, legacy users, dark/light and small window."
  );
} finally {
  await browser.close();
}
