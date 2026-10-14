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
import { sleep } from "./lib/harness.mjs";
import {
  capture,
  captureMenuStrip,
  click,
  openDashboard,
  revealMenuItem,
  sh,
  toggleHiddenBar,
  windows,
} from "./lib/native.mjs";

const APP = join(homedir(), "Applications/Underlane.app");
const BIN = join(APP, "Contents/MacOS/Underlane");
const DB = join(
  homedir(),
  "Library/Application Support/com.davidsonsns.underlane/underlane.db"
);
const sql = (q) => sh("sqlite3", [DB, q]);
const log = (m) => console.log(`[login-item-check] ${m}`);
const pid = () =>
  execFileSync("sh", [
    "-c",
    "pgrep -f 'Applications/Underlane.app/Contents/MacOS/Underlane$' || true",
  ])
    .toString()
    .trim();
async function quit() {
  if (!pid()) {
    return;
  }
  execFileSync("osascript", ["-e", 'quit app "Underlane"']);
  for (let i = 0; i < 40 && pid(); i += 1) {
    // biome-ignore lint/performance/noAwaitInLoops: Polls and native UI actions must complete in order before the next step.
    await sleep(250);
  }
}

if (pid()) {
  throw new Error("quit Underlane first");
}
if (Number(sql("PRAGMA user_version")) !== 3) {
  throw new Error(
    "Launch the updated Underlane once, then quit it to initialize schema v3 before running this check."
  );
}
const origin = spawn(
  "python3",
  ["-m", "http.server", "18779", "--bind", "127.0.0.1"],
  { detached: true, stdio: "ignore" }
);
const TID = "login-item-check";
sql(`INSERT OR IGNORE INTO groups(id,name,sort) VALUES ('lic','Login check',99);
     INSERT OR REPLACE INTO tunnels(id,group_id,name,kind,origin_url,inspect,autostart,loglevel,created_at,sort)
     VALUES ('${TID}','lic','starts-at-login','quick','http://127.0.0.1:18779',0,1,'info',${Date.now()},99);`);
let expanded = false;
try {
  log(`register: ${sh(BIN, ["--login-item", "enable"])}`);

  // Control: a plain open is NOT a login launch.
  execFileSync("open", ["-g", APP]);
  for (
    let i = 0;
    i < 40 &&
    !sql("SELECT value FROM settings WHERE key='last_launch'").includes(
      String(Date.now()).slice(0, 6)
    );
    i += 1
  ) {
    // biome-ignore lint/performance/noAwaitInLoops: Polls and native UI actions must complete in order before the next step.
    await sleep(250);
  }
  await sleep(2000);
  log(
    `plain open -> last_launch = ${sql("SELECT value FROM settings WHERE key='last_launch'")}`
  );
  await quit();

  // The login-item launch.
  const before = Date.now();
  log(sh("swift", ["scripts/launch-as-login-item.swift", APP]));
  let launch = "";
  for (let i = 0; i < 60; i += 1) {
    // biome-ignore lint/performance/noAwaitInLoops: Polls and native UI actions must complete in order before the next step.
    await sleep(500);
    launch = sql("SELECT value FROM settings WHERE key='last_launch'");
    if (launch && JSON.parse(launch).at >= before) {
      break;
    }
  }
  log(`login-item launch -> last_launch = ${launch}`);
  const p = pid();
  log(
    `pid ${p}, parent ${sh("sh", ["-c", `ps -o comm= -p $(ps -o ppid= -p ${p})`])}, tty ${sh("ps", ["-o", "tty=", "-p", p])}`
  );
  log(
    `launchd job: ${sh("sh", ["-c", "launchctl list | grep application.com.davidsonsns.underlane || true"])}`
  );
  await sleep(1500);
  log(
    `windows opened by the login launch: ${windows().length} (none expected)`
  );
  let url = "";
  for (let i = 0; i < 90 && !url; i += 1) {
    // biome-ignore lint/performance/noAwaitInLoops: Polls and native UI actions must complete in order before the next step.
    await sleep(1000);
    const m = sql(
      `SELECT message FROM logs WHERE tunnel_id='${TID}' AND message LIKE '%trycloudflare.com%' AND ts >= ${before} ORDER BY id DESC LIMIT 1`
    ).match(/https:\/\/[a-z0-9-]+\.trycloudflare\.com/);
    if (m) {
      [url] = m;
    }
  }
  for (
    let i = 0;
    i < 30 &&
    !sql(
      `SELECT count(*) FROM logs WHERE tunnel_id='${TID}' AND ts >= ${before} AND message LIKE 'Registered tunnel connection%'`
    ).startsWith("1");
    i += 1
  ) {
    // biome-ignore lint/performance/noAwaitInLoops: Polls and native UI actions must complete in order before the next step.
    await sleep(1000);
  }
  log(
    `auto-started "starts-at-login": ${url}; ${sql(`SELECT message FROM logs WHERE tunnel_id='${TID}' AND ts >= ${before} AND message LIKE 'Registered tunnel connection%' LIMIT 1`)}`
  );

  // Screenshots of the login state.
  const revealed = await revealMenuItem();
  ({ expanded } = revealed);
  const { item } = revealed;
  if (!item || item.x < 0) {
    throw new Error("Underlane menu bar item is unavailable");
  }
  captureMenuStrip(item, "docs/screenshots/24-login-item-menu-bar.png");
  click(item.x + item.w / 2, 12);
  await sleep(1500);
  capture("25-login-item-popover.png", 300, 420);
  const dash = await openDashboard();
  click(dash.x + 60, dash.y + 147);
  await sleep(1500);
  capture("26-login-item-settings.png", 700);
} finally {
  await quit();
  log(
    `after quit: ${execFileSync("sh", ["-c", "ps -axo command | grep -cE '[c]om.davidsonsns.underlane/neutral' || true"]).toString().trim()} cloudflared left`
  );
  log(`unregister: ${sh(BIN, ["--login-item", "disable"])}`);
  sql(
    `DELETE FROM tunnels WHERE id='${TID}'; DELETE FROM logs WHERE tunnel_id='${TID}'; DELETE FROM groups WHERE id NOT IN (SELECT group_id FROM tunnels WHERE group_id IS NOT NULL);`
  );
  log(`tunnels left: ${sql("SELECT name, autostart FROM tunnels")}`);
  try {
    process.kill(-origin.pid);
  } catch {
    /* A retry or cleanup tolerates this operation failing. */
  }
  if (expanded) {
    toggleHiddenBar();
  }
}
