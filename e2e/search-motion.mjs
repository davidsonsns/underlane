// Run with Vite on port 1420 (or E2E_BASE_URL).
import assert from "node:assert/strict";
import {
  browserPage,
  installHostnameMock,
  installPageMock,
} from "./lib/browser.mjs";

const { baseUrl, browser, page, errors } = await browserPage({ webkit: true });
async function checkSearch(view, reducedMotion) {
  await page.emulateMedia({ reducedMotion });
  const label = `Search ${view}`;
  await page.goto(`${baseUrl}/?search-motion=${view}-${reducedMotion}#${view}`);
  const trigger = page.getByRole("button", { exact: true, name: label });
  const input = page.getByRole("textbox", { exact: true, name: label });
  const openSearch = async () => {
    await trigger.click();
    await input.fill("needle");
    await page.waitForTimeout(250);
  };
  await openSearch();

  const frames = await page.evaluate(async (name) => {
    const button = document.querySelector(`button[aria-label="${name}"]`);
    const box = button.parentElement;
    const samples = [];
    const sample = () => {
      const bounds = button.getBoundingClientRect();
      const style = getComputedStyle(button);
      const field = box.querySelector(
        '[data-slot="input-group"]'
      )?.parentElement;
      samples.push({
        fieldOpacity: field ? Number(getComputedStyle(field).opacity) : 0,
        iconVisible: style.display !== "none" && Number(style.opacity) > 0.05,
        iconX: bounds.x,
        width: box.getBoundingClientRect().width,
      });
    };
    sample();
    document.querySelector('button[aria-label="Close search"]').click();
    await new Promise((resolve) => {
      const start = performance.now();
      const next = () => {
        sample();
        if (performance.now() - start < 350) {
          requestAnimationFrame(next);
        } else {
          resolve();
        }
      };
      requestAnimationFrame(next);
    });
    return samples;
  }, label);

  assert.ok(
    frames.every((frame) => frame.width <= frames[0].width + 1),
    `${label}: closing search briefly grows before shrinking`
  );
  assert.equal(frames.at(-1).width, 28);
  assert.ok(
    frames.every((frame) => !frame.iconVisible || frame.fieldOpacity < 0.1),
    `${label}: search icon overlaps the closing field`
  );
  assert.ok(
    frames
      .filter((frame) => frame.iconVisible)
      .every((frame) => Math.abs(frame.iconX - frames.at(-1).iconX) < 1),
    `${label}: returning search icon moves across the toolbar`
  );
  assert.equal(
    await trigger.evaluate((e) => e === document.activeElement),
    true
  );
  assert.equal(await input.count(), 0);

  await openSearch();
  assert.equal(await input.inputValue(), "needle");
  await input.press("Escape");
  await input.waitFor({ state: "detached" });
  assert.equal(
    await trigger.evaluate((e) => e === document.activeElement),
    true
  );
  await trigger.click();
  assert.equal(await input.inputValue(), "");
  await input.fill("keep me");
  await page.getByRole("heading", { level: 1 }).click();
  await input.waitFor({ state: "detached" });
  await trigger.click();
  assert.equal(await input.inputValue(), "keep me");
  console.log(
    `${label}: smooth close, focus, Escape, and outside click (${reducedMotion})`
  );
}

try {
  await installPageMock(page, installHostnameMock, {
    delay: 0,
    mockName: "__searchMock",
  });
  await checkSearch("requests", "no-preference");
  await checkSearch("logs", "no-preference");
  await checkSearch("requests", "reduce");
  await checkSearch("logs", "reduce");
  assert.deepEqual(errors, [], "Unexpected browser error");
} finally {
  await browser.close();
}
