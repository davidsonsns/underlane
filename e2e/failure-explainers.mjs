#!/usr/bin/env node
// Local P3 regression: real app, synthetic diagnostics, no account or origin requests.
import { spawn, execFileSync } from "node:child_process";
import { mkdtempSync, mkdirSync, copyFileSync, chmodSync, writeFileSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";
import http from "node:http";

const app = resolve(process.argv[2] ?? "target/debug/bundle/macos/Underlane.app");
const root = mkdtempSync(join(tmpdir(), "underlane-explainers-"));
const data = join(root, "data"), bindir = join(root, "bin");
mkdirSync(data); mkdirSync(bindir);
const fake = join(bindir, "cloudflared"), mode = join(bindir, "mode");
copyFileSync(resolve("crates/underlane-core/tests/common/fake_cloudflared.py"), fake);
chmodSync(fake, 0o755); writeFileSync(mode, "ok");
const settings = JSON.stringify({ cloudflared_path: fake, theme: "light" }).replaceAll("'", "''");
execFileSync("sqlite3", [join(data, "underlane.db"), `CREATE TABLE settings (key TEXT PRIMARY KEY,value TEXT NOT NULL); INSERT INTO settings VALUES ('settings','${settings}');`]);
const port = 47500 + Math.floor(Math.random() * 500);
const child = spawn(join(app, "Contents/MacOS/Underlane"), [], {
  env: { ...process.env, UNDERLANE_DATA_DIR: data, UNDERLANE_E2E_PORT: String(port) }, stdio: ["ignore", "pipe", "pipe"],
});
let appLog = "", id, quickId;
child.stdout.on("data", (chunk) => { appLog += chunk; });
child.stderr.on("data", (chunk) => { appLog += chunk; });
const evidence = { root, steps: [] };
function check(condition, message) {
  if (!condition) throw new Error(message);
  console.log(`ok: ${message}`); evidence.steps.push(message);
}
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
function ctl(method, path) {
  return new Promise((yes, no) => {
    const request = http.request({ host: "127.0.0.1", port, method, path, timeout: 12000 }, (response) => {
      let text = "";
      response.on("data", (chunk) => { text += chunk; });
      response.on("end", () => {
        try { const value = JSON.parse(text); response.statusCode === 200 ? yes(value) : no(new Error(text)); }
        catch (error) { no(error); }
      });
    });
    request.on("error", no); request.on("timeout", () => request.destroy(new Error("control timeout"))); request.end();
  });
}
async function wait(message, fn) {
  for (let i = 0; i < 100; i++) {
    try { const value = await fn(); if (value) return value; } catch {}
    await sleep(100);
  }
  throw new Error(`Timed out: ${message}`);
}
const state = () => ctl("GET", `/state?id=${id}`);
const ui = () => ctl("GET", `/ui?view=tunnels:${id}`);
async function start(fixture) {
  await ctl("POST", `/stop?id=${id}`);
  writeFileSync(mode, fixture);
  await ctl("POST", `/start?id=${id}`);
}
try {
  await wait("health", async () => (await ctl("GET", "/health")).ok);
  id = (await ctl("POST", "/quick?name=P3%20regression&origin=http%3A%2F%2Flocalhost%3A43215")).id;
  quickId = id;
  await start("runtime-known");
  const known = await wait("known failure", async () => { const s = await state(); return s.status === "crashed" && s.failure ? s : null; });
  check(known.failure.cause.includes("credentials") && known.failure.action, "known runtime failure shows cause and next action");
  check(known.failure.raw.includes("authentication failed") && !JSON.stringify(known).includes("invented-"), "complete runtime diagnostic is redacted");
  check(known.supervised && known.detail.includes("Restarting"), "crash retry and backoff remain supervised");
  const visible = (await ui()).text;
  check(visible.includes(known.failure.cause) && visible.includes("Raw diagnostic") && visible.includes("Try again now"), "failure explanation and retry are visible");
  await start("runtime-burst");
  const burst = await wait("burst failure", async () => { const s = await state(); return s.status === "crashed" && s.failure ? s : null; });
  check(burst.failure.cause.includes("credentials") && burst.failure.raw.includes("authentication failed") && burst.failure.raw.includes("synthetic exit context 39"), "exit keeps the observed error behind a burst of info logs");
  await start("runtime-unknown");
  const unknown = await wait("unknown failure", async () => { const s = await state(); return s.status === "crashed" && s.failure ? s : null; });
  check(unknown.failure.cause.includes("unrecognized") && unknown.failure.raw.includes("mystery failure"), "malformed unknown output keeps safe raw without diagnosis");
  check(!JSON.stringify(unknown).includes("invented-"), "malformed raw diagnostic hides URL and token secrets");
  await start("runtime-warning");
  await wait("connected despite warning", async () => (await state()).status === "connected");
  check(!(await state()).failure && !(await ui()).text.includes("Raw diagnostic"), "warnings and successful output do not create failure notices");
  await start("runtime-recover");
  const waiting = await wait("edge failure", async () => { const s = await state(); return s.failure && s.pid ? s : null; });
  check(waiting.failure.cause.includes("edge") && waiting.status !== "crashed", "error logs preserve the living connector state");
  writeFileSync(mode, "ok");
  const recovered = await wait("edge recovery", async () => { const s = await state(); return s.status === "connected" && !s.failure ? s : null; });
  check(recovered.pid === waiting.pid, "healthy edge clears obsolete explanation without replacing child");
  check(!(await ui()).text.includes("Raw diagnostic"), "recovered UI clears diagnostic disclosure");
  await start("runtime-origin");
  await wait("ready edge and origin failure", async () => { const s = await state(); return s.status === "connected" && s.failure ? s : null; });
  await sleep(2300);
  const origin = await state();
  check(origin.status === "connected" && origin.failure?.cause.includes("origin"), "edge readiness does not claim origin recovery");
  check(!JSON.stringify(origin).includes("invented-"), "origin URL credentials and query secrets remain redacted");
  await ctl("POST", `/stop?id=${id}`);
  check(!(await state()).failure, "Stop clears the current failure explanation");
  writeFileSync(mode, "ok");
  const config = join(root, "invalid.yml");
  const source = "ingress:\n - hostname: invalid.example.invalid\n   service: http://localhost:43216\n";
  writeFileSync(config, source);
  await ctl("POST", `/import?path=${encodeURIComponent(config)}`);
  id = (await ctl("GET", "/overview")).tunnels.find((t) => t.config_path === config).id;
  await start("invalid-config");
  const prep = await wait("preparation failure", async () => { const s = await state(); return s.status === "crashed" && s.failure ? s : null; });
  check(prep.failure.cause.includes("config") && prep.failure.action?.includes("http_status:404"), "preparation failure explains config cause and fix");
  check(readFileSync(config, "utf8") === source, "failure handling never edits imported YAML");
  const logs = await ctl("GET", `/logs?id=${quickId}`);
  check(logs.some((line) => line.message.includes("mystery failure")) && !JSON.stringify(logs).includes("invented-"), "persisted runtime logs contain diagnostics without synthetic secrets");
  console.log("FAILURE EXPLAINERS E2E PASSED");
} catch (error) {
  evidence.error = String(error); console.error(error, appLog); process.exitCode = 1;
} finally {
  if (id) await ctl("POST", `/stop?id=${id}`).catch(() => {});
  await ctl("POST", "/quit").catch(() => {});
  await Promise.race([new Promise((resolve) => child.once("exit", resolve)), sleep(1500)]);
  if (child.exitCode === null) child.kill("SIGTERM");
  mkdirSync("e2e/out", { recursive: true });
  writeFileSync("e2e/out/failure-explainers-evidence.json", JSON.stringify(evidence, null, 2));
}
