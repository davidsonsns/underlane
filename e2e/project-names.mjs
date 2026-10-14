// Inline project renaming regression. Run with Vite on port 1420.
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

const project = () =>
  page.getByRole("textbox", { exact: true, name: "Group name" });
const saved = (name) =>
  page.waitForFunction(
    (expected) => window.__projectMock.overview.groups[0].name === expected,
    name
  );
const field = () =>
  page.getByRole("textbox", { exact: true, name: "Group name" });
const calls = () =>
  page.evaluate(() =>
    window.__projectMock.calls.filter((c) => c.cmd === "rename_group")
  );
await project().waitFor();
const initial = await project().evaluate((e) => ({
  height: e.getBoundingClientRect().height,
  size: getComputedStyle(e).fontSize,
  tag: e.tagName,
}));
await project().click();
assert.equal(
  await project().evaluate((e) => e.tagName),
  "DIV",
  "Click keeps the editable tag"
);
assert.equal(
  await project().evaluate((e) => getComputedStyle(e).fontSize),
  initial.size,
  "Click preserves typography"
);
assert.equal(
  await project().evaluate((e) => e.getBoundingClientRect().height),
  initial.height,
  "Click preserves layout"
);
assert.equal(
  await page.locator('input[aria-label="Group name"]').count(),
  0,
  "No input replaces the heading"
);
await field().fill("  New   project  ");
await field().press("Enter");
await saved("New project");
await page.waitForFunction(
  () =>
    document
      .querySelector('[aria-label="Group name"]')
      .getAttribute("aria-busy") === "false"
);
assert.equal((await calls()).length, 1, "Enter saves once despite blur");
assert.equal((await calls())[0].args.name, "New project");
assert.equal(
  await project().evaluate((e) => document.activeElement === e),
  true,
  "Enter restores focus"
);
await project().click();
await field().fill("Cancelled");
await field().press("Escape");
assert.equal((await calls()).length, 1, "Escape never saves");
await project().click();
await field().press("Tab");
await project().waitFor();
assert.equal((await calls()).length, 1, "Untouched blur never saves");
await project().click();
await field().fill("a".repeat(65));
await field().press("Enter");
await page.getByText("Use 64 characters or fewer.", { exact: true }).waitFor();
assert.equal((await calls()).length, 1, "Oversized name never saves");
await field().fill("   ");
await field().press("Enter");
await page.getByText("Enter a name.", { exact: true }).waitFor();
assert.equal((await calls()).length, 1, "Blank name never saves");
await field().fill("Blur save");
await field().press("Tab");
await saved("Blur save");
await page.waitForFunction(
  () =>
    document
      .querySelector('[aria-label="Group name"]')
      .getAttribute("aria-busy") === "false"
);
assert.equal((await calls()).length, 2, "Edited blur saves once");
await page.evaluate(() => {
  window.__projectMock.failRename = true;
});
await project().click();
await field().fill("Retry name");
await field().press("Enter");
await page.getByText("Could not save project name", { exact: true }).waitFor();
assert.equal(
  await field().textContent(),
  "Retry name",
  "Failure preserves draft"
);
await page.evaluate(() => {
  window.__projectMock.failRename = false;
});
await field().press("Enter");
await saved("Retry name");
await page.waitForFunction(
  () =>
    document
      .querySelector('[aria-label="Group name"]')
      .getAttribute("aria-busy") === "false"
);
const long = `${"Long project name ".repeat(3)}1234567890`;
assert.equal(long.length, 64);
await project().click();
await field().fill(long);
await field().press("Enter");
await saved(long);
await page.waitForFunction(
  () =>
    document
      .querySelector('[aria-label="Group name"]')
      .getAttribute("aria-busy") === "false"
);
await page.locator("[data-tunnel-id=synthetic]").click();
const bounds = await project().evaluate((e) => ({
  overflow: getComputedStyle(e).textOverflow,
  scroll: e.scrollWidth,
  width: e.clientWidth,
}));
assert.ok(
  bounds.scroll > bounds.width,
  "Long heading overflows available width"
);
assert.equal(bounds.overflow, "ellipsis", "Long heading shows ellipsis");
await page.mouse.move(700, 500);
await project().hover();
await page.locator("[data-slot=tooltip-content]").waitFor();
assert.ok(
  (await page.locator("[data-slot=tooltip-content]").innerText()).includes(
    long
  ),
  "Tooltip contains full name"
);
await page.evaluate(() => {
  window.__projectMock.renameDelay = 500;
});
await project().click();
await field().fill("Focus retained");
await field().press("Enter");
const row = page.locator("[data-tunnel-id=synthetic]");
await row.click();
await saved("Focus retained");
await page.waitForFunction(
  () =>
    document
      .querySelector('[aria-label="Group name"]')
      .getAttribute("aria-busy") === "false"
);
assert.equal(
  await row.evaluate((e) => document.activeElement === e),
  true,
  "Pending Enter save does not steal subsequent focus"
);
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
  "Project rename passed: Enter, Escape, blur, limits, retry, focus, ellipsis, tooltip"
);
