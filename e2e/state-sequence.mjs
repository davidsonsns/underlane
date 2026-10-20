// The actual AppStore receives lifecycle events and delayed IPC overview replies.
import assert from "node:assert/strict";
import {
  browserPage,
  installHostnameMock,
  installPageMock,
} from "./lib/browser.mjs";

function installStateSequenceMock() {
  const m = window.__stateSequence;
  m.store = async () => {
    const resource = performance
      .getEntriesByType("resource")
      .find(
        (entry) => new URL(entry.name).pathname === "/src/lib/store.svelte.ts"
      );
    return (await import(resource.name)).store;
  };
  const [tunnel] = m.overview.tunnels;
  tunnel.connection = null;
  const base = structuredClone(tunnel.state);
  m.state = (state_seq, enabledIndex, status = "connected", run = 1) => ({
    ...structuredClone(base),
    connections: status === "stopped" ? 0 : 4,
    mappings: base.mappings.map((mapping, index) => ({
      ...mapping,
      enabled: index === enabledIndex,
    })),
    pid: status === "stopped" ? null : 123,
    run,
    state_seq,
    status,
    supervised: status !== "stopped",
  });
  tunnel.state = m.state(8, 0);
  const dispatch = window.__mockDispatch;
  window.__mockDispatch = (command, args) => {
    if (command === "overview" && m.holdOverview) {
      m.holdOverview = false;
      const snapshot = structuredClone(m.overview);
      return new Promise((resolve) => {
        m.resolveOverview = () => resolve(snapshot);
      });
    }
    return dispatch(command, args);
  };
}

const { baseUrl, browser, page, errors } = await browserPage({
  webkit: process.env.BROWSER === "webkit",
});
const read = () =>
  page.evaluate(async () => {
    const store = await window.__stateSequence.store();
    const [tunnel] = store.tunnels;
    return {
      enabled: tunnel?.state.mappings
        .filter((mapping) => mapping.hostname)
        .map((mapping) => mapping.enabled),
      run: tunnel?.state.run,
      seq: tunnel?.state.state_seq ?? 0,
      status: tunnel?.state.status,
    };
  });
async function emit(seq, enabled, status = "connected", run = 1) {
  await page.evaluate(
    ([sequence, index, nextStatus, nextRun]) => {
      const payload = window.__stateSequence.state(
        sequence,
        index,
        nextStatus,
        nextRun
      );
      if (sequence === null) {
        payload.state_seq = undefined;
      }
      window.__mockListeners["tunnel-state"]?.({ payload });
    },
    [seq, enabled, status, run]
  );
}
async function holdOverview(seq, enabled, status = "connected", run = 1) {
  await page.evaluate(
    async ([sequence, index, nextStatus, nextRun]) => {
      const m = window.__stateSequence;
      m.overview.tunnels[0].state = m.state(
        sequence,
        index,
        nextStatus,
        nextRun
      );
      m.holdOverview = true;
      m.resolveOverview = null;
      const store = await m.store();
      window.__pendingOverview = store.refresh();
    },
    [seq, enabled, status, run]
  );
  await page.waitForFunction(() =>
    Boolean(window.__stateSequence.resolveOverview)
  );
}
async function releaseOverview() {
  await page.evaluate(async () => {
    window.__stateSequence.resolveOverview();
    await window.__pendingOverview;
  });
}

try {
  await installPageMock(
    page,
    installHostnameMock,
    { delay: 0, mockName: "__stateSequence" },
    installStateSequenceMock
  );
  await page.goto(`${baseUrl}/#tunnels:synthetic`);
  await page.getByRole("heading", { exact: true, name: "Hostnames" }).waitFor();

  await emit(10, -1, "stopped");
  await emit(9, 0);
  assert.deepEqual(await read(), {
    enabled: [false, false, false, false],
    run: 1,
    seq: 10,
    status: "stopped",
  });
  await emit(11, 1);
  assert.equal((await read()).seq, 11);
  await holdOverview(11, 1);
  await emit(12, 2);
  await releaseOverview();
  assert.deepEqual((await read()).enabled, [false, false, true, false]);
  assert.equal((await read()).seq, 12);

  // A newer overview wins even if a row event changed state while the IPC was outstanding.
  await holdOverview(14, 0);
  await emit(13, 3);
  await releaseOverview();
  await emit(13, 3);
  assert.equal((await read()).seq, 14);
  assert.deepEqual((await read()).enabled, [true, false, false, false]);
  await emit(null, 2);
  await emit(0, 2);
  await holdOverview(0, 2);
  await releaseOverview();
  assert.equal((await read()).seq, 14);
  await emit(100, 0);
  await emit(1, 1, "connected", 2);
  await emit(999, 0, "connected", 1);
  assert.equal((await read()).run, 2);
  assert.equal((await read()).seq, 1);
  assert.deepEqual((await read()).enabled, [false, true, false, false]);

  // Reset drops the high-water state and rejects an overview captured before reset.
  await holdOverview(101, 0, "connected", 2);
  await page.evaluate(() => {
    const m = window.__stateSequence;
    m.overview.tunnels[0].state = m.state(0, -1, "stopped", 0);
    window.__mockListeners["app-reset"]?.({ payload: null });
  });
  await page.waitForFunction(async () => {
    const store = await window.__stateSequence.store();
    return store.tunnels[0]?.state.run === 0;
  });
  await releaseOverview();
  assert.equal((await read()).run, 0);
  assert.equal((await read()).seq, 0);
  assert.deepEqual(errors, []);
  console.log(
    "State sequence passed: stopped/partial rows reject stale events and overviews, newer replies advance, new runs supersede prior sequences, reset clears state."
  );
} finally {
  await browser.close();
}
