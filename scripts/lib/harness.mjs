import { execFileSync, spawn } from "node:child_process";
import { Resolver } from "node:dns/promises";
import {
  chmodSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  writeFileSync,
} from "node:fs";
import http from "node:http";
import https from "node:https";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { setTimeout as delay } from "node:timers/promises";

export const sleep = (milliseconds) => delay(milliseconds);

export function createControl(
  port,
  { timeout = 30_000, strict = false, envelope = false } = {}
) {
  const request = (method, path, body) =>
    new Promise((done, reject) => {
      const payload = body === undefined ? undefined : JSON.stringify(body);
      const req = http.request(
        {
          headers:
            payload === undefined
              ? {}
              : {
                  "content-length": Buffer.byteLength(payload),
                  "content-type": "application/json",
                },
          host: "127.0.0.1",
          method,
          path,
          port,
          timeout,
        },
        (response) => {
          let text = "";
          response.on("data", (chunk) => {
            text += chunk;
          });
          response.on("end", () => {
            let value;
            try {
              value = JSON.parse(text);
            } catch (error) {
              if (strict) {
                reject(error);
                return;
              }
              value = text;
            }
            done({ status: response.statusCode, value });
          });
          response.on("error", reject);
        }
      );
      req.on("error", reject);
      req.on("timeout", () => req.destroy(new Error("control timeout")));
      req.end(payload);
    });
  const ctl = async (method, path, body) => {
    const result = await request(method, path, body);
    if (strict && result.status !== 200) {
      throw new Error(JSON.stringify(result));
    }
    return envelope
      ? { body: result.value, status: result.status }
      : result.value;
  };
  return { ctl, request };
}

export async function waitFor(what, fn, timeoutMs = 90_000, everyMs = 1000) {
  const end = Date.now() + timeoutMs;
  let last;
  while (Date.now() < end) {
    try {
      // biome-ignore lint/performance/noAwaitInLoops: Each poll depends on the previous result.
      last = await fn();
      if (last) {
        return last;
      }
    } catch (error) {
      last = error;
    }
    await sleep(everyMs);
  }
  throw new Error(
    `timed out waiting for ${what} (last: ${JSON.stringify(last)})`
  );
}

export function fakeFixture(prefix, theme = "light") {
  const root = mkdtempSync(join(tmpdir(), prefix));
  const data = join(root, "data");
  const bindir = join(root, "bin");
  mkdirSync(data);
  mkdirSync(bindir);
  const fake = join(bindir, "cloudflared");
  copyFileSync(
    resolve("crates/underlane-core/tests/common/fake_cloudflared.py"),
    fake
  );
  chmodSync(fake, 0o755);
  const mode = join(bindir, "mode");
  writeFileSync(mode, "ok");
  const settings = JSON.stringify({ cloudflared_path: fake, theme }).replaceAll(
    "'",
    "''"
  );
  execFileSync("sqlite3", [
    join(data, "underlane.db"),
    `CREATE TABLE settings (key TEXT PRIMARY KEY,value TEXT NOT NULL); INSERT INTO settings VALUES ('settings','${settings}');`,
  ]);
  return { bindir, data, fake, mode, root };
}
export function launchApp(executable, data, port, options = {}) {
  const child = spawn(executable, [], {
    env: {
      ...process.env,
      UNDERLANE_DATA_DIR: data,
      UNDERLANE_E2E_PORT: String(port),
      ...options.env,
    },
    stdio: options.stdio ?? ["ignore", "pipe", "pipe"],
  });
  let log = "";
  const append = (chunk) => {
    log += chunk;
  };
  child.stdout?.on("data", append);
  child.stderr?.on("data", append);
  return { appLog: () => log, child };
}
export function evidenceCheck(evidence) {
  return (condition, message) => {
    if (!condition) {
      throw new Error(message);
    }
    console.log(`ok: ${message}`);
    evidence.steps.push(message);
  };
}
export async function stopApp(child, ctl, id) {
  const bestEffort = (promise) =>
    promise.catch(() => {
      /* The app may already have exited. */
    });
  if (id) {
    await bestEffort(ctl("POST", `/stop?id=${id}`));
  }
  await bestEffort(ctl("POST", "/quit"));
  if (child.exitCode !== null) {
    return;
  }
  await Promise.race([
    new Promise((done) => child.once("exit", done)),
    sleep(1500),
  ]);
  if (child.exitCode === null) {
    child.kill("SIGTERM");
  }
}
export function createNotes() {
  const notes = [];
  const note = (message) => {
    console.log(message);
    notes.push(message);
  };
  return { note, notes };
}
function echoServer(name, timestamp = false) {
  const server = http.createServer((req, res) => {
    let body = "";
    req.on("data", (chunk) => {
      body += chunk;
    });
    req.on("end", () => {
      let received = null;
      try {
        received = body ? JSON.parse(body) : null;
      } catch {
        /* Non-JSON traffic is valid for this echo fixture. */
      }
      const json = JSON.stringify({
        method: req.method,
        path: req.url,
        received,
        service: name,
        ...(timestamp ? { at: new Date().toISOString() } : {}),
      });
      res.writeHead(200, { "content-type": "application/json" });
      res.end(json);
    });
  });
  return new Promise((done) =>
    server.listen(0, "127.0.0.1", () => done(server))
  );
}
function httpsOk(url, method, body, ip) {
  return new Promise((done) => {
    const payload = body ? JSON.stringify(body) : undefined;
    const req = https.request(
      {
        headers: payload
          ? {
              "content-length": Buffer.byteLength(payload),
              "content-type": "application/json",
            }
          : {},
        host: url.hostname,
        lookup: (_host, options, callback) =>
          options?.all
            ? callback(null, [{ address: ip, family: 4 }])
            : callback(null, ip, 4),
        method,
        path: url.pathname + url.search,
        servername: url.hostname,
      },
      (response) => {
        response.resume();
        response.on("end", () => done(response.statusCode === 200));
      }
    );
    req.on("error", () => done(false));
    req.end(payload);
  });
}
export async function hit(
  address,
  method,
  body,
  dnsServers = ["1.1.1.1", "1.0.0.1"]
) {
  const url = new URL(address),
    resolver = new Resolver();
  resolver.setServers(dnsServers);
  const ip = await waitFor(
    `dns ${url.hostname}`,
    async () => (await resolver.resolve4(url.hostname))[0],
    90_000,
    2000
  );
  return waitFor(
    `200 from ${address}`,
    () => httpsOk(url, method, body, ip),
    90_000,
    2500
  );
}
export function captureWindow(file, minW, maxW, pid) {
  const args = [
    "scripts/capture-window.swift",
    "Underlane",
    file,
    String(minW),
    String(maxW),
  ];
  if (pid !== undefined) {
    args.push(String(pid));
  }
  execFileSync("swift", args, { stdio: "inherit" });
  return file;
}
function tryCapture(file, minW, maxW, pid) {
  try {
    captureWindow(file, minW, maxW, pid);
    return existsSync(file);
  } catch {
    return false;
  }
}
export async function captureWithRetry(file, minW, maxW, pid) {
  for (let attempt = 0; attempt < 4; attempt += 1) {
    if (tryCapture(file, minW, maxW, pid)) {
      return file;
    }
    if (attempt < 3) {
      // biome-ignore lint/performance/noAwaitInLoops: Retry only after the window's animation advances.
      await sleep(800);
    }
  }
  throw new Error(`could not capture ${file}`);
}

export function regressionSession(executable, data, port, root) {
  const { child, appLog } = launchApp(executable, data, port);
  const evidence = { root, steps: [] };
  const check = evidenceCheck(evidence);
  const { ctl } = createControl(port, { strict: true, timeout: 12_000 });
  const wait = (message, fn) => waitFor(message, fn, 10_000, 100);
  const fail = (error) => {
    evidence.error = String(error);
    console.error(error, appLog());
    process.exitCode = 1;
  };
  const finish = async (id, file) => {
    await stopApp(child, ctl, id);
    mkdirSync("e2e/out", { recursive: true });
    writeFileSync(file, JSON.stringify(evidence, null, 2));
  };
  return { check, ctl, fail, finish, wait };
}
export async function seedQuickTunnels(
  ctl,
  servers,
  specs,
  { start = false, timestamp = false } = {}
) {
  const ids = [];
  for (const [name, inspect] of specs) {
    // biome-ignore lint/performance/noAwaitInLoops: Register each local server before proceeding to the next tunnel.
    const server = await echoServer(name, timestamp);
    servers.push(server);
    const result = await ctl(
      "POST",
      `/quick?origin=${encodeURIComponent(`http://127.0.0.1:${server.address().port}`)}&name=${name}&inspect=${inspect ? 1 : 0}`
    );
    ids.push(result.id);
    if (start) {
      await ctl("POST", `/start?id=${result.id}`);
    }
  }
  return ids;
}
export function connectedStates(ctl, ids) {
  return waitFor(
    "all tunnels connected",
    async () => {
      const states = await Promise.all(
        ids.map((id) => ctl("GET", `/state?id=${id}`))
      );
      return states.every(
        (state) => state.status === "connected" && state.public_url
      )
        ? states
        : null;
    },
    120_000
  );
}
export async function finishShots(child, ctl, servers, file, notes) {
  try {
    await ctl("POST", "/quit");
  } catch {
    /* The app may already be closed. */
  }
  await sleep(2000);
  if (child.exitCode === null) {
    child.kill("SIGTERM");
  }
  for (const server of servers) {
    server.close();
  }
  writeFileSync(file, `${notes.join("\n")}\n`);
}
