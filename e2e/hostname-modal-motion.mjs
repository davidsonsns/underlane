// Run with Vite on port 1420. PLAYWRIGHT_MODULE can point to an installed Playwright module.
import assert from "node:assert/strict";
import {
  browserPage,
  installHostnameMock,
  installPageMock,
} from "./lib/browser.mjs";

const { baseUrl, browser, page, errors } = await browserPage({ webkit: false });
await installPageMock(page, installHostnameMock, {
  delay: Number(process.env.LOAD_DELAY_MS ?? 100),
  mockName: "__hostnameMock",
});
await page.goto(`${baseUrl}/#tunnels:synthetic`);
await page.getByRole("heading", { exact: true, name: "Hostnames" }).waitFor();

const samples = await page.evaluate(async () => {
  const frames = [];
  let frameId;
  const sample = () => {
    const e = document.querySelector('[role="dialog"]');
    if (e) {
      const b = e.getBoundingClientRect(),
        s = getComputedStyle(e);
      frames.push({
        height: b.height,
        loading: e.innerText.includes("Loading hostnames"),
        opacity: Number(s.opacity),
        y: b.y,
      });
    }
    frameId = requestAnimationFrame(sample);
  };
  frameId = requestAnimationFrame(sample);
  [...document.querySelectorAll("button")]
    .find((e) => e.textContent.trim() === "Manage hostnames")
    .click();
  await new Promise((r) => setTimeout(r, 2000));
  cancelAnimationFrame(frameId);
  return frames;
});
assert.ok(samples.length > 5, "Modal frame sampling missed the open");
const ys = samples.filter((f) => f.opacity > 0.02).map((f) => f.y);
const movement = Math.max(...ys) - Math.min(...ys);
console.log(
  JSON.stringify(
    {
      first: samples[0],
      frames: samples.length,
      last: samples.at(-1),
      loadCalls: await page.evaluate(
        () =>
          window.__hostnameMock.calls.filter((c) => c.cmd === "load_hostnames")
            .length
      ),
      loadingFrames: samples.filter((f) => f.loading).length,
      movement,
    },
    null,
    2
  )
);
assert.ok(
  movement <= 17,
  `Modal moved ${movement.toFixed(1)}px, beyond the shared sheet animation`
);
const heights = samples.map((f) => f.height);
assert.ok(
  Math.max(...heights) - Math.min(...heights) < 1,
  "Modal changed height while opening"
);
assert.ok(
  samples.every((f) => !f.loading),
  "Short loading panel flashed before editor"
);
const hostnameTop = samples.at(-1).y;
await page.getByRole("button", { exact: true, name: "Cancel" }).click();
await page.getByRole("dialog").waitFor({ state: "detached" });
await page
  .getByRole("button", { exact: true, name: "New project or group" })
  .click();
await page.getByRole("menuitem", { exact: true, name: "New project" }).click();
await page.getByRole("dialog").waitFor();
await page.waitForTimeout(300);
const addTop = await page
  .getByRole("dialog")
  .evaluate((e) => e.getBoundingClientRect().top);
assert.ok(
  Math.abs(hostnameTop - addTop) < 1,
  `Manage hostnames top ${hostnameTop}px differs from New ${addTop}px`
);
console.log(`Both sheets align at ${addTop}px`);
assert.deepEqual(errors, [], "Unexpected browser error");
await browser.close();
