const INVALID = /invalid/;
const TIMEOUT = /control timeout/;
const ABSENT = /absent.*false/;

import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import http from "node:http";
import test from "node:test";
import { createControl, stopApp, waitFor } from "./harness.mjs";

async function localControl(t, handler) {
  const server = http.createServer(handler);
  await new Promise((done) => server.listen(0, "127.0.0.1", done));
  t.after(() => new Promise((done) => server.close(done)));
  return server.address().port;
}
test("control supports JSON bodies, envelopes, raw failures and strict errors", async (t) => {
  const port = await localControl(t, (incoming, response) => {
    let text = "";
    incoming.on("data", (chunk) => {
      text += chunk;
    });
    incoming.on("end", () => {
      const routes = {
        "/bad": [400, '{"error":"invalid"}'],
        "/raw": [200, "plain text"],
      };
      const [status, body] = routes[incoming.url] ?? [
        200,
        JSON.stringify({ received: JSON.parse(text || "null") }),
      ];
      response.writeHead(status);
      response.end(body);
    });
  });
  const { ctl, request } = createControl(port, { strict: true });
  assert.deepEqual(await ctl("POST", "/echo", { hello: "world" }), {
    received: { hello: "world" },
  });
  assert.equal((await request("GET", "/bad")).status, 400);
  await assert.rejects(ctl("GET", "/bad"), INVALID);
  await assert.rejects(ctl("GET", "/raw"), SyntaxError);
  assert.deepEqual(
    await createControl(port, { envelope: true }).ctl("GET", "/raw"),
    { body: "plain text", status: 200 }
  );
});
test("control request timeouts reject instead of hanging", async (t) => {
  const port = await localControl(t, () => {
    /* Intentionally never respond. */
  });
  await assert.rejects(
    createControl(port, { timeout: 20 }).ctl("GET", "/hang"),
    TIMEOUT
  );
});
test("polling retries thrown and false results, then returns the first success", async () => {
  let calls = 0;
  const result = await waitFor(
    "fixture readiness",
    () => {
      calls += 1;
      if (calls === 1) {
        throw new Error("not ready");
      }
      return calls === 3 && "ready";
    },
    100,
    1
  );
  assert.equal(result, "ready");
  assert.equal(calls, 3);
  await assert.rejects(
    waitFor("absent", () => false, 5, 1),
    ABSENT
  );
});
test("cleanup tolerates failed stop and quit requests and kills the lingering child", async () => {
  const child = new EventEmitter();
  child.exitCode = null;
  let signal;
  child.kill = (value) => {
    signal = value;
  };
  const paths = [];
  const ctl = (_method, path) => {
    paths.push(path);
    return Promise.reject(new Error("already gone"));
  };
  const finished = stopApp(child, ctl, "fixture");
  await finished;
  assert.deepEqual(paths, ["/stop?id=fixture", "/quit"]);
  assert.equal(signal, "SIGTERM");
});
test("cleanup does not wait or kill a child that already exited", async () => {
  const child = {
    exitCode: 0,
    kill() {
      assert.fail("exited child must not be killed");
    },
  };
  await stopApp(child, () => Promise.resolve());
});
