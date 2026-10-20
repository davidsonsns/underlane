// Native WebKit -> row button -> Tauri IPC -> supervisor -> state events -> rendered status.
// Uses a disposable database and fake connector; never opens a real tunnel.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { randomUUID } from "node:crypto";
import { readFileSync, rmSync, writeFileSync } from "node:fs";
import http from "node:http";
import { join, resolve } from "node:path";
import {
  createControl,
  fakeFixture,
  launchApp,
  stopApp,
  waitFor,
} from "../scripts/lib/harness.mjs";

const bin = resolve(
  process.argv[2] ??
    "target/debug/bundle/macos/Underlane.app/Contents/MacOS/Underlane"
);
const fixture = fakeFixture("underlane-hostname-native-", "dark");
const port = 49_000 + Math.floor(Math.random() * 900);
const { ctl } = createControl(port, { strict: true });
const server = http.createServer((_req, res) => res.end("origin alive"));
await new Promise((done) => server.listen(0, "127.0.0.1", done));
const source = join(fixture.root, "Native hostnames.yml");
const hosts = ["a", "b", "c", "d", "e"].map((name) => `${name}.native.invalid`);
const GATE_URL = /service:\s*['"]?(http:\/\/127\.0\.0\.1:\d+)/g;
const original = `tunnel: ${randomUUID()}\ningress:\n${hosts.map((host) => `  - hostname: ${host}\n    service: http://127.0.0.1:${server.address().port}\n`).join("")}  - service: http_status:404\n`;
writeFileSync(source, original);
let child;
let appLog;
let id;
let controlOwned = false;
const poll = (what, fn) => waitFor(what, fn, 15_000, 100);
async function launch() {
  controlOwned = false;
  ({ child, appLog } = launchApp(bin, fixture.data, port));
  await poll("native app", async () => (await ctl("GET", "/health")).ok);
  assert.equal(
    resolve((await ctl("GET", "/overview")).data_dir),
    resolve(fixture.data)
  );
  controlOwned = true;
}
async function state() {
  return (await ctl("GET", "/overview")).tunnels.find(
    (tunnel) => tunnel.id === id
  ).state;
}
async function connection() {
  return (await ctl("GET", "/overview")).tunnels.find(
    (tunnel) => tunnel.id === id
  ).connection;
}

async function expectHealth(expected) {
  const current = await connection();
  for (const host of expected) {
    const route = current?.routes.find(
      (candidate) => candidate.hostname === host
    );
    assert.equal(
      route?.local.status,
      "reachable",
      `cached health survives for ${host}`
    );
  }
  const rendered = await ui();
  for (const row of rendered.rows.filter((candidate) =>
    expected.includes(candidate.hostname)
  )) {
    assert.ok(
      row.dots.some((dot) => dot.label?.includes("Local service reachable")),
      `${row.hostname} must keep its known local health dot after a sibling toggles`
    );
  }
}
async function ui(action, hostname) {
  const params = new URLSearchParams({ inspect: "1", view: `tunnels:${id}` });
  if (action) {
    params.set("action", action);
    params.set("hostname", hostname);
  }
  const rendered = JSON.parse((await ctl("GET", `/ui?${params}`)).text);
  assert.equal(rendered.error, undefined, JSON.stringify(rendered));
  return rendered;
}
function enabled(current) {
  return current.mappings
    .filter((route) => route.hostname && route.enabled)
    .map((route) => route.hostname);
}
async function expectRunning(expected, pid) {
  const current = await poll(`running ${expected.join(",")}`, async () => {
    const next = await state();
    return next.status === "connected" &&
      JSON.stringify(enabled(next)) === JSON.stringify(expected)
      ? next
      : null;
  });
  if (pid) {
    assert.equal(
      current.pid,
      pid,
      "unaffected hostnames keep the original connector"
    );
  }
  const derived = readFileSync(
    join(fixture.data, "derived", `${id}.yml`),
    "utf8"
  );
  const gates = [...derived.matchAll(GATE_URL)].map((match) => match[1]);
  assert.equal(gates.length, hosts.length);
  await Promise.all(
    gates.map(async (gate, index) => {
      const response = await fetch(gate);
      assert.equal(
        response.status,
        expected.includes(hosts[index]) ? 200 : 503,
        `forwarding gate for ${hosts[index]}`
      );
      await response.text();
    })
  );
  return current;
}
async function expectStopped() {
  const current = await poll("no connector and stopped project", async () => {
    const next = await state();
    return next.status === "stopped" &&
      !next.supervised &&
      !next.pid &&
      enabled(next).length === 0
      ? next
      : null;
  });
  assert.equal((await ctl("GET", "/overview")).active_count, 0);
  const rendered = await ui();
  assert.equal(
    rendered.state.status,
    "stopped",
    "event-driven dashboard agrees with backend"
  );
  assert.equal(rendered.sidebarStatus, "Stopped");
  assert.equal(rendered.headerStatus, "Stopped");
  for (const row of rendered.rows) {
    assert.ok(row.actions.includes(`Start ${row.hostname}`));
    assert.ok(
      row.dots.some((dot) => dot.visible),
      "localhost status dot remains visible"
    );
  }
  assert.equal(rendered.text.includes("DNS target unverified"), false);
  return current;
}

try {
  await launch();
  await ctl("POST", `/import?path=${encodeURIComponent(source)}`);
  ({ id } = (await ctl("GET", "/overview")).tunnels.find(
    (tunnel) => tunnel.config_path === source
  ));
  await ctl("GET", "/ui?view=tunnels");
  await ctl("POST", "/complete-onboarding");
  const initial = await ui();
  assert.equal(initial.state.status, "stopped");
  await ui("start", hosts[0]);
  const first = await expectRunning([hosts[0]]);
  await ui("start", hosts[2]);
  await expectRunning([hosts[0], hosts[2]], first.pid);
  await ui("stop", hosts[0]);
  await expectRunning([hosts[2]], first.pid);
  await ui("stop", hosts[2]);
  await expectStopped();
  assert.throws(() => process.kill(first.pid, 0), { code: "ESRCH" });
  console.log(
    "PASS: individual row starts, unaffected PID, final row Stop clears native header/sidebar"
  );

  await ctl("POST", `/start?id=${id}`);
  const all = await expectRunning(hosts);
  const localStartedAt = performance.now();
  await waitFor(
    "local health before DNS/public checks finish",
    async () => {
      const current = await connection();
      return (
        current?.routes.length === hosts.length &&
        current.routes.every((route) => route.local.status === "reachable")
      );
    },
    1500,
    25
  );
  console.log(
    `Local health available within ${Math.round(performance.now() - localStartedAt)} ms of connector readiness`
  );
  for (const [index, host] of hosts.entries()) {
    // biome-ignore lint/performance/noAwaitInLoops: Verify each row's state before stopping the next.
    await ui("stop", host);
    if (index < hosts.length - 1) {
      await expectRunning(hosts.slice(index + 1), all.pid);
      await expectHealth(hosts.slice(index + 1));
    }
  }
  await expectStopped();
  assert.equal(
    readFileSync(source, "utf8"),
    original,
    "row controls never edit project YAML"
  );
  console.log(
    "PASS: whole-project Start then five individual Stops; source YAML unchanged"
  );

  await ctl("POST", `/tunnel?id=${id}&autostart=1`);
  await stopApp(child, ctl);
  await launch();
  await expectStopped();
  console.log(
    "PASS: autostart does not revive an empty hostname selection after app relaunch"
  );

  await ui("start", hosts[0]);
  await expectRunning([hosts[0]]);
  await stopApp(child, ctl);
  writeFileSync(
    source,
    original.replaceAll("a.native.invalid", "renamed.native.invalid")
  );
  await launch();
  await expectStopped();
  console.log(
    "PASS: stale saved hostname selection cannot create a green connector with zero selected current hostnames"
  );
  const selections = execFileSync(
    "sqlite3",
    [join(fixture.data, "underlane.db"), "SELECT count(*) FROM tunnels;"],
    { encoding: "utf8" }
  );
  assert.equal(selections.trim(), "1");
} catch (error) {
  console.error(appLog?.());
  throw error;
} finally {
  if (child?.exitCode === null) {
    if (controlOwned) {
      await stopApp(child, ctl, id);
    } else {
      child.kill("SIGTERM");
    }
  }
  server.closeAllConnections();
  await new Promise((done) => server.close(done));
  rmSync(fixture.root, { force: true, recursive: true });
}
