// Synthetic opacity regression. Run with Vite on port 1420 (BROWSER=webkit optional).
import assert from "node:assert/strict";
import { mkdir } from "node:fs/promises";
import {
  browserPage,
  installHostnameMock,
  installPageMock,
} from "./lib/browser.mjs";

const { baseUrl, browser, page, errors } = await browserPage({
  webkit: process.env.BROWSER === "webkit",
});

function extendOpacityMock({ mockName }) {
  const mock = window[mockName];
  const dispatch = window.__mockDispatch;
  const persisted = localStorage.getItem("opacity-test-settings");
  if (persisted) {
    mock.overview.settings = JSON.parse(persisted);
  }
  mock.emitSettings = () =>
    window.__mockListeners["settings-changed"]?.({
      event: "settings-changed",
      payload: structuredClone(mock.overview.settings),
    });
  window.__mockDispatch = async (cmd, args) => {
    if (cmd !== "save_settings") {
      return dispatch(cmd, args);
    }
    mock.calls.push({ args: structuredClone(args), cmd });
    await new Promise((resolve) => setTimeout(resolve, 100));
    if (mock.failSave) {
      throw new Error("Synthetic settings save failure");
    }
    mock.overview.settings = structuredClone(args.settings);
    localStorage.setItem(
      "opacity-test-settings",
      JSON.stringify(args.settings)
    );
    mock.emitSettings();
    return null;
  };
}

await installPageMock(
  page,
  installHostnameMock,
  {
    delay: 0,
    mockName: "__opacityMock",
  },
  extendOpacityMock
);

const slider = page.getByRole("slider", { name: "Background opacity" });
const settings = () =>
  page
    .locator("aside")
    .getByRole("button", { exact: true, name: "Settings" })
    .click();
const waitSaved = (opacity) =>
  page.waitForFunction(
    (expected) =>
      window.__opacityMock.overview.settings.background_opacity === expected,
    opacity
  );
const preview = async (opacity) => {
  await slider.focus();
  await slider.evaluate((input, value) => {
    input.value = String(value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  }, opacity);
};
const commit = () => slider.dispatchEvent("change");
async function assertCanvas(opacity) {
  const actual = await page.getByRole("main").evaluate((main) => {
    const canvas = document.createElement("canvas");
    const context = canvas.getContext("2d");
    const alpha = (element) => {
      context.clearRect(0, 0, 1, 1);
      context.fillStyle = getComputedStyle(element).backgroundColor;
      context.fillRect(0, 0, 1, 1);
      return context.getImageData(0, 0, 1, 1).data[3];
    };
    return {
      canvas: alpha(main),
      card: alpha(main.querySelector("section > div.rounded-xl")),
      control: alpha(main.querySelector('[data-field="path"]')),
    };
  });
  for (const surface of ["canvas", "card"]) {
    assert.ok(
      Math.abs(actual[surface] - Math.round(opacity * 2.55)) <= 1,
      `${surface} opacity should be ${opacity}%, received alpha ${actual[surface]}`
    );
  }
  assert.equal(actual.control, 255, "Input backgrounds stay opaque");
  assert.equal(await slider.inputValue(), String(opacity));
}

try {
  await page.goto(baseUrl);
  await settings();
  await slider.waitFor();
  await assertCanvas(100);
  assert.equal(await slider.getAttribute("min"), "50");
  assert.equal(await slider.getAttribute("max"), "100");

  await preview(85);
  await assertCanvas(85);
  assert.equal(
    await page.evaluate(
      () =>
        window.__opacityMock.calls.filter(
          (call) => call.cmd === "save_settings"
        ).length
    ),
    0,
    "Dragging previews without writing settings on every input"
  );
  await page.evaluate(() => window.__opacityMock.emitSettings());
  await assertCanvas(85);
  await commit();
  await waitSaved(85);
  await assertCanvas(85);

  await slider.press("ArrowLeft");
  await waitSaved(84);
  assert.equal(
    await slider.evaluate((input) => input === document.activeElement),
    true,
    "Saving keeps keyboard focus on the slider"
  );
  await slider.press("ArrowLeft");
  await waitSaved(83);
  await slider.press("ArrowLeft");
  await slider.press("ArrowLeft");
  await slider.press("ArrowLeft");
  await waitSaved(80);
  await assertCanvas(80);

  await preview(50);
  await commit();
  await waitSaved(50);
  await assertCanvas(50);
  const solid = await page.evaluate(() => ({
    body: getComputedStyle(document.body).opacity,
    canvas: getComputedStyle(document.querySelector("main")).opacity,
    label: getComputedStyle(document.querySelector("h1")).opacity,
  }));
  assert.equal(solid.body, "1");
  assert.equal(solid.label, "1");
  assert.equal(solid.canvas, "1");

  await page.evaluate(() => {
    window.__opacityMock.failSave = true;
  });
  await preview(70);
  await commit();
  await page.getByText("Synthetic settings save failure").waitFor();
  await assertCanvas(50);
  await page.evaluate(() => {
    window.__opacityMock.failSave = false;
  });
  await preview(85);
  await commit();
  await waitSaved(85);
  await page.reload();
  await settings();
  await slider.waitFor();
  await assertCanvas(85);

  await mkdir("e2e/out", { recursive: true });
  await page.evaluate(() =>
    Promise.all(
      document
        .getAnimations()
        .map((animation) => animation.finished.catch(() => undefined))
    )
  );
  await page.screenshot({ path: "e2e/out/opacity-dark-85.png" });
  await page.getByLabel("Interface theme", { exact: true }).click();
  await page.getByRole("option", { exact: true, name: "Light" }).click();
  await page.waitForFunction(
    () => document.documentElement.dataset.theme === "light"
  );
  await assertCanvas(85);
  await page.screenshot({ path: "e2e/out/opacity-light-85.png" });
  await preview(60);
  await page
    .locator("aside")
    .getByRole("button", { exact: true, name: "Requests" })
    .click();
  await page.getByRole("heading", { exact: true, name: "Requests" }).waitFor();
  assert.equal(
    await page.evaluate(() =>
      document.documentElement.style.getPropertyValue(
        "--window-opacity-preview"
      )
    ),
    "",
    "Leaving Settings clears an uncommitted preview"
  );
  await settings();
  await slider.waitFor();
  await assertCanvas(85);
  await preview(100);
  await commit();
  await waitSaved(100);
  await assertCanvas(100);
  assert.deepEqual(errors, []);
  console.log(
    "Opacity: live preview, refresh, keyboard, bounds, rollback, reload, and themes passed."
  );
} finally {
  await browser.close();
}
