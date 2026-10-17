// Group dialog and inline project renaming regression. Run with Vite on port 1420.
// PLAYWRIGHT_MODULE can point to an installed Playwright module; BROWSER=webkit checks WebKit.
import assert from "node:assert/strict";
import {
  browserPage,
  installHostnameMock,
  installPageMock,
} from "./lib/browser.mjs";

const { baseUrl, browser, page, errors } = await browserPage({
  webkit: process.env.BROWSER === "webkit",
});
await installPageMock(page, installHostnameMock, {
  delay: Number(process.env.LOAD_DELAY_MS ?? 100),
  mockName: "__projectMock",
});
await page.goto(`${baseUrl}/#tunnels:synthetic`);

const trigger = page.locator('[data-group-menu="group"]');
const dialog = page.getByRole("dialog", { exact: true, name: "Rename group" });
const field = () =>
  dialog.getByRole("textbox", { exact: true, name: "Group name" });
const saved = (name) =>
  page.waitForFunction(
    (expected) => window.__projectMock.overview.groups[0].name === expected,
    name
  );
const calls = () =>
  page.evaluate(() =>
    window.__projectMock.calls.filter((c) => c.cmd === "rename_group")
  );
async function openRename() {
  await trigger.click();
  await page
    .getByRole("menuitem", { exact: true, name: "Rename group" })
    .click();
  await dialog.waitFor();
  await field().waitFor();
}
async function closedWithFocus() {
  await dialog.waitFor({ state: "hidden" });
  await page.waitForFunction(
    () =>
      document.activeElement ===
      document.querySelector('[data-group-menu="group"]')
  );
}
await trigger.waitFor();
await openRename();
assert.equal(
  await field().inputValue(),
  "254",
  "Dialog starts with the group name"
);
await field().fill("  New   group  ");
await field().press("Enter");
await saved("New group");
await closedWithFocus();
assert.equal((await calls()).length, 1, "Enter saves once");
assert.equal(
  (await calls())[0].args.name,
  "New group",
  "Whitespace is normalized"
);

await openRename();
await field().fill("Cancelled");
await dialog.getByRole("button", { exact: true, name: "Cancel" }).click();
await closedWithFocus();
assert.equal((await calls()).length, 1, "Cancel never saves");
await openRename();
assert.equal(
  await field().inputValue(),
  "New group",
  "Cancel discards the draft"
);
await field().fill("Escape cancelled");
await field().press("Escape");
await closedWithFocus();
assert.equal((await calls()).length, 1, "Escape never saves");

await openRename();
await field().fill("a".repeat(65));
assert.equal(
  await dialog
    .getByRole("button", { exact: true, name: "Save changes" })
    .isDisabled(),
  true,
  "Oversized names cannot save"
);
await field().press("Enter");
assert.equal((await calls()).length, 1, "Oversized name never writes");
await field().fill("   ");
assert.equal(
  await dialog
    .getByRole("button", { exact: true, name: "Save changes" })
    .isDisabled(),
  true,
  "Blank names cannot save"
);
await field().press("Enter");
assert.equal((await calls()).length, 1, "Blank name never writes");
const long = "L".repeat(64);
assert.equal(long.length, 64);
await field().fill(long);
await field().press("Enter");
await saved(long.trim());
await closedWithFocus();
assert.equal((await calls()).length, 2, "A 64-character draft can save");

await page.evaluate(() => {
  window.__projectMock.failRename = true;
});
await openRename();
await field().fill("Retry name");
await field().press("Enter");
await dialog
  .getByText("Could not save project name", { exact: true })
  .waitFor();
assert.equal(
  await field().inputValue(),
  "Retry name",
  "Failure preserves the draft"
);
await page.evaluate(() => {
  window.__projectMock.failRename = false;
});
await field().press("Enter");
await saved("Retry name");
await closedWithFocus();
assert.equal(
  (await calls()).length,
  4,
  "A backend failure permits one explicit retry"
);

await page.evaluate(() => {
  const dispatch = window.__mockDispatch;
  window.__mockDispatch = (cmd, args) => {
    if (cmd === "overview" && window.__projectMock.failRefresh) {
      throw new Error("Refresh unavailable");
    }
    return dispatch(cmd, args);
  };
  window.__projectMock.failRefresh = true;
});
await openRename();
await field().fill("Saved before refresh");
await field().press("Enter");
await dialog
  .getByText("Group saved. Refresh unavailable", { exact: true })
  .waitFor();
assert.equal(
  await field().isDisabled(),
  true,
  "A saved draft stays frozen until refreshed"
);
assert.equal(
  (await calls()).length,
  5,
  "Successful write happens before refresh failure"
);
await page.evaluate(() => {
  window.__projectMock.failRefresh = false;
});
await dialog
  .getByRole("button", { exact: true, name: "Retry refresh" })
  .click();
await closedWithFocus();
assert.equal(
  (await calls()).length,
  5,
  "Refresh retry never repeats the write"
);

await page.evaluate(() => {
  window.__projectMock.renameDelay = 500;
});
await openRename();
await field().fill("Busy save");
await field().press("Enter");
await page.waitForFunction(
  () =>
    document
      .querySelector('[role="dialog"] form')
      ?.getAttribute("aria-busy") === "true"
);
await page.keyboard.press("Escape");
await page.mouse.click(1000, 425);
assert.equal(
  await dialog.isVisible(),
  true,
  "Busy dialog ignores Escape and outside clicks"
);
await saved("Busy save");
await closedWithFocus();
assert.equal((await calls()).length, 6, "Busy submission writes once");
const title = page.getByRole("textbox", { exact: true, name: "Project name" });
const before = await title.evaluate((e) => ({
  font: getComputedStyle(e).fontSize,
  height: e.getBoundingClientRect().height,
  tag: e.tagName,
}));
await title.click();
assert.deepEqual(
  await title.evaluate((e) => ({
    font: getComputedStyle(e).fontSize,
    height: e.getBoundingClientRect().height,
    tag: e.tagName,
  })),
  before,
  "Tunnel title keeps its tag, typography and layout"
);
await title.fill("  New   tunnel  ");
await title.press("Enter");
await page.waitForFunction(
  () => window.__projectMock.overview.tunnels[0].name === "New tunnel"
);
assert.equal(await title.textContent(), "New tunnel");
assert.equal(
  await page.locator('input[aria-label="Project name"]').count(),
  0,
  "Tunnel title never becomes an input"
);
await title.fill("Cancelled tunnel");
await title.press("Escape");
assert.equal(await title.textContent(), "New tunnel");
assert.equal(
  await title.evaluate(
    (e) =>
      getComputedStyle(e).getPropertyValue("user-select") ||
      getComputedStyle(e).getPropertyValue("-webkit-user-select")
  ),
  "text",
  "Editable name supports text selection"
);
await title.fill("");
await title.evaluate((e) => {
  const data = new DataTransfer();
  data.setData("text/plain", "Pasted\n   tunnel");
  data.setData("text/html", "<b>Pasted</b><script>throw 1</script>");
  e.dispatchEvent(
    new ClipboardEvent("paste", {
      bubbles: true,
      cancelable: true,
      clipboardData: data,
    })
  );
});
assert.equal(
  await title.textContent(),
  "Pasted tunnel",
  "Pasted whitespace becomes a single line"
);
assert.equal(
  await title.locator("b, script").count(),
  0,
  "Pasted HTML never enters the editable tag"
);
await title.press("Enter");
await page.waitForFunction(
  () => window.__projectMock.overview.tunnels[0].name === "Pasted tunnel"
);
await title.fill("Outside click saved");
await page.mouse.click(1000, 425);
assert.equal(
  await title.evaluate((e) => document.activeElement === e),
  false,
  "Clicking blank space removes editor focus"
);
assert.equal(
  await title.evaluate((e) =>
    e.contains(window.getSelection()?.anchorNode ?? null)
  ),
  false,
  "Outside click clears the editor caret and selection"
);
await page.waitForFunction(
  () => window.__projectMock.overview.tunnels[0].name === "Outside click saved"
);
assert.deepEqual(errors, [], "No browser errors");
await browser.close();
console.log(
  "Group dialog and project rename passed: Enter, Cancel, Escape, limits, retry, refresh-only retry, busy dismissal, focus, inline title"
);
