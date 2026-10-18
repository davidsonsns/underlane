import assert from "node:assert/strict";
import { after, before, test } from "node:test";
import { fileURLToPath } from "node:url";
import { gunzipSync } from "node:zlib";
import { preview } from "astro";
// biome-ignore lint/correctness/noUndeclaredDependencies: Playwright is a shared workspace-root development dependency.
import { chromium } from "playwright";

let server;
let browser;
let baseUrl;
const isPostHog = (request) =>
  new URL(request.url()).hostname.endsWith(".posthog.com");
const decode = (request) => {
  if (
    !(isPostHog(request) && new URL(request.url()).pathname.startsWith("/e/"))
  ) {
    return [];
  }
  let body = request.postDataBuffer();
  if (!body) {
    return [];
  }
  if (body[0] === 0x1f && body[1] === 0x8b) {
    body = gunzipSync(body);
  }
  const encoded = new URLSearchParams(body.toString()).get("data");
  const payload = JSON.parse(
    encoded ? Buffer.from(encoded, "base64").toString() : body.toString()
  );
  return payload.batch ?? (Array.isArray(payload) ? payload : [payload]);
};

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

async function visit(url) {
  const context = await browser.newContext({
    reducedMotion: "reduce",
    userAgent:
      "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/134.0.0.0 Safari/537.36",
  });
  context.setDefaultTimeout(5000);
  // Exercise visitor analytics rather than the SDK's automated-browser filter.
  await context.addInitScript(() => {
    Object.defineProperty(navigator, "webdriver", { get: () => false });
    Object.defineProperty(navigator, "userAgentData", { get: () => undefined });
  });
  const events = [];
  const requests = [];
  await context.route("**/*", async (route) => {
    const request = route.request();
    const target = new URL(request.url());
    if (isPostHog(request)) {
      requests.push(request.url());
      events.push(...decode(request));
      return route.fulfill({ body: "{}", contentType: "application/json" });
    }
    if (target.hostname === "github.com") {
      return route.fulfill({
        body: "<h1>Setup guide</h1>",
        contentType: "text/html",
      });
    }
    if (target.origin === new URL(url).origin) {
      return route.fulfill({
        response: await route.fetch({
          url: `${baseUrl}${target.pathname}${target.search}`,
        }),
      });
    }
    return route.abort();
  });
  const captured = (name) =>
    context
      .waitForEvent("request", {
        predicate: (request) =>
          decode(request).some((event) => event.event === name),
      })
      .then((request) => decode(request).find((event) => event.event === name));
  const page = await context.newPage();
  return { captured, context, events, page, requests, url };
}

test("production captures campaign pageviews and CTA intents through outbound navigation without persistence", async () => {
  const { captured, context, events, page, url } = await visit(
    "https://underlane.app/?utm_source=browser-test&utm_campaign=launch&utm_medium=email"
  );
  try {
    const pageview = captured("$pageview");
    await page.goto(url);
    const { properties } = await pageview;
    assert.equal(properties.utm_source, "browser-test");
    assert.equal(properties.utm_campaign, "launch");
    assert.equal(properties.utm_medium, "email");
    const hero = captured("get_underlane_clicked");
    await page
      .locator(".hero-actions")
      .getByRole("link", { name: "Get Underlane" })
      .click();
    assert.equal((await hero).properties.location, "hero");
    assert.equal(
      events.filter((event) => event.event === "get_underlane_clicked").length,
      1
    );
    assert.equal(
      events.filter((event) => event.event === "$pageview").length,
      1
    );
    assert.deepEqual(await context.cookies(), []);
    assert.deepEqual(
      await page.evaluate(() => [
        Object.keys(localStorage),
        Object.keys(sessionStorage),
      ]),
      [[], []]
    );
    const guide = captured("install_guide_opened");
    await page.getByRole("link", { name: "Build Underlane for macOS" }).click();
    await page.waitForURL("https://github.com/davidsonsns/underlane#setup");
    assert.equal((await guide).properties.location, "download");
  } finally {
    await context.close();
  }
});

test("www production counts a copied command only after the clipboard accepts it", async () => {
  const { captured, context, events, page, url } = await visit(
    "https://www.underlane.app/"
  );
  try {
    const pageview = captured("$pageview");
    await page.goto(url);
    await pageview;
    await page.clock.install();
    await page.locator(".source-install summary").click();
    await page.evaluate(() => {
      navigator.clipboard.writeText = () =>
        Promise.reject(new Error("Clipboard denied"));
    });
    await page
      .getByRole("button", { name: "Copy installation commands" })
      .click();
    await page.clock.fastForward(5000);
    assert.equal(
      events.filter((event) => event.event === "install_command_copied").length,
      0
    );
    await page.evaluate(() => {
      navigator.clipboard.writeText = (text) => {
        window.copiedCommand = text;
        return Promise.resolve();
      };
    });
    const copied = captured("install_command_copied");
    await page
      .getByRole("button", { name: "Copy installation commands" })
      .click();
    await page.clock.fastForward(5000);
    assert.equal((await copied).properties.location, "download");
    assert.equal(
      await page.evaluate(() => window.copiedCommand),
      await page.locator("#install-command").textContent()
    );
    assert.equal(
      events.filter((event) => event.event === "install_command_copied").length,
      1
    );
  } finally {
    await context.close();
  }
});

for (const origin of [
  "http://localhost",
  "https://preview.underlane.app",
  "https://underlane.app.example.com",
]) {
  test(`${origin} sends no analytics`, async () => {
    const { context, page, requests, url } = await visit(`${origin}/`);
    try {
      await page.clock.install();
      await page.goto(url);
      await page
        .locator(".hero-actions")
        .getByRole("link", { name: "Get Underlane" })
        .click();
      await page.clock.fastForward(10_000);
      assert.deepEqual(requests, []);
    } finally {
      await context.close();
    }
  });
}
