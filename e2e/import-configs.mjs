// Run with Vite on port 1420 (or E2E_BASE_URL).
import assert from "node:assert/strict";
import {
  browserPage,
  installHostnameMock,
  installPageMock,
} from "./lib/browser.mjs";

const { baseUrl, browser, page, errors } = await browserPage({ webkit: true });
try {
  await installPageMock(page, installHostnameMock, {
    delay: 0,
    mockName: "__configMock",
  });
  await page.addInitScript(() => {
    const original = window.__mockDispatch;
    window.__mockDispatch = (cmd, args) => {
      const m = window.__configMock;
      const path = "/synthetic/.cloudflared/underlane/available.yaml";
      if (cmd === "plugin:dialog|open") {
        return m.pickerPath ?? null;
      }
      if (cmd === "scan_default") {
        return [
          { imported: m.imported ?? false, name: "Available", path },
          {
            imported: m.imported ?? false,
            name: "newlycitizen",
            path: "/synthetic/.cloudflared/underlane/newlycitizen.yml",
          },
          {
            imported: true,
            name: "Used",
            path: "/synthetic/.cloudflared/used.yml",
          },
        ];
      }
      if (cmd === "preview_project_config") {
        const names = {
          [path]: "Available",
          "/synthetic/.cloudflared/underlane/newlycitizen.yml": "newlycitizen",
          "/synthetic/custom.yml": "custom",
        };
        return {
          name: names[args.path],
          path: args.path,
          routes: [
            {
              hostname: "app.example.com",
              path: null,
              service: "http://localhost:3000",
            },
          ],
        };
      }
      if (cmd === "create_project") {
        m.created = args;
        m.imported = true;
        return "synthetic";
      }
      return original(cmd, args);
    };
  });
  await page.goto(`${baseUrl}/#tunnels:synthetic`);
  await page
    .getByRole("button", { exact: true, name: "New project or group" })
    .click();
  await page
    .getByRole("menuitem", { exact: true, name: "New project" })
    .click();
  const dialog = page.getByRole("dialog");
  const picker = dialog.getByLabel("Configuration", { exact: true });
  await picker.click();
  const option = page.getByRole("option", { name: "underlane/available.yaml" });
  await option.waitFor();
  assert.equal(await page.getByRole("option", { name: "used.yml" }).count(), 0);
  await option.click();
  await dialog.getByText("app.example.com", { exact: true }).waitFor();
  assert.equal(
    await dialog
      .getByRole("textbox", { exact: true, name: "Name" })
      .inputValue(),
    "Available"
  );
  await picker.click();
  await page
    .getByRole("option", { name: "underlane/newlycitizen.yml" })
    .click();
  await page.waitForFunction(
    () =>
      !document.querySelector('[role="dialog"] [data-slot="select-trigger"]')
        .disabled
  );
  assert.equal(
    await dialog
      .getByRole("textbox", { exact: true, name: "Name" })
      .inputValue(),
    "newlycitizen",
    "Changing configuration updates the project name"
  );
  await picker.click();
  await option.click();
  await dialog.getByText("app.example.com", { exact: true }).waitFor();
  await picker.click();
  await page.getByRole("option", { name: "Browse files…" }).click();
  await page.waitForFunction(
    () =>
      !document.querySelector('[role="dialog"] [data-slot="select-trigger"]')
        .disabled
  );
  assert.equal(
    await picker.textContent().then((text) => text.trim()),
    "available.yaml"
  );
  assert.equal(
    await dialog
      .getByRole("textbox", { exact: true, name: "Name" })
      .inputValue(),
    "Available",
    "Reselecting restores the config name and cancelling Browse preserves it"
  );
  await picker.click();
  await page
    .getByRole("option", { exact: true, name: "Create a new config" })
    .click();
  assert.equal(
    await dialog.getByText("app.example.com", { exact: true }).count(),
    0
  );
  await picker.click();
  await option.click();
  await dialog.getByText("app.example.com", { exact: true }).waitFor();
  await dialog
    .getByRole("button", { exact: true, name: "Create project" })
    .click();
  await dialog.waitFor({ state: "detached" });
  assert.deepEqual(await page.evaluate(() => window.__configMock.created), {
    configPath: "/synthetic/.cloudflared/underlane/available.yaml",
    directory: null,
    group: "",
    name: "Available",
  });
  await page
    .getByRole("button", { exact: true, name: "New project or group" })
    .click();
  await page
    .getByRole("menuitem", { exact: true, name: "New project" })
    .click();
  await picker.click();
  await page.getByRole("option", { name: "Browse files…" }).waitFor();
  assert.equal(await page.getByRole("option").count(), 2);
  assert.equal(
    await dialog
      .getByText("No unused valid configs found in ~/.cloudflared.")
      .count(),
    0
  );
  await page.getByRole("option", { name: "Browse files…" }).click();
  await page.waitForFunction(
    () =>
      !document.querySelector('[role="dialog"] [data-slot="select-trigger"]')
        .disabled
  );
  assert.equal((await picker.textContent()).trim(), "Create a new config");
  await page.evaluate(() => {
    window.__configMock.pickerPath = "/synthetic/custom.yml";
  });
  await picker.click();
  await page.getByRole("option", { name: "Browse files…" }).click();
  await dialog.getByText("app.example.com", { exact: true }).waitFor();
  assert.equal((await picker.textContent()).trim(), "custom.yml");
  assert.equal(
    await dialog
      .getByRole("textbox", { exact: true, name: "Name" })
      .inputValue(),
    "custom",
    "Browsing for a configuration fills the project name"
  );
  assert.deepEqual(errors, []);
  console.log(
    "Config chooser UI passed: unused selection, name updates, preview, cancellation, reset/reselect, create, no-candidates menu, and browse."
  );
} finally {
  await browser.close();
}
