#!/usr/bin/env node
// Real native save path, disposable database/config, and the shared fake connector.
// Run: node e2e/hostnames.mjs target/debug/bundle/macos/Underlane.app
import { execFileSync } from "node:child_process";
import { mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import {
  createControl,
  evidenceCheck,
  fakeFixture,
  launchApp,
  sleep,
  stopApp,
  waitFor,
} from "../scripts/lib/harness.mjs";

const YAML_EXTENSION = /\.ya?ml$/i;
const app = resolve(
  process.argv[2] ?? "target/debug/bundle/macos/Underlane.app"
);
const executable = app.endsWith(".app")
  ? join(app, "Contents/MacOS/Underlane")
  : app;
const { root, data, bindir, fake, mode } = fakeFixture(
  "underlane-hostnames-",
  "dark"
);
// Count actual invocations in this disposable copy without changing the shared fixture.
writeFileSync(
  fake,
  readFileSync(fake, "utf8").replace(
    "args = sys.argv[1:]",
    'args = sys.argv[1:]\nwith open(os.path.join(here, "invocations.jsonl"), "a") as f:\n    f.write(json.dumps(args) + "\\n")'
  )
);
const config = join(root, "config.yml");
const original = `# User formatting and comments survive no-op; backups preserve these exact bytes.
tunnel: synthetic-hostnames
loglevel: warn
future-option:
  nested: retain-me
ingress:
  - hostname: alpha.example.invalid
    service: http://localhost:43210
    originRequest:
      connectTimeout: 17s
    future-rule: keep-rule
  - path: ^/assets/.*
    service: http://localhost:43212
    future-rule: keep-path-only
  - hostname: remove.example.invalid
    service: http://localhost:43213
  - hostname: api.example.invalid
    path: ^/v1/.*
    service: http://localhost:43214
    originRequest:
      noTLSVerify: true
  - service: http_status:403
    future-rule: keep-fallback
`;
writeFileSync(config, original);
const port = 49_200 + Math.floor(Math.random() * 500);
const { child, appLog } = launchApp(executable, data, port);
const evidence = { executable, pid: child.pid, port, root, steps: [] };
console.log(
  `Isolated native app PID ${child.pid}; control port ${port}; fixture ${root}`
);
const check = evidenceCheck(evidence);
const { ctl, request } = createControl(port, { strict: true, timeout: 12_000 });
const wait = (message, fn) => waitFor(message, fn, 15_000, 100);
const backups = () =>
  readdirSync(root).filter((name) => name.includes("underlane-backup"));
const pending = () =>
  readdirSync(root).filter((name) => name.includes("underlane-pending"));
const runCount = () =>
  readFileSync(join(bindir, "invocations.jsonl"), "utf8")
    .trim()
    .split("\n")
    .map(JSON.parse)
    .filter((args) => args.includes("run")).length;
let id;
const state = () => ctl("GET", `/state?id=${id}`);
const load = () => ctl("GET", `/hostnames?id=${id}`);
const save = (draft) => ctl("POST", `/hostnames?id=${id}`, draft);
const view = async () =>
  (await ctl("GET", "/overview")).tunnels.find((tunnel) => tunnel.id === id);
async function rejectSave(
  draft,
  message,
  expectedBytes,
  expectedState,
  expectedRuns,
  expectedBackups
) {
  const response = await request("POST", `/hostnames?id=${id}`, draft);
  check(
    response.status === 400 && response.value.error.includes(message),
    `${message} rejects the save`
  );
  const current = await state();
  check(
    readFileSync(config, "utf8") === expectedBytes,
    `${message} preserves source bytes and the running child`
  );
  check(
    current.pid === expectedState.pid,
    `${message} preserves the running child PID`
  );
  check(
    current.run === expectedState.run,
    `${message} preserves the connector run`
  );
  check(backups().length === expectedBackups, `${message} creates no backup`);
  check(pending().length === 0, `${message} leaves no candidate`);
  check(
    runCount() === expectedRuns,
    `${message} creates no restart, backup, or leftover candidate`
  );
}
try {
  await wait("health", async () => (await ctl("GET", "/health")).ok);
  check(
    (await ctl("GET", "/overview")).data_dir === data,
    "app uses only the disposable data directory"
  );
  const imported = await ctl(
    "POST",
    `/import?path=${encodeURIComponent(config)}`
  );
  check(imported.added.length === 1, "disposable source config imported");
  ({ id } = (await ctl("GET", "/overview")).tunnels.find(
    (tunnel) => tunnel.config_path === config
  ));
  await ctl("POST", `/tunnel?id=${id}&inspect=0`);
  await ctl("POST", `/start?id=${id}`);
  const started = await wait("connected", async () => {
    const s = await state();
    return s.status === "connected" ? s : null;
  });
  const starts = runCount();
  let draft = await load();
  check(
    draft.routes.length === 3 &&
      draft.routes[2].path === "^/v1/.*" &&
      draft.fallback_service === "http_status:403",
    "editor exposes hostname rules and preserves path/custom fallback values"
  );
  await save(draft);
  check(
    readFileSync(config, "utf8") === original &&
      backups().length === 0 &&
      runCount() === starts &&
      (await state()).pid === started.pid,
    "no-op keeps exact bytes and makes no backup or restart"
  );

  draft.routes[0].hostname = "updated.example.invalid";
  draft.routes[0].service = "localhost:43215";
  draft.routes.splice(1, 1);
  draft.routes.push({
    hostname: "added.example.invalid",
    index: null,
    path: null,
    service: "43216",
  });
  check(
    (await save(draft)).restart_error === null,
    "running CRUD save completes without restart error"
  );
  const updated = await wait("CRUD restart", async () => {
    const s = await state();
    return s.status === "connected" &&
      s.config_revision > started.config_revision
      ? s
      : null;
  });
  await sleep(500);
  check(
    updated.pid !== started.pid && runCount() === starts + 1,
    "running CRUD save restarts the actual connector exactly once"
  );
  const saved = readFileSync(config, "utf8");
  draft = await load();
  check(
    draft.routes.map((route) => route.hostname).join(",") ===
      "updated.example.invalid,api.example.invalid,added.example.invalid",
    "CRUD updates, removes, and adds hostname rules in order"
  );
  check(
    draft.routes[0].service === "http://localhost:43215" &&
      draft.routes[2].service === "http://localhost:43216",
    "service shorthands normalize before publication"
  );
  check(
    backups().length === 1 &&
      readFileSync(join(root, backups()[0]), "utf8") === original,
    "successful save backs up exact original bytes"
  );
  check(
    backups().every(
      (name) =>
        name.startsWith(".") &&
        name.endsWith(".bak") &&
        !YAML_EXTENSION.test(name)
    ),
    "backups are hidden and excluded from YAML config discovery"
  );
  check(
    [
      "retain-me",
      "connectTimeout: 17s",
      "keep-rule",
      "keep-path-only",
      "noTLSVerify: true",
      "keep-fallback",
    ].every((value) => saved.includes(value)),
    "root options and untouched rule options survive CRUD"
  );
  const { mappings } = (await view()).state;
  check(
    mappings.length === 5 &&
      mappings[0].hostname === "updated.example.invalid" &&
      mappings[1].path === "^/assets/.*" &&
      mappings[2].path === "^/v1/.*" &&
      mappings[3].hostname === "added.example.invalid" &&
      mappings[4].service === "http_status:403",
    "running mappings reflect CRUD while path-only rules, path filters, order, and custom fallback survive"
  );
  check(
    (await ctl("GET", `/config-change?id=${id}`)).status === "unchanged",
    "successful restart marks saved source as applied"
  );

  const external = `${saved}# external editor\n`;
  writeFileSync(config, external);
  draft.routes[0].hostname = "conflict.example.invalid";
  await rejectSave(draft, "config changed", external, updated, starts + 1, 1);
  writeFileSync(config, saved);
  draft = await load();
  draft.routes[0].hostname = "invalid.example.invalid";
  writeFileSync(mode, "invalid-config");
  await rejectSave(
    draft,
    "Could not save hostnames",
    saved,
    updated,
    starts + 1,
    1
  );
  writeFileSync(mode, "ok");

  const quick = await ctl("POST", "/quick?name=boundary&origin=43219");
  execFileSync("sqlite3", [
    join(data, "underlane.db"),
    "INSERT INTO tunnels(id,group_id,name,kind,config_path,origin_url,inspect,autostart,loglevel,created_at,sort) VALUES ('token-boundary',NULL,'Synthetic token','token',NULL,NULL,0,0,'info',0,0);",
  ]);
  for (const boundary of [quick.id, "token-boundary"]) {
    // biome-ignore lint/performance/noAwaitInLoops: Polls and native UI actions must complete in order before the next step.
    const loading = await request("GET", `/hostnames?id=${boundary}`);
    const saving = await request("POST", `/hostnames?id=${boundary}`, draft);
    check(
      loading.status === 400 &&
        saving.status === 400 &&
        loading.value.error.includes("locally managed config") &&
        saving.value.error.includes("locally managed config"),
      `${boundary === quick.id ? "quick" : "token"} tunnel rejects both hostname load and save`
    );
  }
  check(
    readFileSync(config, "utf8") === saved &&
      (await state()).pid === updated.pid,
    "unsupported tunnel boundary tests leave config tunnel intact"
  );

  const shadowConfig = join(root, "shadow.yml");
  const shadowSource =
    "tunnel: synthetic-shadow\ningress:\n  - hostname: '*.example.invalid'\n    service: http://localhost:43220\n  - hostname: outside.other.invalid\n    service: http://localhost:43221\n  - service: http_status:404\n";
  writeFileSync(shadowConfig, shadowSource);
  await ctl("POST", `/import?path=${encodeURIComponent(shadowConfig)}`);
  const shadowId = (await ctl("GET", "/overview")).tunnels.find(
    (tunnel) => tunnel.config_path === shadowConfig
  ).id;
  for (const change of ["new", "changed"]) {
    // biome-ignore lint/performance/noAwaitInLoops: Polls and native UI actions must complete in order before the next step.
    const shadowDraft = await ctl("GET", `/hostnames?id=${shadowId}`);
    if (change === "new") {
      shadowDraft.routes.push({
        hostname: "API.EXAMPLE.INVALID",
        index: null,
        path: null,
        service: "43222",
      });
    } else {
      shadowDraft.routes[1].hostname = "API.EXAMPLE.INVALID";
    }
    const rejected = await request(
      "POST",
      `/hostnames?id=${shadowId}`,
      shadowDraft
    );
    check(
      rejected.status === 400 &&
        rejected.value.error.includes(
          "`api.example.invalid` is already matched"
        ) &&
        readFileSync(shadowConfig, "utf8") === shadowSource &&
        backups().length === 1 &&
        pending().length === 0 &&
        (await state()).pid === updated.pid &&
        runCount() === starts + 1,
      `${change} hostname shadowed by an earlier wildcard is rejected without publication, backup, or restart`
    );
  }

  await ctl("POST", `/stop?id=${id}`);
  await wait("stopped", async () => (await state()).status === "stopped");
  draft = await load();
  draft.routes[0].hostname = "stopped.example.invalid";
  check((await save(draft)).restart_error === null, "stopped save succeeds");
  check(
    (await state()).status === "stopped" &&
      runCount() === starts + 1 &&
      (await view()).state.mappings[0].hostname === "stopped.example.invalid",
    "stopped save stays stopped and immediately refreshes overview mappings"
  );
  check(
    backups().length === 2 &&
      readFileSync(join(root, backups()[1]), "utf8") === saved,
    "each successful change keeps its own exact backup"
  );
  draft = await load();
  draft.routes = [];
  await save(draft);
  const empty = await load();
  const finalMappings = (await view()).state.mappings;
  check(
    empty.routes.length === 0 &&
      empty.fallback_service === "http_status:403" &&
      finalMappings.length === 2 &&
      finalMappings[0].path === "^/assets/.*" &&
      finalMappings[1].service === "http_status:403",
    "removing all hostnames retains hidden path-only rule and required custom catch-all"
  );
  const beforeNoop = readFileSync(config, "utf8");
  const backupCount = backups().length;
  await save(empty);
  check(
    readFileSync(config, "utf8") === beforeNoop &&
      backups().length === backupCount &&
      runCount() === starts + 1 &&
      pending().length === 0,
    "empty no-op still leaves exact bytes, backups, and connector count unchanged"
  );
  evidence.passed = true;
  console.log("HOSTNAMES NATIVE E2E PASSED");
} catch (error) {
  evidence.passed = false;
  evidence.error = String(error);
  evidence.app_log = appLog();
  console.error(error, appLog());
  process.exitCode = 1;
} finally {
  await stopApp(child, ctl, id);
  mkdirSync("e2e/out/hostnames", { recursive: true });
  writeFileSync(
    "e2e/out/hostnames/native-results.json",
    JSON.stringify(evidence, null, 2)
  );
}
