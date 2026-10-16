// Run against Vite on port 1420; DNS results and Tauri calls are synthetic.
import assert from "node:assert/strict";
import { mkdir } from "node:fs/promises";
import {
  browserPage,
  installHostnameMock,
  installPageMock,
} from "./lib/browser.mjs";

function installDnsResponses() {
  const messages = [
    "This hostname has an existing DNS record that does not proxy to this tunnel. It was preserved; review the record in Cloudflare before changing it.",
    "DNS points to this tunnel, but Cloudflare proxying is disabled. Enable proxying for this record in Cloudflare.",
    "No DNS record exists for this hostname. Create one to route it to this tunnel.",
    "Could not verify DNS. Check your connection and retry.",
  ];
  window.__dnsResults = Object.fromEntries(
    window.__dnsMock.config.routes.map((route, index) => [
      route.hostname,
      {
        can_create: index === 2,
        message: messages[index],
        needs_dns_access: index === 3,
        status: ["conflict", "conflict", "missing", "unknown"][index],
      },
    ])
  );
  const dispatch = window.__mockDispatch;
  window.__mockDispatch = (cmd, args) => {
    if (cmd === "check_hostname_dns") {
      window.__dnsMock.calls.push({ args, cmd });
      return structuredClone(window.__dnsResults[args.hostname]);
    }
    return dispatch(cmd, args);
  };
}

const { baseUrl, browser, page, errors } = await browserPage({
  webkit: process.env.BROWSER === "webkit",
});
try {
  await page.setViewportSize({ height: 708, width: 1092 });
  await installPageMock(
    {
      addInitScript: ({ content }) =>
        page.addInitScript({
          content: `${content}; (${installDnsResponses})();`,
        }),
    },
    installHostnameMock,
    { delay: 0, mockName: "__dnsMock" }
  );
  await page.goto(`${baseUrl}/#tunnels:synthetic`);
  const results = await page.evaluate(() => window.__dnsResults);
  const hosts = Object.keys(results);
  const tooltip = page.locator(
    "[data-slot=tooltip-content]:not([data-state=closed])"
  );
  const labels = {
    conflict: "DNS needs attention",
    missing: "DNS missing",
    unknown: "DNS target unverified",
  };
  async function checkTooltip(hostname, result) {
    const badge = page.getByRole("button", {
      exact: true,
      name: `${labels[result.status]} for ${hostname}`,
    });
    await badge.hover();
    await tooltip.waitFor();
    assert.ok((await tooltip.innerText()).includes(hostname));
    assert.ok((await tooltip.innerText()).includes(result.message));
    assert.ok((await tooltip.innerText()).includes("Check DNS"));
    await page.keyboard.press("Escape");
    await tooltip.waitFor({ state: "hidden" });
  }
  for (const [hostname, result] of Object.entries(results)) {
    // biome-ignore lint/performance/noAwaitInLoops: Tooltips share one page and must open and close in order.
    await checkTooltip(hostname, result);
  }

  const badge = page.getByRole("button", {
    exact: true,
    name: `DNS needs attention for ${hosts[0]}`,
  });
  await badge.focus();
  await page.keyboard.press("Tab");
  await page.keyboard.press("Shift+Tab");
  await tooltip.waitFor();
  assert.ok((await tooltip.innerText()).includes("It was preserved"));
  assert.equal(
    await badge.getAttribute("aria-describedby"),
    await tooltip.getAttribute("id")
  );
  await page.waitForFunction(
    () =>
      !document.querySelector("[data-slot=tooltip-content][data-state=closed]")
  );
  const bounds = await tooltip.boundingBox();
  assert.ok(bounds.x >= 0 && bounds.y >= 0);
  assert.ok(bounds.x + bounds.width <= 1092 && bounds.y + bounds.height <= 708);
  await mkdir("e2e/out", { recursive: true });
  await page.screenshot({
    animations: "disabled",
    path: "e2e/out/dns-status-dark.png",
  });
  await page.evaluate(() => {
    document.documentElement.dataset.theme = "light";
  });
  await page.screenshot({
    animations: "disabled",
    path: "e2e/out/dns-status-light.png",
  });
  await page.keyboard.press("Escape");
  await tooltip.waitFor({ state: "hidden" });

  await page.evaluate((hostname) => {
    window.__dnsResults[hostname] = {
      can_create: false,
      message: "Cloudflare confirmed this hostname is routed to this tunnel.",
      needs_dns_access: false,
      status: "connected",
    };
  }, hosts[0]);
  await page
    .getByRole("button", { name: `DNS actions for ${hosts[0]}` })
    .click();
  await page.getByRole("menuitem", { exact: true, name: "Check DNS" }).click();
  await page
    .getByRole("button", { name: `DNS verified for ${hosts[0]}` })
    .hover();
  await tooltip.waitFor();
  assert.ok((await tooltip.innerText()).includes("Cloudflare confirmed"));
  assert.ok(!(await tooltip.innerText()).includes("It was preserved"));
  const calls = await page.evaluate(() => window.__dnsMock.calls);
  assert.equal(
    calls.filter((call) => call.cmd === "check_hostname_dns").length,
    5
  );
  assert.equal(
    calls.filter((call) =>
      ["create_hostname_dns", "connect_hostname_dns"].includes(call.cmd)
    ).length,
    0
  );
  assert.deepEqual(errors, []);
  console.log(
    "DNS tooltip passed: diagnostic details, keyboard focus, Escape, refresh, and no DNS mutations"
  );
} finally {
  await browser.close();
}
