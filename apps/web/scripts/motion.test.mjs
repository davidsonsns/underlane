import assert from "node:assert/strict";
import { after, before, test } from "node:test";
import { fileURLToPath } from "node:url";
import { preview } from "astro";
// biome-ignore lint/correctness/noUndeclaredDependencies: Playwright is a shared workspace-root development dependency.
import { chromium } from "playwright";

let server;
let browser;
let baseUrl;
const incomingScreenshots = /\/_astro\/(projects|logs)-(light|dark)[^/]*\.webp/;
const rotationControl = /pause|play slideshow/i;

before(async () => {
  server = await preview({
    logLevel: "silent",
    root: fileURLToPath(new URL("../", import.meta.url)),
    server: { host: "127.0.0.1", port: 0 },
  });
  baseUrl = `http://127.0.0.1:${server.port}`;
  browser = await chromium.launch({ headless: true });
});

after(async () => {
  await browser?.close();
  await server?.stop();
});

test("rapid tab changes retain one accessible panel and a stable preview", async () => {
  const page = await browser.newPage({ reducedMotion: "no-preference" });
  const errors = [];
  const pendingRequests = [];
  const gate = Promise.withResolvers();
  page.on("pageerror", (error) => errors.push(error.message));
  try {
    await page.route(incomingScreenshots, (route) => {
      const request = gate.promise.then(() => route.continue());
      pendingRequests.push(request);
      return request;
    });
    await page.goto(baseUrl);
    await page.waitForFunction(() =>
      document.querySelector("[data-reveal].reveal-pending")
    );
    await page.evaluate(() =>
      Promise.all(
        [
          ...document.querySelectorAll(
            ".hero-copy > h1, .hero-copy > p, .hero-actions, .product-preview"
          ),
        ]
          .flatMap((element) => element.getAnimations())
          .map((animation) => animation.finished)
      )
    );
    const stage = page.locator(".preview-stage");
    const height = await stage.evaluate(
      (element) => element.getBoundingClientRect().height
    );
    const requested = page.waitForRequest(incomingScreenshots);
    await page.getByRole("tab", { name: "Projects" }).click();
    await requested;
    assert.equal(await page.getByRole("tabpanel").count(), 1);
    assert.equal(
      await page
        .locator("#panel-requests")
        .evaluate((element) => element.inert),
      true
    );
    await page.getByRole("tab", { name: "Live logs" }).click();
    assert.ok(pendingRequests.length > 0);
    assert.equal(await page.locator("#panel-requests").isVisible(), true);
    assert.equal(
      await page
        .locator("#panel-requests")
        .evaluate((element) => getComputedStyle(element).opacity),
      "1"
    );
    assert.equal(await page.locator("#panel-projects").isVisible(), false);
    await page
      .getByRole("tab", { exact: true, name: "Request inspector" })
      .click();
    await page.getByRole("tab", { name: "Live logs" }).click();
    gate.resolve();
    await page.waitForFunction(
      () =>
        document.querySelectorAll(".preview-panel:not([hidden])").length === 1
    );
    assert.equal(
      await page
        .getByRole("tab", { name: "Live logs" })
        .getAttribute("aria-selected"),
      "true"
    );
    assert.ok(
      Math.abs(
        (await stage.evaluate(
          (element) => element.getBoundingClientRect().height
        )) - height
      ) < 1
    );
    await page.getByRole("tab", { name: "Live logs" }).press("Home");
    assert.equal(
      await page
        .getByRole("tab", { exact: true, name: "Request inspector" })
        .getAttribute("aria-selected"),
      "true"
    );
    await page.locator("#workflow").scrollIntoViewIfNeeded();
    await page.waitForFunction(() => {
      const section = document.querySelector("#workflow");
      return (
        !section.classList.contains("reveal-pending") &&
        getComputedStyle(section).opacity === "1"
      );
    });
    await page.locator("#hero-title").scrollIntoViewIfNeeded();
    assert.equal(await page.locator("#workflow.reveal-pending").count(), 0);
    assert.deepEqual(errors, []);
  } finally {
    gate.resolve();
    await Promise.allSettled(pendingRequests);
    await page.close();
  }
});

test("reduced motion is instant, including changes during a crossfade", async () => {
  const page = await browser.newPage({ reducedMotion: "reduce" });
  try {
    await page.goto(baseUrl);
    await page.getByRole("tab", { name: "Projects" }).click();
    assert.equal(await page.locator(".reveal-pending").count(), 0);
    assert.equal(await page.locator(".preview-panel:not([hidden])").count(), 1);
    assert.equal(await page.evaluate(() => document.getAnimations().length), 0);

    await page.emulateMedia({ reducedMotion: "no-preference" });
    await page.reload();
    await page.waitForFunction(() => document.querySelector(".reveal-pending"));
    await page.getByRole("tab", { name: "Live logs" }).click();
    await page.emulateMedia({ reducedMotion: "reduce" });
    await page.waitForFunction(
      () =>
        document.getAnimations().length === 0 &&
        document.querySelectorAll(".preview-panel:not([hidden])").length ===
          1 &&
        document.querySelectorAll(".reveal-pending").length === 0
    );
    assert.equal(
      await page.getByRole("tabpanel", { name: "Live logs" }).count(),
      1
    );
  } finally {
    await page.close();
  }
});

test("content remains visible without JavaScript on mobile", async () => {
  const page = await browser.newPage({
    javaScriptEnabled: false,
    reducedMotion: "reduce",
    viewport: { height: 844, width: 390 },
  });
  try {
    await page.goto(baseUrl);
    const sections = await page
      .locator("[data-reveal]")
      .evaluateAll((elements) =>
        elements.map((element) => getComputedStyle(element).opacity)
      );
    assert.ok(sections.length > 0);
    assert.ok(sections.every((opacity) => opacity === "1"));
    assert.equal(
      await page.getByRole("tabpanel", { name: "Request inspector" }).count(),
      1
    );
    const layout = await page.evaluate(() => ({
      overflow: [...document.querySelectorAll("body *")]
        .filter((element) => element.getBoundingClientRect().right > innerWidth)
        .map((element) => element.className),
      viewport: innerWidth,
      width: document.documentElement.scrollWidth,
    }));
    assert.ok(layout.width <= layout.viewport, JSON.stringify(layout));
    const question = page.locator(".question").first();
    await question.locator("summary").click();
    await question.locator("p").waitFor({ state: "visible" });
    assert.equal(await question.evaluate((element) => element.open), true);
    assert.equal(await question.locator("p").isVisible(), true);
    await page.locator(".question").nth(1).locator("summary").click();
    assert.equal(await question.evaluate((element) => element.open), false);
    assert.equal(await page.locator(".question[open]").count(), 1);
  } finally {
    await page.close();
  }
});

test("FAQ animates both directions and handles rapid keyboard toggles and reduced motion", {
  timeout: 10_000,
}, async () => {
  const page = await browser.newPage({
    reducedMotion: "no-preference",
  });
  const question = page.locator(".question").first();
  const summary = question.locator("summary");
  const traceToggle = () =>
    question.evaluate(async (element) => {
      const initialHeight = element.getBoundingClientRect().height;
      element.querySelector("summary").click();
      const heights = [];
      const deadline = performance.now() + 550;
      while (performance.now() < deadline) {
        // biome-ignore lint/performance/noAwaitInLoops: Sample successive browser animation frames, including native pseudo-elements.
        await new Promise(requestAnimationFrame);
        heights.push(element.getBoundingClientRect().height);
      }
      return { heights, initialHeight, open: element.open };
    });
  try {
    await page.goto(`${baseUrl}/#faq`);
    const opening = await traceToggle();
    const closedHeight = opening.initialHeight;
    const openHeight = opening.heights.at(-1);
    assert.equal(opening.open, true);
    assert.ok(openHeight > closedHeight + 1);
    assert.ok(
      opening.heights.some(
        (height) => height > closedHeight + 1 && height < openHeight - 1
      ),
      JSON.stringify(opening)
    );

    const closing = await traceToggle();
    assert.equal(closing.open, false);
    assert.ok(Math.abs(closing.heights.at(-1) - closedHeight) < 1);
    assert.ok(
      closing.heights.some(
        (height) => height > closedHeight + 1 && height < openHeight - 1
      ),
      JSON.stringify(closing)
    );

    await summary.focus();
    await summary.press("Space");
    await summary.press("Enter");
    await summary.press("Space");
    await question.locator("p").waitFor({ state: "visible" });
    assert.equal(await question.evaluate((element) => element.open), true);

    const nextQuestion = page.locator(".question").nth(1);
    await nextQuestion.locator("summary").press("Enter");
    assert.equal(await question.evaluate((element) => element.open), false);
    assert.equal(await nextQuestion.evaluate((element) => element.open), true);
    assert.equal(await page.locator(".question[open]").count(), 1);
    await summary.press("Space");
    assert.equal(await nextQuestion.evaluate((element) => element.open), false);
    assert.equal(await page.locator(".question[open]").count(), 1);

    await page.emulateMedia({ reducedMotion: "reduce" });
    await page.waitForFunction(
      () =>
        document.querySelector(".question").getAnimations({ subtree: true })
          .length === 0
    );
    assert.ok(
      Math.abs(
        (await question.evaluate(
          (element) => element.getBoundingClientRect().height
        )) - openHeight
      ) < 1
    );
    await summary.press("Enter");
    await question.evaluate(() => new Promise(requestAnimationFrame));
    assert.equal(await question.evaluate((element) => element.open), false);
    const reducedState = await question.evaluate((element) => ({
      answerHeight: element
        .querySelector(".question-answer")
        .getBoundingClientRect().height,
      display: getComputedStyle(element, "::details-content").display,
      height: element.getBoundingClientRect().height,
      summaryHeight: element.querySelector("summary").getBoundingClientRect()
        .height,
      transitions: getComputedStyle(element, "::details-content").transition,
      visibility: getComputedStyle(element, "::details-content")
        .contentVisibility,
    }));
    assert.ok(
      Math.abs(reducedState.height - closedHeight) < 1,
      JSON.stringify({ closedHeight, openHeight, reducedState })
    );
    assert.equal(
      await question.evaluate(
        (element) => element.getAnimations({ subtree: true }).length
      ),
      0
    );
  } finally {
    await page.close();
  }
});

test("slideshow advances after six seconds, wraps, and keeps focus in place", async () => {
  const page = await browser.newPage({ reducedMotion: "no-preference" });
  const advanceTo = async (id) => {
    await page.clock.fastForward(6000);
    assert.equal(
      await page.locator(`#tab-${id}`).getAttribute("aria-selected"),
      "true"
    );
  };
  try {
    await page.clock.install();
    await page.goto(baseUrl);
    assert.equal(
      await page
        .locator("#tab-requests .preview-progress")
        .evaluate(
          (element) => element.getAnimations()[0].effect.getTiming().duration
        ),
      6000
    );
    await advanceTo("projects");
    await advanceTo("logs");
    await advanceTo("requests");
    assert.equal(
      await page.evaluate(() => document.activeElement.tagName),
      "BODY"
    );
    assert.equal(await page.getByRole("tabpanel").count(), 1);
  } finally {
    await page.close();
  }
});

test("slideshow never pauses and manual selection restarts its countdown", async () => {
  const page = await browser.newPage({
    reducedMotion: "no-preference",
    viewport: { height: 844, width: 390 },
  });
  const selectedTab = () =>
    page.locator('.preview-tab[aria-selected="true"]').getAttribute("id");
  try {
    await page.clock.install();
    await page.goto(baseUrl);
    await page.locator(".preview-stage").hover();
    await page
      .getByRole("tab", { exact: true, name: "Request inspector" })
      .focus();
    await page.clock.fastForward(6000);
    assert.equal(await selectedTab(), "tab-projects");
    assert.equal(
      await page.evaluate(() => document.activeElement.id),
      "tab-requests"
    );

    await page
      .getByRole("tab", { exact: true, name: "Request inspector" })
      .press("ArrowRight");
    await page.clock.fastForward(4000);
    await page.getByRole("tab", { name: "Projects" }).click();
    await page.clock.fastForward(3000);
    assert.equal(await selectedTab(), "tab-projects");
    await page.clock.fastForward(3000);
    assert.equal(await selectedTab(), "tab-logs");

    await page.locator("#download").scrollIntoViewIfNeeded();
    await page.clock.fastForward(6000);
    assert.equal(await selectedTab(), "tab-requests");
    await page.evaluate(() => {
      Object.defineProperty(document, "hidden", {
        configurable: true,
        value: true,
      });
      document.dispatchEvent(new Event("visibilitychange"));
    });
    await page.clock.fastForward(6000);
    assert.equal(await selectedTab(), "tab-projects");
    await page.evaluate(() => {
      Reflect.deleteProperty(document, "hidden");
      document.dispatchEvent(new Event("visibilitychange"));
    });

    await page.emulateMedia({ reducedMotion: "reduce" });
    await page.waitForFunction(() =>
      [...document.querySelectorAll(".preview-progress")].every(
        (element) => element.getAnimations().length === 0
      )
    );
    await page.clock.fastForward(6000);
    assert.equal(await selectedTab(), "tab-logs");
    assert.equal(
      await page.getByRole("button", { name: rotationControl }).count(),
      0
    );
    await page.setViewportSize({ height: 844, width: 320 });
    const layout = await page.evaluate(() => ({
      overflow: [...document.querySelectorAll("body *")]
        .filter((element) => element.getBoundingClientRect().right > innerWidth)
        .map((element) => element.className),
      viewport: innerWidth,
      width: document.documentElement.scrollWidth,
    }));
    assert.ok(layout.width <= layout.viewport, JSON.stringify(layout));
  } finally {
    await page.close();
  }
});
