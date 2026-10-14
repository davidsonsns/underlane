#!/usr/bin/env node
// Local P3 regression: real app, synthetic diagnostics, no account or origin requests.
import { readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import {
  fakeFixture,
  regressionSession,
  sleep,
} from "../scripts/lib/harness.mjs";

const app = resolve(
  process.argv[2] ?? "target/debug/bundle/macos/Underlane.app"
);
const { root, data, mode } = fakeFixture("underlane-explainers-", "light");
const port = 47_500 + Math.floor(Math.random() * 500);
let id, quickId;
const { ctl, wait, check, fail, finish } = regressionSession(
  join(app, "Contents/MacOS/Underlane"),
  data,
  port,
  root
);
const state = () => ctl("GET", `/state?id=${id}`);
const ui = () => ctl("GET", `/ui?view=tunnels:${id}`);
async function start(fixture) {
  await ctl("POST", `/stop?id=${id}`);
  writeFileSync(mode, fixture);
  await ctl("POST", `/start?id=${id}`);
}
try {
  await wait("health", async () => (await ctl("GET", "/health")).ok);
  ({ id } = await ctl(
    "POST",
    "/quick?name=P3%20regression&origin=http%3A%2F%2Flocalhost%3A43215"
  ));
  quickId = id;
  await start("runtime-known");
  const known = await wait("known failure", async () => {
    const s = await state();
    return s.status === "crashed" && s.failure ? s : null;
  });
  check(
    known.failure.cause.includes("credentials") && known.failure.action,
    "known runtime failure shows cause and next action"
  );
  check(
    known.failure.raw.includes("authentication failed") &&
      !JSON.stringify(known).includes("invented-"),
    "complete runtime diagnostic is redacted"
  );
  check(
    known.supervised && known.detail.includes("Restarting"),
    "crash retry and backoff remain supervised"
  );
  const visible = (await ui()).text;
  check(
    visible.includes(known.failure.cause) &&
      visible.includes("Raw diagnostic") &&
      visible.includes("Try again now"),
    "failure explanation and retry are visible"
  );
  await start("runtime-burst");
  const burst = await wait("burst failure", async () => {
    const s = await state();
    return s.status === "crashed" && s.failure ? s : null;
  });
  check(
    burst.failure.cause.includes("credentials") &&
      burst.failure.raw.includes("authentication failed") &&
      burst.failure.raw.includes("synthetic exit context 39"),
    "exit keeps the observed error behind a burst of info logs"
  );
  await start("runtime-unknown");
  const unknown = await wait("unknown failure", async () => {
    const s = await state();
    return s.status === "crashed" && s.failure ? s : null;
  });
  check(
    unknown.failure.cause.includes("unrecognized") &&
      unknown.failure.raw.includes("mystery failure"),
    "malformed unknown output keeps safe raw without diagnosis"
  );
  check(
    !JSON.stringify(unknown).includes("invented-"),
    "malformed raw diagnostic hides URL and token secrets"
  );
  await start("runtime-warning");
  await wait(
    "connected despite warning",
    async () => (await state()).status === "connected"
  );
  check(
    !((await state()).failure || (await ui()).text.includes("Raw diagnostic")),
    "warnings and successful output do not create failure notices"
  );
  await start("runtime-recover");
  const waiting = await wait("edge failure", async () => {
    const s = await state();
    return s.failure && s.pid ? s : null;
  });
  check(
    waiting.failure.cause.includes("edge") && waiting.status !== "crashed",
    "error logs preserve the living connector state"
  );
  writeFileSync(mode, "ok");
  const recovered = await wait("edge recovery", async () => {
    const s = await state();
    return s.status === "connected" && !s.failure ? s : null;
  });
  check(
    recovered.pid === waiting.pid,
    "healthy edge clears obsolete explanation without replacing child"
  );
  check(
    !(await ui()).text.includes("Raw diagnostic"),
    "recovered UI clears diagnostic disclosure"
  );
  await start("runtime-origin");
  await wait("ready edge and origin failure", async () => {
    const s = await state();
    return s.status === "connected" && s.failure ? s : null;
  });
  await sleep(2300);
  const origin = await state();
  check(
    origin.status === "connected" && origin.failure?.cause.includes("origin"),
    "edge readiness does not claim origin recovery"
  );
  check(
    !JSON.stringify(origin).includes("invented-"),
    "origin URL credentials and query secrets remain redacted"
  );
  await ctl("POST", `/stop?id=${id}`);
  check(
    !(await state()).failure,
    "Stop clears the current failure explanation"
  );
  writeFileSync(mode, "ok");
  const config = join(root, "invalid.yml");
  const source =
    "ingress:\n - hostname: invalid.example.invalid\n   service: http://localhost:43216\n";
  writeFileSync(config, source);
  await ctl("POST", `/import?path=${encodeURIComponent(config)}`);
  ({ id } = (await ctl("GET", "/overview")).tunnels.find(
    (t) => t.config_path === config
  ));
  await start("invalid-config");
  const prep = await wait("preparation failure", async () => {
    const s = await state();
    return s.status === "crashed" && s.failure ? s : null;
  });
  check(
    prep.failure.cause.includes("config") &&
      prep.failure.action?.includes("http_status:404"),
    "preparation failure explains config cause and fix"
  );
  check(
    readFileSync(config, "utf8") === source,
    "failure handling never edits imported YAML"
  );
  const logs = await ctl("GET", `/logs?id=${quickId}`);
  check(
    logs.some((line) => line.message.includes("mystery failure")) &&
      !JSON.stringify(logs).includes("invented-"),
    "persisted runtime logs contain diagnostics without synthetic secrets"
  );
  console.log("FAILURE EXPLAINERS E2E PASSED");
} catch (error) {
  fail(error);
} finally {
  await finish(id, "e2e/out/failure-explainers-evidence.json");
}
