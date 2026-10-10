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
const sql = (q) => sh("sqlite3", [DB, q]);
const extras = (app) => sh("swift", ["scripts/menubar-extras.swift", app]).split("\n")
  .map((l) => l.match(/^(\d+) x=(-?\d+) w=(\d+)/)).filter(Boolean).map((m) => ({ x: +m[2], w: +m[3] }));
const click = (x, y) => execFileSync("swift", ["scripts/click.swift", String(Math.round(x)), String(Math.round(y))]);
const windows = () => sh("swift", ["scripts/window-bounds.swift", "Burrow"]).split("\n").filter(Boolean).map((l) => JSON.parse(l));
const capture = (name, min = 300, max = 4000) => execFileSync("swift", ["scripts/capture-window.swift", "Burrow", join(OUT, name), String(min), String(max)], { stdio: "inherit" });

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
try {
  log(`login item before: ${sh(BIN, ["--login-item", "status"])}`);
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
  click(dash.x + 60, dash.y + 147); // sidebar: Settings
  await sleep(1200);
  click(dash.x + 832, dash.y + 327); // Appearance: Light
  await sleep(1200);
  capture("21-release-settings-light.png", 700);
  log(`theme after clicking Light: ${sql("SELECT json_extract(value,'$.theme') FROM settings WHERE key='settings'")}`);
  click(dash.x + 60, dash.y + 60); // sidebar: Tunnels
  await sleep(1200);
  capture("22-release-dashboard-light.png", 700);
  click(dash.x + 60, dash.y + 147);
  await sleep(1000);
  click(dash.x + 886, dash.y + 327); // Appearance: Dark
  await sleep(1200);
  capture("23-release-settings-dark.png", 700);
  log(`theme after clicking Dark: ${sql("SELECT json_extract(value,'$.theme') FROM settings WHERE key='settings'")}`);
  click(dash.x + 770, dash.y + 327); // back to System
  await sleep(800);
  log(`theme restored: ${sql("SELECT json_extract(value,'$.theme') FROM settings WHERE key='settings'")}`);
} finally {
  execFileSync("osascript", ["-e", 'quit app "Burrow"']);
  await sleep(5000);
  log(`after quit, Burrow/cloudflared processes left: ${execFileSync("sh", ["-c", "ps -axo command | grep -cE '[B]urrow.app/Contents/MacOS|[c]om.davidsonsns.burrow/neutral' || true"]).toString().trim()}`);
  log(`login item disable: ${sh(BIN, ["--login-item", "disable"])}`);
  sql(`DELETE FROM tunnels WHERE id='${TID}'; DELETE FROM logs WHERE tunnel_id='${TID}'; DELETE FROM requests WHERE tunnel_id='${TID}'; DELETE FROM projects WHERE id NOT IN (SELECT project_id FROM tunnels);`);
  log(`tunnels left in the real data folder: ${sql("SELECT name, autostart FROM tunnels")}`);
  try { process.kill(-origin.pid); } catch { origin.kill(); }
  if (expanded) { const c = extras("Hidden Bar")[0]; click(c.x + c.w / 2, 12); }
}
