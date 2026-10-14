#!/usr/bin/env node
// Local P1 regression: a real app with a disposable config and the shared fake connector.
// Run after building with --features e2e: node e2e/config-badge.mjs target/debug/bundle/macos/Underlane.app
import { readFileSync, renameSync, unlinkSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { fakeFixture, regressionSession } from "../scripts/lib/harness.mjs";

const app = resolve(
  process.argv[2] ?? "target/debug/bundle/macos/Underlane.app"
);
const { root, data } = fakeFixture("underlane-config-badge-", "light");
const config = join(root, "config.yml");
const source = (originPort) =>
  `tunnel: synthetic-p1\ningress:\n  - hostname: badge.example.invalid\n    service: http://localhost:${originPort}\n  - service: http_status:404\n`;
const a = source(43_210),
  b = source(43_211);
writeFileSync(config, a);
const port = 48_500 + Math.floor(Math.random() * 500);
const { ctl, wait, check, fail, finish } = regressionSession(
  join(app, "Contents/MacOS/Underlane"),
  data,
  port,
  root
);
let id;
function state() {
  return ctl("GET", `/state?id=${id}`);
}
function comparison() {
  return ctl("GET", `/config-change?id=${id}`);
}
async function text() {
  // Remount the detail: equivalent to revisiting it after an external editor change.
  await ctl("GET", "/ui?view=settings");
  return (await ctl("GET", `/ui?view=${encodeURIComponent(`tunnels:${id}`)}`))
    .text;
}
try {
  await wait("health", async () => (await ctl("GET", "/health")).ok);
  const imported = await ctl(
    "POST",
    `/import?path=${encodeURIComponent(config)}`
  );
  check(imported.added.length === 1, "disposable config imported");
  ({ id } = (await ctl("GET", "/overview")).tunnels.find(
    (tunnel) => tunnel.config_path === config
  ));
  await ctl("POST", `/start?id=${id}`);
  const started = await wait("fake connector", async () => {
    const s = await state();
    return s.status === "connected" ? s : null;
  });
  check(
    (await comparison()).status === "unchanged",
    "initial source matches loaded config"
  );
  check(
    !(await text()).includes("Config changed on disk"),
    "unchanged config has no badge"
  );
  const derived = join(data, "derived", `${id}.yml`);
  const runningA = readFileSync(derived, "utf8");
  writeFileSync(config, b);
  check(
    (await comparison()).status === "changed",
    "external edit detected without restart"
  );
  check(
    (await state()).pid === started.pid &&
      readFileSync(derived, "utf8") === runningA,
    "existing child and derived config stay on A"
  );
  const changed = await text();
  check(
    changed.includes("Config changed on disk") &&
      changed.includes("Restart to apply your changes."),
    "visible badge explains restart"
  );
  writeFileSync(config, a);
  check(
    (await comparison()).status === "unchanged",
    "restored bytes clear the mismatch"
  );
  check(
    !(await text()).includes("Config changed on disk"),
    "restored bytes clear the visible badge"
  );
  writeFileSync(config, `${a}# external comment\n`);
  check(
    (await comparison()).status === "changed",
    "comment-only edit is detected"
  );
  const replacement = join(root, "replacement.yml");
  writeFileSync(replacement, b);
  renameSync(replacement, config);
  check(
    (await comparison()).status === "changed",
    "atomic file replacement is detected"
  );
  await ctl("POST", `/restart?id=${id}`);
  const restarted = await wait("loaded B", async () => {
    const s = await state();
    return s.status === "connected" &&
      s.config_revision > started.config_revision
      ? s
      : null;
  });
  check(
    (await comparison()).status === "unchanged" &&
      readFileSync(derived, "utf8").includes("43211"),
    "successful restart loads B and clears mismatch"
  );
  check(
    !(await text()).includes("Config changed on disk"),
    "successful restart clears visible badge"
  );
  check(
    readFileSync(config, "utf8") === b,
    "app never rewrites the original YAML"
  );
  unlinkSync(config);
  check(
    (await comparison()).status === "unreadable",
    "missing file is unreadable, not unchanged"
  );
  const unreadable = await text();
  check(
    unreadable.includes("Could not read the config on disk"),
    "missing-file warning is visible"
  );
  check(
    (await state()).pid === restarted.pid,
    "missing source does not stop the live child"
  );
  writeFileSync(
    config,
    "tunnel: synthetic-p1\ningress:\n - hostname: broken.invalid\n"
  );
  await ctl("POST", `/restart?id=${id}`);
  const failed = await wait("failed restart", async () => {
    const s = await state();
    return s.status === "crashed" ? s : null;
  });
  check(
    failed.config_revision === restarted.config_revision &&
      (await comparison()).status === "changed",
    "failed restart never marks changed bytes applied"
  );
  writeFileSync(config, b);
  await ctl("POST", `/restart?id=${id}`);
  await wait("recovery", async () => (await state()).status === "connected");
  await ctl("POST", `/stop?id=${id}`);
  check(
    (await comparison()).status === "not_running",
    "stop clears the running comparison"
  );
  check(
    !(await text()).includes("Config changed on disk"),
    "stopped tunnel has no badge"
  );
  check(
    readFileSync(config, "utf8") === b,
    "source bytes remain untouched after stop"
  );
  console.log("CONFIG BADGE E2E PASSED");
} catch (error) {
  fail(error);
} finally {
  await finish(id, "e2e/out/config-badge-evidence.json");
}
