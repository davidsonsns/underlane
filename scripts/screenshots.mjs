#!/usr/bin/env node
// Drives the real app (debug build with the e2e control server) through the manual
// verification scenarios and captures window-only screenshots into docs/screenshots/.
//
// usage: node scripts/screenshots.mjs target/debug/bundle/macos/Burrow.app
import { spawn, execFileSync } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync } from "node:fs";
import http from "node:http";
import https from "node:https";
import { Resolver } from "node:dns/promises";
import { homedir, tmpdir } from "node:os";
import { join, resolve } from "node:path";

const app = resolve(process.argv[2] ?? "target/debug/bundle/macos/Burrow.app");
const bin = join(app, "Contents/MacOS/Burrow");
const out = resolve("docs/screenshots");
mkdirSync(out, { recursive: true });
const CONTROL = 48100 + Math.floor(Math.random() * 500);
const dataDir = mkdtempSync(join(tmpdir(), "burrow-shots-"));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const notes = [];
const note = (s) => {
  console.log(s);
  notes.push(s);
};

function ctl(method, path) {
  return new Promise((res, rej) => {
    const req = http.request({ host: "127.0.0.1", port: CONTROL, method, path, timeout: 30000 }, (r) => {
      let b = "";
      r.on("data", (c) => (b += c));
      r.on("end", () => {
        try {
          res(JSON.parse(b));
        } catch {
          res(b);
        }
      });
    });
    req.on("error", rej);
    req.end();
  });
}
async function waitFor(what, fn, ms = 90000, every = 1000) {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    try {
      const v = await fn();
      if (v) return v;
    } catch {}
    await sleep(every);
  }
  throw new Error(`timed out: ${what}`);
}
function capture(name, minW = 700, maxW = 4000) {
  const file = join(out, name);
  execFileSync("swift", ["scripts/capture-window.swift", "Burrow", file, String(minW), String(maxW)], { stdio: "inherit" });
  return file;
}
// The menu bar may be managed by an app like Hidden Bar that collapses new items. Reveal
// Burrow's item for the capture, then put the menu bar back the way it was.
function extras(app) {
  try {
    const out = execFileSync("swift", ["scripts/menubar-extras.swift", app]).toString();
    return out.trim().split("\n").map((l) => {
      const m = l.match(/^(\d+) x=(-?\d+) w=(\d+)/);
      return m ? { i: +m[1], x: +m[2], w: +m[3] } : null;
    }).filter(Boolean);
  } catch {
    return [];
  }
}
async function captureMenuBar(name) {
  let item = extras("Burrow")[0];
  let expanded = false;
  if (!item || item.x < 0) {
    const chevron = extras("Hidden Bar")[0];
    if (chevron) {
      execFileSync("swift", ["scripts/click.swift", String(chevron.x + chevron.w / 2), "12"]);
      expanded = true;
      await sleep(1200);
      item = extras("Burrow")[0];
    }
  }
  if (item && item.x >= 0) {
    const x = Math.max(0, item.x - 110), w = item.w + 220;
    execFileSync("screencapture", ["-x", "-R", `${x},0,${w},30`, join(out, name)]);
    note(`menu bar strip captured around x=${item.x} -> ${name}`);
  } else {
    note(`could not reveal Burrow's menu bar item for ${name}`);
  }
  if (expanded) {
    const chevron = extras("Hidden Bar")[0];
    if (chevron) execFileSync("swift", ["scripts/click.swift", String(chevron.x + chevron.w / 2), "12"]);
  }
}

function launch() {
  const child = spawn(bin, [], { env: { ...process.env, BURROW_DATA_DIR: dataDir, BURROW_E2E_PORT: String(CONTROL) }, stdio: "ignore" });
  return child;
}
function echoServer(label) {
  const s = http.createServer((req, res) => {
    let body = "";
    req.on("data", (c) => (body += c));
    req.on("end", () => {
      const json = JSON.stringify({ service: label, method: req.method, path: req.url, received: body ? JSON.parse(body) : null, at: new Date().toISOString() });
      res.writeHead(200, { "content-type": "application/json" });
      res.end(json);
    });
  });
  return new Promise((r) => s.listen(0, "127.0.0.1", () => r(s)));
}
async function hit(url, method, body) {
  const host = new URL(url).hostname;
  const resolver = new Resolver();
  resolver.setServers(["1.1.1.1"]);
  const ip = await waitFor(`dns ${host}`, async () => (await resolver.resolve4(host))[0], 90000, 2000);
  return waitFor(`200 from ${url}`, () => new Promise((res) => {
    const payload = body ? JSON.stringify(body) : undefined;
    const req = https.request({ host, servername: host, path: new URL(url).pathname + new URL(url).search, method,
      headers: payload ? { "content-type": "application/json", "content-length": Buffer.byteLength(payload) } : {},
      lookup: (_h, o, cb) => (o && o.all ? cb(null, [{ address: ip, family: 4 }]) : cb(null, ip, 4)) }, (r) => {
      r.resume();
      r.on("end", () => res(r.statusCode === 200));
    });
    req.on("error", () => res(false));
    req.end(payload);
  }), 90000, 2500);
}

let child = launch();
const servers = [];
try {
  await waitFor("control server", async () => (await ctl("GET", "/health")).ok, 30000, 300);
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
  const specs = [["web", true], ["api", false], ["docs", false]];
  const ids = [];
  for (const [name, inspect] of specs) {
    const s = await echoServer(name);
    servers.push(s);
    const r = await ctl("POST", `/quick?origin=${encodeURIComponent(`http://127.0.0.1:${s.address().port}`)}&name=${name}&inspect=${inspect ? 1 : 0}`);
    ids.push(r.id);
    await ctl("POST", `/start?id=${r.id}`);
  }
  const states = await waitFor("all connected", async () => {
    const st = await Promise.all(ids.map((id) => ctl("GET", `/state?id=${id}`)));
    return st.every((s) => s.status === "connected" && s.public_url) ? st : null;
  }, 120000);
  note(`running: ${states.map((s) => s.public_url).join(", ")}; menu bar title = "${states[0].menu_bar_title}"`);

  // Traffic through the inspected tunnel.
  await hit(`${states[0].public_url}/api/orders?page=1`, "POST", { item: "notebook", qty: 2, gift: true });
  await hit(`${states[0].public_url}/api/orders/42`, "GET");
  await hit(`${states[0].public_url}/health`, "GET");
  await sleep(1500);

  // Menu bar crop around the tray item (shows the count).
  await captureMenuBar("03-menu-bar-count.png");
  await ctl("POST", "/popover");
  await sleep(1200);
  capture("04-popover-three-tunnels.png", 300, 420);
  await ctl("POST", "/popover");

  await ctl("POST", `/navigate?view=${encodeURIComponent(`tunnels:${ids[0]}`)}`);
  await sleep(1500);
  capture("05-dashboard-dark.png");

  await ctl("GET", `/ui?view=${encodeURIComponent(`requests:${ids[0]}`)}&select=first`);
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
  await ctl("POST", `/navigate?view=${encodeURIComponent(`tunnels:${ids[0]}`)}`);
  await sleep(1500);
  capture("10-dashboard-light.png");
  await ctl("GET", `/ui?view=${encodeURIComponent(`requests:${ids[0]}`)}&select=first`);
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
  const leftover = execFileSync("ps", ["-axww", "-o", "command="]).toString().split("\n").filter((l) => l.includes(dataDir) && l.includes("cloudflared"));
  note(`after quit: ${leftover.length} cloudflared processes left`);
  child = launch();
  await waitFor("control server (relaunch)", async () => (await ctl("GET", "/health")).ok, 30000, 300);
  const auto = await waitFor("autostart tunnels connected", async () => {
    const st = await Promise.all(ids.map((id) => ctl("GET", `/state?id=${id}`)));
    return st[0].status === "connected" && st[1].status === "connected" ? st : null;
  }, 120000);
  note(`after relaunch: ${auto.map((s, i) => `${specs[i][0]}=${s.status}`).join(", ")}; menu bar title = "${auto[0].menu_bar_title}"`);
  await captureMenuBar("14-menu-bar-after-autostart.png");
  await ctl("POST", "/popover");
  await sleep(1200);
  capture("13-autostart-after-relaunch.png", 300, 420);
  await ctl("POST", "/popover");
} finally {
  try {
    await ctl("POST", "/quit");
  } catch {}
  await sleep(2000);
  if (child.exitCode == null) child.kill("SIGTERM");
  servers.forEach((s) => s.close());
  writeFileSync(join(out, "session-notes.txt"), notes.join("\n") + "\n");
}
