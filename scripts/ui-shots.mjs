#!/usr/bin/env node
// Screenshots of every screen in both themes, for comparing UI changes phase by phase.
// Drives the real app (debug build with the e2e control server) on a throwaway data dir,
// so it works on any machine and never touches the user's settings.
//
// usage: node scripts/ui-shots.mjs target/debug/bundle/macos/Underlane.app --label <name> [--offline]
//   writes e2e/out/ui/<label>/{light,dark}-{tunnels,requests,logs,settings,add,popover}.png,
//   plus system-{tunnels,popover}.png (theme "system": whatever the OS appearance is) and notes.txt.
//   --offline skips starting tunnels (stopped states only, no requests) for machines without network.
// It does not change the macOS appearance.
import { execFileSync, spawn } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import {
  captureWithRetry,
  connectedStates,
  createControl,
  createNotes,
  finishShots,
  hit,
  seedQuickTunnels,
  sleep,
  waitFor,
} from "./lib/harness.mjs";

const args = process.argv.slice(2);
const flag = (name) => args.includes(name);
const opt = (name, dflt) => {
  const i = args.indexOf(name);
  return i >= 0 && args[i + 1] ? args[i + 1] : dflt;
};
const app = resolve(
  args.find((a, i) => !a.startsWith("--") && args[i - 1] !== "--label") ??
    "target/debug/bundle/macos/Underlane.app"
);
const label = opt("--label", "latest");
// The label names one folder under e2e/out/ui, which is deleted and rewritten below.
if (!/^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(label) || label.includes("..")) {
  console.error(`bad --label "${label}": use letters, digits, ".", "_" or "-"`);
  process.exit(2);
}
const offline = flag("--offline");
const bin = join(app, "Contents/MacOS/Underlane");
const out = resolve("e2e/out/ui", label);
rmSync(out, { force: true, recursive: true });
mkdirSync(out, { recursive: true });
const CONTROL = 48_600 + Math.floor(Math.random() * 400);
const dataDir = mkdtempSync(join(tmpdir(), "underlane-ui-shots-"));
const { notes, note } = createNotes();
const { ctl } = createControl(CONTROL);
// Use the child PID so the owner's running copy is never captured.
const capture = (name, minW = 700, maxW = 4000) =>
  captureWithRetry(join(out, name), minW, maxW, child.pid);
function osAppearance() {
  try {
    return (
      execFileSync("defaults", ["read", "-g", "AppleInterfaceStyle"], {
        stdio: ["ignore", "pipe", "ignore"],
      })
        .toString()
        .trim() || "Light"
    );
  } catch {
    return "Light"; // the key is absent in light mode
  }
}
const nav = (view) => ctl("POST", `/navigate?view=${encodeURIComponent(view)}`);

const child = spawn(bin, [], {
  env: {
    ...process.env,
    UNDERLANE_DATA_DIR: dataDir,
    UNDERLANE_E2E_PORT: String(CONTROL),
  },
  stdio: "ignore",
});
const servers = [];
let failed = false;
try {
  await waitFor(
    "control server",
    async () => (await ctl("GET", "/health")).ok,
    30_000,
    300
  );
  note(`label=${label} offline=${offline} app=${app}`);
  note(`OS appearance: ${osAppearance()} (theme "system" shots follow it)`);

  // Three quick tunnels to local echo servers: "web" is inspected, "web" and "api" run,
  // "db" stays stopped so the stopped states (dot, switch, detail) are in every shot.
  const ids = await seedQuickTunnels(
    ctl,
    servers,
    [
      ["web", true],
      ["api", false],
      ["db", false],
    ],
    { start: false, timestamp: false }
  );
  if (!offline) {
    const running = ids.slice(0, 2);
    for (const id of running) {
      // biome-ignore lint/performance/noAwaitInLoops: Polls and native UI actions must complete in order before the next step.
      await ctl("POST", `/start?id=${id}`);
    }
    const states = await connectedStates(ctl, running);
    note(`running: ${states.map((s) => s.public_url).join(", ")}`);
    const base = states[0].public_url;
    await hit(`${base}/api/orders?page=1`, "POST", {
      gift: true,
      item: "notebook",
      qty: 2,
    });
    await hit(`${base}/api/orders/42`, "GET");
    await hit(`${base}/health`, "GET");
    await sleep(1500);
  }

  const shoot = async (theme, views) => {
    await ctl("POST", `/settings?theme=${theme}`);
    await sleep(800);
    for (const v of views) {
      if (v === "requests") {
        // biome-ignore lint/performance/noAwaitInLoops: Polls and native UI actions must complete in order before the next step.
        await ctl(
          "GET",
          `/ui?view=${encodeURIComponent(`requests:${ids[0]}`)}&select=first`
        );
        await sleep(800);
      } else if (v === "popover") {
        await ctl("POST", "/popover");
        await sleep(1200);
        await capture(`${theme}-popover.png`, 300, 420);
        await ctl("POST", "/popover");
        await sleep(600);
        continue;
      } else {
        await nav(
          {
            add: "add",
            logs: `logs:${ids[0]}`,
            settings: "settings",
            tunnels: `tunnels:${ids[0]}`,
          }[v]
        );
        await sleep(1500);
      }
      await capture(`${theme}-${v}.png`);
    }
    // Leave the add sheet closed for the next pass.
    await nav(`tunnels:${ids[0]}`);
    await sleep(500);
  };
  const all = ["tunnels", "requests", "logs", "settings", "add", "popover"];
  await shoot("light", all);
  await shoot("dark", all);
  await shoot("system", ["tunnels", "popover"]);
  const expected = [
    ...["light", "dark"].flatMap((t) => all.map((v) => `${t}-${v}.png`)),
    "system-tunnels.png",
    "system-popover.png",
  ];
  const missing = expected.filter((f) => !existsSync(join(out, f)));
  if (missing.length) {
    throw new Error(`missing screenshots: ${missing.join(", ")}`);
  }
  note(`wrote ${out}`);
} catch (e) {
  failed = true;
  note(`FAILED: ${e.message}`);
} finally {
  await finishShots(child, ctl, servers, join(out, "notes.txt"), notes);
  rmSync(dataDir, { force: true, recursive: true });
  process.exit(failed ? 1 : 0);
}
