#!/usr/bin/env node
// Launch-at-login verification against the installed release build:
//   1. register the login item with SMAppService (what the Settings toggle does)
//   2. control: a plain `open` launch -> Underlane records reason "normal"
//   3. a launch carrying macOS's login-item Apple event (keyAELaunchedAsLogInItem), exactly what
//      the system sends when it opens login items -> Underlane records "login_item", opens no
//      window, and auto-starts tunnels marked "Start automatically"
//   4. screenshots of that state, then restore everything
import { execFileSync, spawn } from "node:child_process";
import { homedir } from "node:os";
import { join } from "node:path";

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const APP = join(homedir(), "Applications/Underlane.app");
const BIN = join(APP, "Contents/MacOS/Underlane");
const DB = join(homedir(), "Library/Application Support/com.davidsonsns.underlane/underlane.db");
const sh = (cmd, args) => execFileSync(cmd, args).toString().trim();
const sql = (q) => sh("sqlite3", [DB, q]);
const log = (m) => console.log(`[login-item-check] ${m}`);
const pid = () => execFileSync("sh", ["-c", "pgrep -f 'Applications/Underlane.app/Contents/MacOS/Underlane$' || true"]).toString().trim();
const windows = () => sh("swift", ["scripts/window-bounds.swift", "Underlane"]).split("\n").filter(Boolean).map((l) => JSON.parse(l));
const extras = (app) => sh("swift", ["scripts/menubar-extras.swift", app]).split("\n").map((l) => l.match(/^(\d+) x=(-?\d+) w=(\d+)/)).filter(Boolean).map((m) => ({ x: +m[2], w: +m[3] }));
const click = (x, y) => execFileSync("swift", ["scripts/click.swift", String(Math.round(x)), String(Math.round(y))]);
const capture = (name, min, max = 4000) => execFileSync("swift", ["scripts/capture-window.swift", "Underlane", join("docs/screenshots", name), String(min), String(max)], { stdio: "inherit" });
async function quit() {
  if (!pid()) return;
  execFileSync("osascript", ["-e", 'quit app "Underlane"']);
  for (let i = 0; i < 40 && pid(); i++) await sleep(250);
}

if (pid()) throw new Error("quit Underlane first");
const origin = spawn("python3", ["-m", "http.server", "18779", "--bind", "127.0.0.1"], { stdio: "ignore", detached: true });
const TID = "login-item-check";
sql(`INSERT OR IGNORE INTO projects(id,name,sort) VALUES ('lic','Login check',99);
     INSERT OR REPLACE INTO tunnels(id,project_id,name,kind,origin_url,inspect,autostart,loglevel,created_at,sort)
     VALUES ('${TID}','lic','starts-at-login','quick','http://127.0.0.1:18779',0,1,'info',${Date.now()},99);`);
let expanded = false;
try {
  log(`register: ${sh(BIN, ["--login-item", "enable"])}`);

  // Control: a plain open is NOT a login launch.
  execFileSync("open", ["-g", APP]);
  for (let i = 0; i < 40 && !sql("SELECT value FROM settings WHERE key='last_launch'").includes(String(Date.now()).slice(0, 6)); i++) await sleep(250);
  await sleep(2000);
  log(`plain open -> last_launch = ${sql("SELECT value FROM settings WHERE key='last_launch'")}`);
  await quit();

  // The login-item launch.
  const before = Date.now();
  log(sh("swift", ["scripts/launch-as-login-item.swift", APP]));
  let launch = "";
  for (let i = 0; i < 60; i++) {
    await sleep(500);
    launch = sql("SELECT value FROM settings WHERE key='last_launch'");
    if (launch && JSON.parse(launch).at >= before) break;
  }
  log(`login-item launch -> last_launch = ${launch}`);
  const p = pid();
  log(`pid ${p}, parent ${sh("sh", ["-c", `ps -o comm= -p $(ps -o ppid= -p ${p})`])}, tty ${sh("ps", ["-o", "tty=", "-p", p])}`);
  log(`launchd job: ${sh("sh", ["-c", "launchctl list | grep application.com.davidsonsns.underlane || true"])}`);
  await sleep(1500);
  log(`windows opened by the login launch: ${windows().length} (none expected)`);
  let url = "";
  for (let i = 0; i < 90 && !url; i++) {
    await sleep(1000);
    const m = sql(`SELECT message FROM logs WHERE tunnel_id='${TID}' AND message LIKE '%trycloudflare.com%' AND ts >= ${before} ORDER BY id DESC LIMIT 1`).match(/https:\/\/[a-z0-9-]+\.trycloudflare\.com/);
    if (m) url = m[0];
  }
  for (let i = 0; i < 30 && !sql(`SELECT count(*) FROM logs WHERE tunnel_id='${TID}' AND ts >= ${before} AND message LIKE 'Registered tunnel connection%'`).startsWith("1"); i++) await sleep(1000);
  log(`auto-started "starts-at-login": ${url}; ${sql(`SELECT message FROM logs WHERE tunnel_id='${TID}' AND ts >= ${before} AND message LIKE 'Registered tunnel connection%' LIMIT 1`)}`);

  // Screenshots of the login state.
  let item = extras("Underlane")[0];
  if (item.x < 0) { const c = extras("Hidden Bar")[0]; click(c.x + c.w / 2, 12); expanded = true; await sleep(1200); item = extras("Underlane")[0]; }
  execFileSync("screencapture", ["-x", "-R", `${item.x - 110},0,${item.w + 220},30`, "docs/screenshots/24-login-item-menu-bar.png"]);
  click(item.x + item.w / 2, 12);
  await sleep(1500);
  capture("25-login-item-popover.png", 300, 420);
  const pop = windows().find((w) => w.w <= 420);
  click(pop.x + 80, pop.y + pop.h - 20);
  await sleep(2500);
  const dash = windows().find((w) => w.w > 700);
  click(dash.x + 60, dash.y + 147);
  await sleep(1500);
  capture("26-login-item-settings.png", 700);
} finally {
  await quit();
  log(`after quit: ${execFileSync("sh", ["-c", "ps -axo command | grep -cE '[c]om.davidsonsns.underlane/neutral' || true"]).toString().trim()} cloudflared left`);
  log(`unregister: ${sh(BIN, ["--login-item", "disable"])}`);
  sql(`DELETE FROM tunnels WHERE id='${TID}'; DELETE FROM logs WHERE tunnel_id='${TID}'; DELETE FROM projects WHERE id NOT IN (SELECT project_id FROM tunnels);`);
  log(`tunnels left: ${sql("SELECT name, autostart FROM tunnels")}`);
  try { process.kill(-origin.pid); } catch {}
  if (expanded) { const c = extras("Hidden Bar")[0]; click(c.x + c.w / 2, 12); }
}
