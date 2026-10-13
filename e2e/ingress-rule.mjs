#!/usr/bin/env node
// Local P2 regression against the real app and shared fake; no account or URL fetch.
import { spawn, execFileSync } from "node:child_process";
import { mkdtempSync, mkdirSync, copyFileSync, chmodSync, writeFileSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";
import http from "node:http";

const app = resolve(process.argv[2] ?? "target/debug/bundle/macos/Underlane.app");
const root = mkdtempSync(join(tmpdir(), "underlane-ingress-"));
const data = join(root, "data"), bindir = join(root, "bin");
mkdirSync(data); mkdirSync(bindir);
const fake = join(bindir, "cloudflared"), mode = join(bindir, "mode");
copyFileSync(resolve("crates/underlane-core/tests/common/fake_cloudflared.py"), fake);
chmodSync(fake, 0o755); writeFileSync(mode, "ok");
const config = join(root, "config.yml");
const source = (port) => `tunnel: synthetic-p2\ningress:\n  - hostname: test.example.invalid\n    path: ^/api\n    service: http://localhost:${port}\n  - hostname: "*.example.invalid"\n    service: http://localhost:43211\n  - service: http_status:404\n`;
const a = source(43210), b = source(43212);
writeFileSync(config, a);
const settings = JSON.stringify({ cloudflared_path: fake, theme: "light" }).replaceAll("'", "''");
execFileSync("sqlite3", [join(data, "underlane.db"), `CREATE TABLE settings (key TEXT PRIMARY KEY,value TEXT NOT NULL); INSERT INTO settings VALUES ('settings','${settings}');`]);
const port = 48000 + Math.floor(Math.random() * 500);
const child = spawn(join(app, "Contents/MacOS/Underlane"), [], {
  env: { ...process.env, UNDERLANE_DATA_DIR: data, UNDERLANE_E2E_PORT: String(port) }, stdio: ["ignore", "pipe", "pipe"],
});
let appLog = "", id, sequence = 0;
let fetched = 0;
const origin = http.createServer((_request, response) => { fetched++; response.end("unexpected fetch"); });
await new Promise((resolve) => origin.listen(0, "127.0.0.1", resolve));
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
const test = (url, tunnelId = id) => ctl("POST", `/ingress-rule?id=${tunnelId}&url=${encodeURIComponent(url)}&requestId=e2e-${++sequence}`);
try {
  await wait("health", async () => (await ctl("GET", "/health")).ok);
  check((await ctl("POST", `/import?path=${encodeURIComponent(config)}`)).added.length === 1, "disposable config imported");
  id = (await ctl("GET", "/overview")).tunnels.find((t) => t.config_path === config).id;
  for (const [url, index, service] of [
    ["https://test.example.invalid/api/users", 0, "http://localhost:43210"],
    ["https://other.example.invalid/", 1, "http://localhost:43211"],
    ["https://elsewhere.invalid/", 2, "http_status:404"],
  ]) {
    const result = await test(url);
    check(result.rule_index === index && result.rules[index].service === service, `offline rule #${index} maps to source service`);
    check(result.source_path === config, "result identifies source on disk");
  }
  const safe = JSON.stringify(await test("https://invented-user:invented-pass@test.example.invalid/api?token=invented-token"));
  check(!safe.includes("invented-"), "formatted match excludes submitted secrets");
  await test(`http://127.0.0.1:${origin.address().port}/submitted-url`);
  check(fetched === 0, "submitted URL is never fetched");
  const single = join(root, "single.yml");
  writeFileSync(single, "url: http://localhost:43213\n");
  await ctl("POST", `/import?path=${encodeURIComponent(single)}`);
  const singleId = (await ctl("GET", "/overview")).tunnels.find((t) => t.config_path === single).id;
  let unsupported;
  try { await test("https://test.example.invalid/", singleId); } catch (error) { unsupported = String(error); }
  check(unsupported?.includes("no local ingress"), "single-origin config receives unsupported explanation");
  const quick = await ctl("POST", "/quick?name=unsupported&origin=http%3A%2F%2Flocalhost%3A43214");
  unsupported = null;
  try { await test("https://test.example.invalid/", quick.id); } catch (error) { unsupported = String(error); }
  check(unsupported?.includes("locally managed"), "quick tunnel cannot invoke the local rule tester");
  for (const fixture of ["ingress-malformed", "ingress-out-of-range", "ingress-invalid"]) {
    writeFileSync(mode, fixture);
    let failed = false;
    try { await test("https://test.example.invalid/api"); } catch { failed = true; }
    check(failed, `${fixture} produces an error instead of a match`);
  }
  writeFileSync(mode, "ingress-slow");
  const requestId = `e2e-cancel-${++sequence}`;
  const pending = ctl("POST", `/ingress-rule?id=${id}&url=https%3A%2F%2Ftest.example.invalid%2Fapi&requestId=${requestId}`)
    .then(() => false, () => true);
  await sleep(150);
  await ctl("POST", `/cancel-ingress?requestId=${requestId}`);
  check(await pending, "native cancellation rejects a pending match");
  writeFileSync(mode, "ok");
  await ctl("POST", `/start?id=${id}`);
  const running = await wait("connected", async () => { const s = await state(); return s.status === "connected" && s.pid ? s : null; });
  writeFileSync(config, b);
  const result = await test("https://test.example.invalid/api");
  check(result.rules[0].service === "http://localhost:43212", "tester uses new config on disk");
  check((await state()).pid === running.pid && readFileSync(join(data, "derived", `${id}.yml`), "utf8").includes("43210"), "old connector stays on loaded config");
  check((await ctl("GET", `/config-change?id=${id}`)).status === "changed", "P1 warning remains after disk test");
  const text = (await ctl("GET", `/ui?view=tunnels:${id}`)).text;
  check(text.includes("Hostnames") && text.includes("Test a URL") && text.includes("Config changed on disk"), "Hostnames tester and mismatch warning coexist");
  let invalid;
  try { await test("ftp://test.example.invalid/"); } catch (error) { invalid = String(error); }
  check(!!invalid, "non HTTP URL is rejected");
  check(readFileSync(config, "utf8") === b, "tests never write imported source");
  const logs = JSON.stringify(await ctl("GET", `/logs?id=${id}`));
  check(!logs.includes("invented-"), "submitted secrets absent from persisted logs");
  console.log("INGRESS RULE E2E PASSED");
} catch (error) {
  evidence.error = String(error); console.error(error, appLog); process.exitCode = 1;
} finally {
  if (id) await ctl("POST", `/stop?id=${id}`).catch(() => {});
  await ctl("POST", "/quit").catch(() => {});
  await Promise.race([new Promise((resolve) => child.once("exit", resolve)), sleep(1500)]);
  if (child.exitCode === null) child.kill("SIGTERM");
  await new Promise((resolve) => origin.close(resolve));
  mkdirSync("e2e/out", { recursive: true });
  writeFileSync("e2e/out/ingress-rule-evidence.json", JSON.stringify(evidence, null, 2));
}
