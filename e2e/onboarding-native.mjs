// Verify first-run/persistence against the packaged app, without real tunnels or authorization.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { rmSync } from "node:fs";
import { join, resolve } from "node:path";
import {
  createControl,
  fakeFixture,
  launchApp,
  stopApp,
  waitFor,
} from "../scripts/lib/harness.mjs";

const app = resolve(
  process.argv[2] ?? "target/debug/bundle/macos/Underlane.app"
);
const bin = join(app, "Contents/MacOS/Underlane");
const fixture = fakeFixture("underlane-onboarding-native-");
const port = 49_000 + Math.floor(Math.random() * 900);
const { ctl } = createControl(port, { strict: true });
let child;

async function launch() {
  ({ child } = launchApp(bin, fixture.data, port));
  await waitFor(
    "native app",
    async () => (await ctl("GET", "/health")).ok,
    30_000,
    200
  );
}

try {
  await launch();
  const welcome = await ctl("GET", "/ui?view=tunnels");
  assert.match(welcome.text, /Your localhost,[\s\S]*one link away/);
  assert.match(welcome.text, /Get started/);
  assert.equal(welcome.resizable, false);
  assert.equal((await ctl("POST", "/complete-onboarding")).ok, true);
  assert.equal((await ctl("GET", "/ui?view=tunnels")).resizable, true);
  await stopApp(child, ctl);

  await launch();
  const dashboard = await ctl("GET", "/ui?view=tunnels");
  assert.doesNotMatch(dashboard.text, /Get started/);
  assert.match(dashboard.text, /Projects/);
  assert.equal(dashboard.resizable, true);
  await ctl("POST", "/quick?origin=http%3A%2F%2Flocalhost%3A3000&name=Legacy");
  await stopApp(child, ctl);

  execFileSync("sqlite3", [
    join(fixture.data, "underlane.db"),
    "DELETE FROM settings WHERE key = 'onboarding_complete';",
  ]);
  await launch();
  const legacy = await ctl("GET", "/ui?view=tunnels");
  assert.match(legacy.text, /Legacy/);
  assert.doesNotMatch(legacy.text, /Get started/);
  assert.equal(legacy.resizable, true);
  await stopApp(child, ctl);
  const persisted = execFileSync(
    "sqlite3",
    [
      join(fixture.data, "underlane.db"),
      "SELECT value FROM settings WHERE key = 'onboarding_complete';",
    ],
    { encoding: "utf8" }
  );
  assert.equal(persisted.trim(), "1");
  await launch();
  const beforeReset = await ctl("GET", "/overview");
  const tunnelId = beforeReset.tunnels[0].id;
  await ctl("POST", `/start?id=${tunnelId}`);
  await waitFor(
    "synthetic tunnel process",
    async () => {
      const overview = await ctl("GET", "/overview");
      return typeof overview.tunnels[0]?.state.pid === "number";
    },
    10_000,
    100
  );
  const running = await ctl("GET", "/overview");
  const tunnelPid = running.tunnels[0].state.pid;
  const processId = child.pid;
  const resetResponse = await ctl("POST", "/reset");
  assert.equal(resetResponse.ok, true);
  const reset = await ctl("GET", "/ui?view=tunnels");
  assert.match(reset.text, /Your localhost,[\s\S]*one link away/);
  assert.doesNotMatch(reset.text, /Legacy/);
  assert.equal(reset.resizable, false);
  assert.equal(child.pid, processId);
  assert.equal(
    child.exitCode,
    null,
    "reset must keep the original app process alive"
  );
  assert.equal((await ctl("GET", "/overview")).active_count, 0);
  assert.throws(() => process.kill(tunnelPid, 0), { code: "ESRCH" });
  const remaining = execFileSync(
    "sqlite3",
    [join(fixture.data, "underlane.db"), "SELECT count(*) FROM tunnels;"],
    { encoding: "utf8" }
  );
  assert.equal(remaining.trim(), "0");
  await stopApp(child, ctl);
  console.log(
    "ONBOARDING NATIVE PASSED: packaged welcome, remembered completion, legacy migration, populated in-process reset to onboarding with unchanged PID. No real tunnel or Cloudflare login."
  );
} finally {
  if (child?.exitCode === null) {
    await stopApp(child, ctl);
  }
  rmSync(fixture.root, { force: true, recursive: true });
}
