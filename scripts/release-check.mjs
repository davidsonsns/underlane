#!/usr/bin/env node
// Manual-verification pass against the INSTALLED RELEASE build (no control server):
// login-item registration, a login-style launch through LaunchServices, auto-start of a
// tunnel, a public curl through it, then popover + dashboard + theme switching driven by real
// clicks. Screenshots go to docs/screenshots/18-*.. and the system is restored afterwards.
//
// usage: node scripts/release-check.mjs
import { execFileSync, execFile, spawn } from "node:child_process";
import { promisify } from "node:util";
import { homedir } from "node:os";
import { join } from "node:path";
import { Resolver } from "node:dns/promises";

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const run = promisify(execFile);
const APP = join(homedir(), "Applications/Burrow.app");
const BIN = join(APP, "Contents/MacOS/Burrow");
const DB = join(homedir(), "Library/Application Support/com.davidsonsns.burrow/burrow.db");
const OUT = "docs/screenshots";
const log = (m) => console.log(`[release-check] ${m}`);
const sh = (cmd, args) => execFileSync(cmd, args).toString().trim();
// .timeout: wait up to 5s for Burrow's write lock instead of failing with "database is locked".
const sql = (q) => sh("sqlite3", ["-cmd", ".timeout 5000", DB, q]);
const extras = (app) => sh("swift", ["scripts/menubar-extras.swift", app]).split("\n")
  .map((l) => l.match(/^(\d+) x=(-?\d+) w=(\d+)/)).filter(Boolean).map((m) => ({ x: +m[2], w: +m[3] }));
const click = (x, y) => execFileSync("swift", ["scripts/click.swift", String(Math.round(x)), String(Math.round(y))]);
const windows = () => sh("swift", ["scripts/window-bounds.swift", "Burrow"]).split("\n").filter(Boolean).map((l) => JSON.parse(l));
const capture = (name, min = 300, max = 4000) => execFileSync("swift", ["scripts/capture-window.swift", "Burrow", join(OUT, name), String(min), String(max)], { stdio: "inherit" });
const theme = () => sql("SELECT json_extract(value,'$.theme') FROM settings WHERE key='settings'");

// Click targets in the dashboard, in points relative to the window's top-left corner. The
// window is a fixed 1080x700 with an overlay title bar, so the webview fills it and these are
// also CSS pixels. They assume cloudflared is found (no "Install it with Homebrew" row above
// Appearance) and a "This launch" row (always there after a launch).
// - Sidebar (re-measured in Phase 2h on the e2e debug build, from the selected item's highlight
//   in real-window captures): 28px nav items every 29px from y=78 (Tunnels 78-105, Settings
//   165-192; centres Tunnels 92, Requests 121, Logs 150, Settings 179), x 10-189, so x=60 is
//   inside. Unchanged from Phase 2a: the shell kept its 46px spacer, brand and item metrics.
// - Appearance (Settings, General group, 4th row): segment centres at y=368; System 864,
//   Light 922, Dark 973. Measured in Phase 2a; Settings didn't move in 2h.
//   The control is right-aligned in the 760px column centred in the 880px content pane. The
//   pane's scroll bar can move it left by about its width; that stays inside each 48-65px
//   segment (half-widths of 24px or more).
// Validated in 2h by clicking each point with click.swift in the e2e debug build: Settings then
// Dark, Light, System, Light (theme read back from the control server each time), then Tunnels
// (capture shows the Tunnels view).
// - Popover "Open Dashboard" (pop.x + 80, pop.y + pop.h - 20), unchanged in Phase 2i: the 360x500
//   footer has 8px side and bottom padding and 22px ghost buttons, so the button spans x 8 to ~150 and
//   y 470-492 (h-30 to h-8; the label ends near x 140). Validated in 2i by clicking that
//   point in the e2e debug build: the dashboard opened and the popover hid.
const AT = { tunnels: [60, 92], settings: [60, 179], system: [864, 368], light: [922, 368], dark: [973, 368] };

if (execFileSync("sh", ["-c", "pgrep -f 'MacOS/Burrow$' || true"]).toString().trim()) {
  throw new Error("quit Burrow first");
}

// A local origin for the auto-start tunnel.
const origin = spawn("python3", ["-m", "http.server", "18777", "--bind", "127.0.0.1"], { stdio: "ignore", detached: true });
const TID = "release-check-tunnel";
sql(`INSERT OR IGNORE INTO projects(id,name,sort) VALUES ('release-check','Release check',99);
     INSERT OR REPLACE INTO tunnels(id,project_id,name,kind,config_path,origin_url,inspect,autostart,loglevel,created_at,sort)
     VALUES ('${TID}','release-check','autostart-demo','quick',NULL,'http://127.0.0.1:18777',1,1,'info',${Date.now()},99);`);
log(`seeded tunnel: ${sql(`SELECT name, kind, inspect, autostart FROM tunnels WHERE id='${TID}'`)}`);

let expanded = false;
const themeBefore = theme();
const loginBefore = JSON.parse(sh(BIN, ["--login-item", "status"]));
try {
  log(`login item before: ${JSON.stringify(loginBefore)}`);
  log(`login item enable: ${sh(BIN, ["--login-item", "enable"])}`);

  // Login-style launch: LaunchServices, no arguments, no terminal.
  execFileSync("open", ["-g", APP]);
  let url = "";
  for (let i = 0; i < 90 && !url; i++) {
    await sleep(1000);
    const m = sql(`SELECT message FROM logs WHERE tunnel_id='${TID}' AND message LIKE '%trycloudflare.com%' ORDER BY id DESC LIMIT 1`).match(/https:\/\/[a-z0-9-]+\.trycloudflare\.com/);
    if (m) url = m[0];
  }
  if (!url) throw new Error("auto-start tunnel never got a public URL");
  const app = sh("sh", ["-c", "pgrep -f 'Applications/Burrow.app/Contents/MacOS/Burrow$'"]);
  log(`app pid ${app}, parent: ${sh("sh", ["-c", `ps -o comm= -p $(ps -o ppid= -p ${app})`])}, tty: ${sh("ps", ["-o", "tty=", "-p", app])}`);
  log("process tree:\n" + sh("sh", ["-c", `ps -axo pid,ppid,tty,command | grep -E '[B]urrow.app/Contents/MacOS/Burrow|[c]loudflared tunnel --config ${DB.replace("/burrow.db", "").replace(/ /g, ".")}' | cut -c1-170`]));
  log(`auto-started tunnel public URL: ${url}`);

  const host = new URL(url).hostname;
  const r = new Resolver();
  r.setServers(["1.1.1.1"]);
  let ip;
  for (let i = 0; i < 60 && !ip; i++) {
    try { ip = (await r.resolve4(host))[0]; } catch { await sleep(2000); }
  }
  let code = "";
  for (let i = 0; i < 20 && code !== "200"; i++) {
    try { code = (await run("curl", ["-s", "-o", "/dev/null", "-w", "%{http_code}", "--max-time", "15", "--resolve", `${host}:443:${ip}`, `https://${host}/release-check?token=abc`])).stdout; } catch { code = "000"; }
    if (code !== "200") await sleep(3000);
  }
  log(`curl https://${host}/release-check -> ${code}`);
  await sleep(1500);
  log(`captured in SQLite by the release app: ${sql(`SELECT method, path, status FROM requests WHERE tunnel_id='${TID}' ORDER BY ts DESC LIMIT 1`)}`);

  // Menu bar (Hidden Bar may have collapsed the item) and the popover, by real clicks.
  let item = extras("Burrow")[0];
  if (item.x < 0) { const c = extras("Hidden Bar")[0]; click(c.x + c.w / 2, 12); expanded = true; await sleep(1200); item = extras("Burrow")[0]; }
  execFileSync("screencapture", ["-x", "-R", `${item.x - 110},0,${item.w + 220},30`, join(OUT, "18-release-menu-bar.png")]);
  click(item.x + item.w / 2, 12);
  await sleep(1500);
  capture("19-release-popover.png", 300, 420);
  const pop = windows().find((w) => w.w <= 420);
  click(pop.x + 80, pop.y + pop.h - 20); // "Open Dashboard"
  await sleep(2500);
  const dash = windows().find((w) => w.w > 700);
  capture("20-release-dashboard.png", 700);
  const tap = (k) => click(dash.x + AT[k][0], dash.y + AT[k][1]);
  // Fail on a miss: a stale coordinate would otherwise "pass" and leave the real settings on Dark.
  const pickTheme = async (want) => {
    tap(want);
    await sleep(1200);
    const got = theme();
    log(`theme after clicking ${want[0].toUpperCase() + want.slice(1)}: ${got}`);
    if (got !== want) throw new Error(`theme after clicking ${want} is "${got}", expected "${want}": the Appearance coordinates (AT) in release-check.mjs are probably stale`);
  };
  tap("settings");
  await sleep(1200);
  await pickTheme("light");
  capture("21-release-settings-light.png", 700);
  tap("tunnels");
  await sleep(1200);
  capture("22-release-dashboard-light.png", 700);
  tap("settings");
  await sleep(1000);
  await pickTheme("dark");
  capture("23-release-settings-dark.png", 700);
  await pickTheme("system"); // back to System
} finally {
  // Every restore step runs even if an earlier one throws; failures are reported at the end.
  const failed = [];
  const step = async (name, fn) => {
    try {
      await fn();
    } catch (e) {
      failed.push(`${name}: ${e.message}`);
      log(`CLEANUP FAILED (${name}): ${e.message}`);
    }
  };
  await step("quit Burrow", async () => {
    execFileSync("osascript", ["-e", 'quit app "Burrow"']);
    await sleep(5000);
    log(`after quit, Burrow/cloudflared processes left: ${execFileSync("sh", ["-c", "ps -axo command | grep -cE '[B]urrow.app/Contents/MacOS|[c]om.davidsonsns.burrow/neutral' || true"]).toString().trim()}`);
  });
  await step("stop the origin server", () => {
    try { process.kill(-origin.pid); } catch { origin.kill(); }
  });
  await step("restore the login item", () => {
    log(`login item ${loginBefore.enabled ? "re-enable" : "disable"}: ${sh(BIN, ["--login-item", loginBefore.enabled ? "enable" : "disable"])}`);
  });
  await step("remove the seeded tunnel", () => {
    sql(`DELETE FROM tunnels WHERE id='${TID}'; DELETE FROM logs WHERE tunnel_id='${TID}'; DELETE FROM requests WHERE tunnel_id='${TID}'; DELETE FROM projects WHERE id NOT IN (SELECT project_id FROM tunnels);`);
    log(`tunnels left in the real data folder: ${sql("SELECT name, autostart FROM tunnels")}`);
  });
  // The run ends on System; put the owner's theme back (Burrow has quit, so write the row).
  await step("restore the theme", () => {
    if (themeBefore && theme() !== themeBefore) {
      sql(`UPDATE settings SET value=json_set(value,'$.theme','${themeBefore}') WHERE key='settings'`);
    }
    log(`theme now: ${theme()} (before the run: ${themeBefore})`);
  });
  await step("collapse Hidden Bar", () => {
    if (expanded) { const c = extras("Hidden Bar")[0]; click(c.x + c.w / 2, 12); }
  });
  if (failed.length) {
    process.exitCode = 1;
    log(`cleanup incomplete, check by hand: ${failed.join("; ")}`);
  }
}
