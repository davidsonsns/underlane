// Run against a production Vite preview so release-only update checks execute:
// pnpm build; pnpm --filter underlane exec vite preview --port 4173
// E2E_BASE_URL=http://localhost:4173 node e2e/app-update.mjs
import assert from "node:assert/strict";
import { mkdir } from "node:fs/promises";
import {
  browserPage,
  installHostnameMock,
  installPageMock,
} from "./lib/browser.mjs";

function updates({ theme, release = true }) {
  const mock = window.__updates;
  mock.overview.settings.theme = theme;
  mock.overview.tunnels[0].state.status = "starting";
  mock.overview.tunnels[0].autostart = false;
  mock.actions = [];
  const metadata = release
    ? { currentVersion: "0.1.0", rawJson: {}, rid: 101, version: "0.2.0" }
    : null;
  const dispatch = window.__mockDispatch;
  window.__mockDispatch = (cmd, args) => {
    if (cmd === "plugin:updater|check") {
      return metadata;
    }
    if (cmd === "plugin:updater|download") {
      args.onEvent.onmessage({
        data: { contentLength: 100 },
        event: "Started",
      });
      args.onEvent.onmessage({ data: { chunkLength: 25 }, event: "Progress" });
      return new Promise((resolve) => {
        mock.finishDownload = () => {
          args.onEvent.onmessage({ event: "Finished" });
          resolve(102);
        };
      });
    }
    if (["plugin:updater|install", "restart_app"].includes(cmd)) {
      mock.actions.push(cmd);
      return null;
    }
    return dispatch(cmd, args);
  };
}

const { baseUrl, browser, errors } = await browserPage();
const output = new URL("./out/app-update/", import.meta.url);
await mkdir(output, { recursive: true });
try {
  await Promise.all(
    ["light", "dark"].map(async (theme) => {
      const page = await browser.newPage({
        viewport: { height: 760, width: 1280 },
      });
      page.on("pageerror", (error) => errors.push(String(error)));
      await installPageMock(
        page,
        installHostnameMock,
        { mockName: "__updates", theme },
        updates
      );
      await page.goto(baseUrl);
      const available = page.getByRole("button", {
        exact: true,
        name: "Update available",
      });
      await available.waitFor();
      await page.screenshot({
        path: new URL(`${theme}-available.png`, output).pathname,
      });
      await available.click();
      const downloading = page.getByRole("button", {
        exact: true,
        name: "Downloading… 25%",
      });
      await downloading.waitFor();
      assert.equal(await downloading.isDisabled(), true);
      assert.deepEqual(await page.evaluate(() => window.__updates.actions), []);
      await page.evaluate(() => window.__updates.finishDownload());
      const ready = page.getByRole("button", {
        exact: true,
        name: "Restart to update",
      });
      await ready.click();
      const dialog = page.getByRole("alertdialog");
      await dialog.waitFor();
      assert.ok(
        (await dialog.innerText()).includes("stopped projects stay stopped")
      );
      assert.deepEqual(await page.evaluate(() => window.__updates.actions), []);
      await page.getByRole("button", { exact: true, name: "Later" }).click();
      await ready.waitFor();
      await ready.click();
      await dialog.waitFor();
      await page.screenshot({
        animations: "disabled",
        path: new URL(`${theme}-confirm.png`, output).pathname,
      });
      await page
        .getByRole("button", { exact: true, name: "Restart and update" })
        .click();
      await page.waitForFunction(() => window.__updates.actions.length === 2);
      assert.deepEqual(await page.evaluate(() => window.__updates.actions), [
        "plugin:updater|install",
        "restart_app",
      ]);
      await page.close();
    })
  );
  const page = await browser.newPage();
  await installPageMock(
    page,
    installHostnameMock,
    { mockName: "__updates", release: false, theme: "light" },
    updates
  );
  await page.goto(baseUrl);
  await page.getByRole("navigation", { name: "Sections" }).waitFor();
  assert.equal(
    await page
      .getByRole("button", { exact: true, name: "Update available" })
      .count(),
    0
  );
  assert.deepEqual(errors, []);
  console.log(
    "Update UI passed: both themes, progress, deferred install, starting-project confirmation, cancel/retry, and no update."
  );
} finally {
  await browser.close();
}
