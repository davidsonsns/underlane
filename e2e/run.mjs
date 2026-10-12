#!/usr/bin/env node
// End-to-end check, no Cloudflare account needed:
//   1. start a local echo server
//   2. launch the real Underlane app (debug build with the e2e control server) on a throwaway data dir
//   3. expose the echo server as a quick tunnel with inspection on
//   4. curl the trycloudflare.com URL with a JSON body
//   5. assert the bodies are in SQLite and in the dashboard's request viewer, and that the
//      menu bar count (read from the real NSStatusItem) goes 0 -> 1 -> 0
//
// usage: node e2e/run.mjs path/to/Underlane.app
import { spawn, execFile, execFileSync } from "node:child_process";
import { promisify } from "node:util";
import { mkdtempSync, mkdirSync, writeFileSync } from "node:fs";
import http from "node:http";
import { Resolver } from "node:dns/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const app = resolve(process.argv[2] ?? "target/debug/bundle/macos/Underlane.app");
const bin = join(app, "Contents/MacOS/Underlane");
const CONTROL = 47000 + Math.floor(Math.random() * 900);
const marker = `underlane-e2e-${Date.now().toString(36)}`;
const outDir = resolve("e2e/out");
mkdirSync(outDir, { recursive: true });
const evidence = { marker, steps: [] };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const t0 = Date.now();
const log = (msg, extra) => {
  const line = `[${((Date.now() - t0) / 1000).toFixed(1).padStart(5)}s] ${msg}`;
  console.log(line);
  evidence.steps.push(extra ? { msg, ...extra } : { msg });
};
function assert(cond, msg, extra) {
  if (!cond) {
    log(`FAIL: ${msg}`, extra);
    throw new Error(msg);
  }
  log(`ok: ${msg}`, extra);
}

// --- 1. echo server ---------------------------------------------------------------------
const echo = http.createServer((req, res) => {
  let body = "";
  req.on("data", (c) => (body += c));
  req.on("end", () => {
    let parsed = null;
    try {
      parsed = JSON.parse(body);
    } catch {}
    const out = JSON.stringify({ echoed: parsed, method: req.method, path: req.url, server: "e2e-echo" });
    res.writeHead(200, { "content-type": "application/json", "content-length": Buffer.byteLength(out) });
    res.end(out);
  });
});
await new Promise((r) => echo.listen(0, "127.0.0.1", r));
const echoPort = echo.address().port;
log(`echo server on 127.0.0.1:${echoPort}`);

// --- 2. launch the app ------------------------------------------------------------------
const dataDir = mkdtempSync(join(tmpdir(), "underlane-e2e-"));
const child = spawn(bin, [], {
  env: { ...process.env, UNDERLANE_DATA_DIR: dataDir, UNDERLANE_E2E_PORT: String(CONTROL), UNDERLANE_LOG: "info" },
  stdio: ["ignore", "pipe", "pipe"],
});
let appLog = "";
child.stdout.on("data", (d) => (appLog += d));
child.stderr.on("data", (d) => (appLog += d));

function ctl(method, path) {
  return new Promise((res, rej) => {
    const req = http.request({ host: "127.0.0.1", port: CONTROL, method, path, timeout: 30000 }, (r) => {
      let b = "";
      r.on("data", (c) => (b += c));
      r.on("end", () => {
        try {
          res({ status: r.statusCode, body: JSON.parse(b) });
        } catch {
          res({ status: r.statusCode, body: b });
        }
      });
    });
    req.on("error", rej);
    req.on("timeout", () => req.destroy(new Error("timeout")));
    req.end();
  });
}

async function waitFor(what, fn, timeoutMs, everyMs = 1000) {
  const end = Date.now() + timeoutMs;
  let last;
  while (Date.now() < end) {
    try {
      last = await fn();
      if (last) return last;
    } catch (e) {
      last = e;
    }
    await sleep(everyMs);
  }
  throw new Error(`timed out waiting for ${what} (last: ${JSON.stringify(last)})`);
}

let tunnelId;
let exitCode = 0;
try {
  await waitFor("control server", async () => (await ctl("GET", "/health")).body.ok, 30000, 300);
  log(`app running (pid ${child.pid}), data dir ${dataDir}`);

  const count = async () => {
    const s = (await ctl("GET", `/state?id=${tunnelId ?? ""}`)).body;
    return { s, title: s.menu_bar_title ?? "" };
  };

  let { s, title } = await count();
  assert(title === "" && s.active_count === 0, "menu bar count is 0 before starting (title hidden)", { menu_bar_title: title, active_count: s.active_count });

  // --- 3. quick tunnel with inspection ----------------------------------------------------
  const created = await ctl("POST", `/quick?origin=${encodeURIComponent(`http://127.0.0.1:${echoPort}`)}&name=e2e-echo&inspect=1`);
  tunnelId = created.body.id;
  assert(!!tunnelId, "quick tunnel created with inspection on", { id: tunnelId });
  await ctl("POST", `/start?id=${tunnelId}`);

  s = await waitFor(
    "tunnel connected with a public URL",
    async () => {
      const st = (await ctl("GET", `/state?id=${tunnelId}`)).body;
      if (st.status === "crashed") throw new Error(`crashed: ${st.detail}`);
      return st.status === "connected" && st.public_url ? st : null;
    },
    120000,
  );
  log(`connected: ${s.public_url} (inspecting=${s.inspecting})`);
  assert(s.inspecting === true, "inspection proxy is active");
  ({ title } = await count());
  assert(title === "1", "menu bar count is 1 while the tunnel runs", { menu_bar_title: title });

  // --- 4. curl the public URL ------------------------------------------------------------
  const host = new URL(s.public_url).hostname;
  const resolver = new Resolver();
  resolver.setServers(["1.1.1.1", "1.0.0.1"]);
  // Ask a public resolver directly: the system resolver may cache NXDOMAIN for the new name.
  const ip = await waitFor("DNS for the quick tunnel", async () => (await resolver.resolve4(host))[0], 120000, 2000);
  log(`${host} resolves to ${ip} (via 1.1.1.1)`);

  const payload = JSON.stringify({ hello: "underlane", marker, items: [1, 2, 3] });
  // Real curl, pinned to the IP from 1.1.1.1 with --resolve (the system resolver may have
  // cached NXDOMAIN for the brand-new hostname).
  const curlArgs = [
    "-sS", "--max-time", "20", "--resolve", `${host}:443:${ip}`,
    "-X", "POST", `https://${host}/e2e?source=run.mjs`,
    "-H", "content-type: application/json", "-H", "authorization: Bearer e2e-secret-token",
    "--data-raw", payload, "-w", "\n%{http_code}",
  ];
  evidence.curl = ["curl", ...curlArgs].join(" ");
  // Async on purpose: the echo server lives in this process, so a blocking exec would deadlock.
  const run = promisify(execFile);
  const send = async () => {
    try {
      const out = (await run("curl", curlArgs)).stdout.toString();
      const nl = out.lastIndexOf("\n");
      return { status: Number(out.slice(nl + 1)), body: out.slice(0, nl) };
    } catch (e) {
      return { status: 0, body: String(e.stderr ?? e) };
    }
  };
  let attempt = 0;
  const resp = await waitFor(
    "a 200 from the public URL",
    async () => {
      const r = await send();
      attempt++;
      if (r.status !== 200) log(`curl attempt ${attempt}: HTTP ${r.status} ${r.body.replace(/\s+/g, " ").slice(0, 140)}`);
      if (r.status === 429) await sleep(5000);
      return r.status === 200 && r.body.includes(marker) ? r : null;
    },
    120000,
    2500,
  );
  log(`curl -X POST https://${host}/e2e (JSON body) -> ${resp.status} ${resp.body.slice(0, 120)}`);
  evidence.public_response = resp.body;

  // --- 5a. stored in SQLite ---------------------------------------------------------------
  const rows = await waitFor(
    "the request in SQLite",
    async () => {
      const out = execFileSync("sqlite3", [
        "-json",
        join(dataDir, "underlane.db"),
        `SELECT method, path, status, req_headers, CAST(req_body AS TEXT) AS req_body, CAST(resp_body AS TEXT) AS resp_body FROM requests WHERE tunnel_id='${tunnelId}' AND path LIKE '/e2e%' AND duration_ms IS NOT NULL`,
      ]).toString();
      const parsed = out.trim() ? JSON.parse(out) : [];
      return parsed.find((r) => r.req_body?.includes(marker)) ? parsed : null;
    },
    20000,
    500,
  );
  const row = rows.find((r) => r.req_body?.includes(marker));
  evidence.sqlite_row = row;
  assert(row.method === "POST" && row.status === 200, "SQLite has the POST with status 200", { path: row.path });
  assert(row.req_body.includes(marker), "SQLite has the request body", { req_body: row.req_body });
  assert(row.resp_body.includes(marker) && row.resp_body.includes("e2e-echo"), "SQLite has the response body", { resp_body: row.resp_body });
  assert(row.req_headers.includes("[redacted]") && !row.req_headers.includes("e2e-secret-token"), "Authorization header was redacted before storage");

  // --- 5b. visible in the dashboard's request viewer --------------------------------------
  const ui = await ctl("GET", `/ui?view=${encodeURIComponent(`requests:${tunnelId}`)}&select=first`);
  const text = ui.body.text ?? "";
  writeFileSync(join(outDir, "request-viewer.txt"), text);
  assert(text.includes("/e2e?source=run.mjs"), "request viewer lists the request");
  assert(text.includes(`"marker": "${marker}"`), "request viewer shows the (pretty-printed) response body");
  assert(text.includes("Copy as curl"), "request detail pane is open");
  const reqUi = await ctl("GET", `/ui?view=${encodeURIComponent(`requests:${tunnelId}`)}&select=first&tab=req`);
  const reqText = reqUi.body.text ?? "";
  writeFileSync(join(outDir, "request-viewer-request-tab.txt"), reqText);
  assert(reqText.includes(`"marker": "${marker}"`) && reqText.includes(`"hello": "underlane"`), "request viewer shows the (pretty-printed) request body");
  assert(reqText.includes("[redacted]") && !reqText.includes("e2e-secret-token"), "request viewer shows the Authorization header redacted");

  const logsUi = await ctl("GET", `/ui?view=${encodeURIComponent(`logs:${tunnelId}`)}`);
  writeFileSync(join(outDir, "log-viewer.txt"), logsUi.body.text ?? "");
  assert(/Registered tunnel connection|Starting metrics server/.test(logsUi.body.text ?? ""), "log viewer shows live cloudflared logs");

  // --- 6. stop: count back to 0 -----------------------------------------------------------
  await ctl("POST", `/stop?id=${tunnelId}`);
  s = await waitFor("tunnel stopped", async () => {
    const st = (await ctl("GET", `/state?id=${tunnelId}`)).body;
    return st.status === "stopped" ? st : null;
  }, 20000, 500);
  ({ title } = await count());
  assert(title === "" && s.active_count === 0, "menu bar count is back to 0 after stopping", { menu_bar_title: title });

  const leftovers = execFileSync("ps", ["-axww", "-o", "command="]).toString().split("\n").filter((l) => l.includes(dataDir) && l.includes("cloudflared"));
  assert(leftovers.length === 0, "no cloudflared process left running");
  log("E2E PASSED");
} catch (e) {
  exitCode = 1;
  console.error(e);
  console.error("--- app log (tail) ---\n" + appLog.split("\n").slice(-40).join("\n"));
} finally {
  try {
    await ctl("POST", "/quit");
  } catch {}
  await sleep(1500);
  if (child.exitCode == null) child.kill("SIGTERM");
  echo.close();
  evidence.passed = exitCode === 0;
  evidence.finished_at = new Date().toISOString();
  writeFileSync(join(outDir, "evidence.json"), JSON.stringify(evidence, null, 2));
  console.log(`evidence written to ${join(outDir, "evidence.json")}`);
  process.exit(exitCode);
}
