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
import { spawn, execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import http from "node:http";
import https from "node:https";
import { Resolver } from "node:dns/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const args = process.argv.slice(2);
const flag = (name) => args.includes(name);
const opt = (name, dflt) => {
  const i = args.indexOf(name);
  return i >= 0 && args[i + 1] ? args[i + 1] : dflt;
};
const app = resolve(args.find((a, i) => !a.startsWith("--") && args[i - 1] !== "--label") ?? "target/debug/bundle/macos/Underlane.app");
const label = opt("--label", "latest");
// The label names one folder under e2e/out/ui, which is deleted and rewritten below.
if (!/^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(label) || label.includes("..")) {
  console.error(`bad --label "${label}": use letters, digits, ".", "_" or "-"`);
  process.exit(2);
}
const offline = flag("--offline");
const bin = join(app, "Contents/MacOS/Underlane");
const out = resolve("e2e/out/ui", label);
rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });
const CONTROL = 48600 + Math.floor(Math.random() * 400);
const dataDir = mkdtempSync(join(tmpdir(), "underlane-ui-shots-"));
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
    req.on("timeout", () => req.destroy(new Error("timeout")));
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
// screencapture can fail while a window is still animating in; retry a few times.
async function capture(name, minW = 700, maxW = 4000) {
  const file = join(out, name);
  for (let attempt = 1; ; attempt++) {
    try {
      // Pass our child's PID so a running copy of the user's own Underlane is never captured.
      execFileSync("swift", ["scripts/capture-window.swift", "Underlane", file, String(minW), String(maxW), String(child.pid)], { stdio: "inherit" });
      if (existsSync(file)) return file;
    } catch {}
    if (attempt === 4) throw new Error(`could not capture ${name}`);
    await sleep(800);
  }
}
function echoServer(name) {
  const s = http.createServer((req, res) => {
    let body = "";
    req.on("data", (c) => (body += c));
    req.on("end", () => {
      let received = null;
      try {
        received = body ? JSON.parse(body) : null;
      } catch {}
      const json = JSON.stringify({ service: name, method: req.method, path: req.url, received });
      res.writeHead(200, { "content-type": "application/json" });
      res.end(json);
    });
  });
  return new Promise((r) => s.listen(0, "127.0.0.1", () => r(s)));
}
// Resolve through 1.1.1.1: the system resolver may have cached NXDOMAIN for a new quick-tunnel name.
async function hit(url, method, body) {
  const u = new URL(url);
  const resolver = new Resolver();
  resolver.setServers(["1.1.1.1", "1.0.0.1"]);
  const ip = await waitFor(`dns ${u.hostname}`, async () => (await resolver.resolve4(u.hostname))[0], 90000, 2000);
  return waitFor(`200 from ${url}`, () => new Promise((res) => {
    const payload = body ? JSON.stringify(body) : undefined;
    const req = https.request({ host: u.hostname, servername: u.hostname, path: u.pathname + u.search, method,
      headers: payload ? { "content-type": "application/json", "content-length": Buffer.byteLength(payload) } : {},
      lookup: (_h, o, cb) => (o && o.all ? cb(null, [{ address: ip, family: 4 }]) : cb(null, ip, 4)) }, (r) => {
      r.resume();
      r.on("end", () => res(r.statusCode === 200));
    });
    req.on("error", () => res(false));
    req.end(payload);
  }), 90000, 2500);
}
function osAppearance() {
  try {
    return execFileSync("defaults", ["read", "-g", "AppleInterfaceStyle"], { stdio: ["ignore", "pipe", "ignore"] }).toString().trim() || "Light";
  } catch {
    return "Light"; // the key is absent in light mode
  }
}
const nav = (view) => ctl("POST", `/navigate?view=${encodeURIComponent(view)}`);

const child = spawn(bin, [], { env: { ...process.env, UNDERLANE_DATA_DIR: dataDir, UNDERLANE_E2E_PORT: String(CONTROL) }, stdio: "ignore" });
const servers = [];
let failed = false;
try {
  await waitFor("control server", async () => (await ctl("GET", "/health")).ok, 30000, 300);
  note(`label=${label} offline=${offline} app=${app}`);
  note(`OS appearance: ${osAppearance()} (theme "system" shots follow it)`);

  // Three quick tunnels to local echo servers: "web" is inspected, "web" and "api" run,
  // "db" stays stopped so the stopped states (dot, switch, detail) are in every shot.
  const ids = [];
  for (const [name, inspect] of [["web", true], ["api", false], ["db", false]]) {
    const s = await echoServer(name);
    servers.push(s);
    const r = await ctl("POST", `/quick?origin=${encodeURIComponent(`http://127.0.0.1:${s.address().port}`)}&name=${name}&inspect=${inspect ? 1 : 0}`);
    ids.push(r.id);
  }
  if (!offline) {
    const running = ids.slice(0, 2);
    for (const id of running) await ctl("POST", `/start?id=${id}`);
    const states = await waitFor("both tunnels connected", async () => {
      const st = await Promise.all(running.map((id) => ctl("GET", `/state?id=${id}`)));
      return st.every((s) => s.status === "connected" && s.public_url) ? st : null;
    }, 120000);
    note(`running: ${states.map((s) => s.public_url).join(", ")}`);
    const base = states[0].public_url;
    await hit(`${base}/api/orders?page=1`, "POST", { item: "notebook", qty: 2, gift: true });
    await hit(`${base}/api/orders/42`, "GET");
    await hit(`${base}/health`, "GET");
    await sleep(1500);
  }

  const shoot = async (theme, views) => {
    await ctl("POST", `/settings?theme=${theme}`);
    await sleep(800);
    for (const v of views) {
      if (v === "requests") {
        await ctl("GET", `/ui?view=${encodeURIComponent(`requests:${ids[0]}`)}&select=first`);
        await sleep(800);
      } else if (v === "popover") {
        await ctl("POST", "/popover");
        await sleep(1200);
        await capture(`${theme}-popover.png`, 300, 420);
        await ctl("POST", "/popover");
        await sleep(600);
        continue;
      } else {
        await nav({ tunnels: `tunnels:${ids[0]}`, logs: `logs:${ids[0]}`, settings: "settings", add: "add" }[v]);
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
  const expected = [...["light", "dark"].flatMap((t) => all.map((v) => `${t}-${v}.png`)), "system-tunnels.png", "system-popover.png"];
  const missing = expected.filter((f) => !existsSync(join(out, f)));
  if (missing.length) throw new Error(`missing screenshots: ${missing.join(", ")}`);
  note(`wrote ${out}`);
} catch (e) {
  failed = true;
  note(`FAILED: ${e.message}`);
} finally {
  try {
    await ctl("POST", "/quit");
  } catch {}
  await sleep(2000);
  if (child.exitCode == null) child.kill("SIGTERM");
  servers.forEach((s) => s.close());
  writeFileSync(join(out, "notes.txt"), notes.join("\n") + "\n");
  rmSync(dataDir, { recursive: true, force: true });
  process.exit(failed ? 1 : 0);
}
