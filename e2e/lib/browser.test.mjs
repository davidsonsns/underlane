const SAVE_FAILURE = /Could not save/;

import assert from "node:assert/strict";
import test from "node:test";
import vm from "node:vm";
import { installHostnameMock, installPageMock } from "./browser.mjs";

test("serialized browser bridge dispatches fixture commands and preserves event callbacks", async () => {
  let source;
  await installPageMock(
    {
      addInitScript: (script) => {
        source = script.content;
      },
    },
    installHostnameMock,
    { delay: 0, mockName: "__projectMock" },
    ({ mockName }) => {
      window[mockName].extended = true;
    }
  );
  const window = {};
  vm.runInNewContext(source, { console, setTimeout, structuredClone, window });
  const bridge = window.__TAURI_INTERNALS__;
  assert.equal(window.__projectMock.extended, true);
  const overview = await bridge.invoke("overview");
  assert.equal(overview.tunnels[0].name, "example");
  overview.tunnels[0].name = "mutated copy";
  assert.equal((await bridge.invoke("overview")).tunnels[0].name, "example");
  await bridge.invoke("update_tunnel", { patch: { name: "renamed" } });
  assert.equal((await bridge.invoke("overview")).tunnels[0].name, "renamed");
  window.__projectMock.renameDelay = 0;
  await bridge.invoke("rename_group", { name: "new group" });
  assert.equal((await bridge.invoke("overview")).groups[0].name, "new group");
  window.__projectMock.failRename = true;
  await assert.rejects(
    bridge.invoke("rename_group", { name: "failed group" }),
    SAVE_FAILURE
  );
  assert.equal((await bridge.invoke("overview")).groups[0].name, "new group");
  let event;
  const handler = bridge.transformCallback((value) => {
    event = value;
  });
  await bridge.invoke("plugin:event|listen", {
    event: "fixture-event",
    handler,
  });
  window.__mockListeners["fixture-event"]("payload");
  assert.equal(event, "payload");
  bridge.unregisterCallback(handler);
  assert.equal(await bridge.invoke("plugin:window|unknown", {}), null);
  assert.equal((await bridge.invoke("load_hostnames")).routes.length, 4);
});
