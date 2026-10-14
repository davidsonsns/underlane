import { expect, it, vi } from "vitest";
import { guardedQuery, queryTracker } from "./guarded-query";

it("publishes current failures and always finishes the current query", async () => {
  const error = new Error("network");
  const onError = vi.fn(),
    onFinish = vi.fn();
  await guardedQuery(
    () => Promise.reject(error),
    () => true,
    onError,
    onFinish
  );
  expect(onError).toHaveBeenCalledWith(error);
  expect(onFinish).toHaveBeenCalledOnce();
});

it("a superseded query cannot publish errors or clear the next query's loading state", async () => {
  const onError = vi.fn(),
    onFinish = vi.fn();
  await guardedQuery(
    () => Promise.reject(new Error("old")),
    () => false,
    onError,
    onFinish
  );
  expect(onError).not.toHaveBeenCalled();
  expect(onFinish).not.toHaveBeenCalled();
});

it("checks liveness when the pending work actually finishes", async () => {
  let live = true;
  let resolve: (() => void) | undefined;
  const work = new Promise<void>((done) => {
    resolve = done;
  });
  const onFinish = vi.fn();
  const query = guardedQuery(
    () => work,
    () => live,
    vi.fn(),
    onFinish
  );
  live = false;
  resolve?.();
  await query;
  expect(onFinish).not.toHaveBeenCalled();
});

function deferred() {
  let finish: (() => void) | undefined;
  const promise = new Promise<void>((resolve) => {
    finish = resolve;
  });
  return { complete: () => finish?.(), promise };
}

it("an old query completion cannot clear a newer inflight query", async () => {
  const tracker = queryTracker({ failure: null, loading: true });
  const old = deferred(),
    fresh = deferred();
  const first = tracker.run(
    () => old.promise,
    () => false
  );
  const second = tracker.run(
    () => fresh.promise,
    () => true
  );
  old.complete();
  await first;
  expect(tracker.active).toBe(second);
  fresh.complete();
  await second;
  expect(tracker.active).toBeNull();
});

it("cancellation clears the inflight slot without letting old completion replace it", async () => {
  const tracker = queryTracker({ failure: null, loading: true });
  const work = deferred();
  const run = tracker.run(
    () => work.promise,
    () => false
  );
  tracker.cancel();
  expect(tracker.active).toBeNull();
  work.complete();
  await run;
  expect(tracker.active).toBeNull();
});
