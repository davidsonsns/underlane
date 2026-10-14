#!/usr/bin/env node
// Drives the real app (debug build with the e2e control server) through the manual
// verification scenarios and captures window-only screenshots into docs/screenshots/.
//
// usage: node scripts/screenshots.mjs target/debug/bundle/macos/Underlane.app
import { execFileSync, spawn } from "node:child_process";
import { mkdirSync, mkdtempSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join, resolve } from "node:path";
import {
  captureWindow,
  connectedStates,
  createControl,
  createNotes,
  finishShots,
  hit,
  seedQuickTunnels,
  sleep,
  waitFor,
} from "./lib/harness.mjs";

import {
  captureMenuStrip,
  revealMenuItem,
  toggleHiddenBar,
} from "./lib/native.mjs";

const app = resolve(
  process.argv[2] ?? "target/debug/bundle/macos/Underlane.app"
);
const bin = join(app, "Contents/MacOS/Underlane");
const out = resolve("docs/screenshots");
mkdirSync(out, { recursive: true });
const CONTROL = 48_100 + Math.floor(Math.random() * 500);
const dataDir = mkdtempSync(join(tmpdir(), "underlane-shots-"));
const { notes, note } = createNotes();
const { ctl } = createControl(CONTROL);
const capture = (name, minW = 700, maxW = 4000) =>
  captureWindow(join(out, name), minW, maxW);
// The menu bar may be managed by an app like Hidden Bar that collapses new items. Reveal
// Underlane's item for the capture, then put the menu bar back the way it was.
async function captureMenuBar(name) {
  const { item, expanded } = await revealMenuItem({ bestEffort: true });
  try {
    if (item && item.x >= 0) {
      captureMenuStrip(item, join(out, name));
      note(`menu bar strip captured around x=${item.x} -> ${name}`);
    } else {
      note(`could not reveal Underlane's menu bar item for ${name}`);
    }
  } finally {
    if (expanded) {
      toggleHiddenBar(true);
    }
  }
}

function launch() {
  const child = spawn(bin, [], {
    env: {
      ...process.env,
      UNDERLANE_DATA_DIR: dataDir,
      UNDERLANE_E2E_PORT: String(CONTROL),
    },
    stdio: "ignore",
  });
  return child;
}

let child = launch();
const servers = [];
try {
  await waitFor(
    "control server",
    async () => (await ctl("GET", "/health")).ok,
    30_000,
    300
  );
  await ctl("POST", "/settings?theme=dark");

  // 1. Import an existing config.yml from ~/.cloudflared (read-only; not started).
  await ctl("POST", "/navigate?view=add");
  await sleep(1500);
  capture("01-import-sheet.png");
  const cfg = join(homedir(), ".cloudflared/example.yml");
  const imported = await ctl("POST", `/import?path=${encodeURIComponent(cfg)}`);
  note(`import ${cfg}: ${JSON.stringify(imported)}`);
  const ov = await ctl("GET", "/overview");
  const example = ov.tunnels.find((t) => t.config_path === cfg);
  await ctl("POST", `/navigate?view=${encodeURIComponent(`tunnels:${example.id}`)}`);
  await sleep(1500);
  capture("02-imported-config.png");

  // 2. Run several tunnels at once (quick tunnels to local servers; one inspected).
  const specs = [
    ["web", true],
    ["api", false],
    ["docs", false],
  ];
  const ids = await seedQuickTunnels(ctl, servers, specs, {
    start: true,
    timestamp: true,
  });
  const states = await connectedStates(ctl, ids);
  note(
    `running: ${states.map((s) => s.public_url).join(", ")}; menu bar title = "${states[0].menu_bar_title}"`
  );

  // Traffic through the inspected tunnel.
  await hit(`${states[0].public_url}/api/orders?page=1`, "POST", {
    gift: true,
    item: "notebook",
    qty: 2,
  });
  await hit(`${states[0].public_url}/api/orders/42`, "GET");
  await hit(`${states[0].public_url}/health`, "GET");
  await sleep(1500);

  // Menu bar crop around the tray item (shows the count).
  await captureMenuBar("03-menu-bar-count.png");
  await ctl("POST", "/popover");
  await sleep(1200);
  capture("04-popover-three-tunnels.png", 300, 420);
  await ctl("POST", "/popover");

  await ctl(
    "POST",
    `/navigate?view=${encodeURIComponent(`tunnels:${ids[0]}`)}`
  );
  await sleep(1500);
  capture("05-dashboard-dark.png");

  await ctl(
    "GET",
    `/ui?view=${encodeURIComponent(`requests:${ids[0]}`)}&select=first`
  );
  await sleep(800);
  capture("06-requests-dark.png");
  await ctl("POST", `/navigate?view=${encodeURIComponent(`logs:${ids[0]}`)}`);
  await sleep(1500);
  capture("07-logs-dark.png");

  // 3. Theme switching.
  await ctl("POST", "/navigate?view=settings");
  await sleep(1200);
  capture("08-settings-dark.png");
  await ctl("POST", "/settings?theme=light");
  await sleep(1200);
  capture("09-settings-light.png");
  await ctl(
    "POST",
    `/navigate?view=${encodeURIComponent(`tunnels:${ids[0]}`)}`
  );
  await sleep(1500);
  capture("10-dashboard-light.png");
  await ctl(
    "GET",
    `/ui?view=${encodeURIComponent(`requests:${ids[0]}`)}&select=first`
  );
  await sleep(800);
  capture("11-requests-light.png");
  await ctl("POST", "/popover");
  await sleep(1200);
  capture("12-popover-light.png", 300, 420);
  await ctl("POST", "/popover");

  // 4. Auto-start: mark two tunnels, quit, relaunch without touching anything.
  await ctl("POST", `/tunnel?id=${ids[0]}&autostart=1`);
  await ctl("POST", `/tunnel?id=${ids[1]}&autostart=1`);
  await ctl("POST", "/quit");
  await sleep(4000);
  const leftover = execFileSync("ps", ["-axww", "-o", "command="])
    .toString()
    .split("\n")
    .filter((l) => l.includes(dataDir) && l.includes("cloudflared"));
  note(`after quit: ${leftover.length} cloudflared processes left`);
  child = launch();
  await waitFor(
    "control server (relaunch)",
    async () => (await ctl("GET", "/health")).ok,
    30_000,
    300
  );
  const auto = await waitFor(
    "autostart tunnels connected",
    async () => {
      const st = await Promise.all(
        ids.map((id) => ctl("GET", `/state?id=${id}`))
      );
      return st[0].status === "connected" && st[1].status === "connected"
        ? st
        : null;
    },
    120_000
  );
  note(
    `after relaunch: ${auto.map((s, i) => `${specs[i][0]}=${s.status}`).join(", ")}; menu bar title = "${auto[0].menu_bar_title}"`
  );
  await captureMenuBar("14-menu-bar-after-autostart.png");
  await ctl("POST", "/popover");
  await sleep(1200);
  capture("13-autostart-after-relaunch.png", 300, 420);
  await ctl("POST", "/popover");
} finally {
  await finishShots(child, ctl, servers, join(out, "session-notes.txt"), notes);
}
