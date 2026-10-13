import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { configChecker, type ConfigTarget } from "./config-change";
import type { ConfigChange } from "./types";

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

const baseline: ConfigTarget = { id: "one", source_path: "/tmp/source.yml", run: 4, config_revision: 2 };
const result = (status: ConfigChange["status"] = "changed"): ConfigChange => ({
  source_path: baseline.source_path, run: baseline.run, config_revision: baseline.config_revision, status, message: null,
});

function setup() {
  let target: ConfigTarget | null = { ...baseline };
  const load = vi.fn<(id: string) => Promise<ConfigChange | null>>();
  const apply = vi.fn();
  const fail = vi.fn();
  const checker = configChecker(() => target, load, apply, fail);
  return { load, apply, fail, checker, select: (next: ConfigTarget | null) => { target = next; } };
}

describe("config checks", () => {
  it("coalesces mount/focus/visibility while a read is outstanding, then permits a fresh check", async () => {
    const { checker, load, apply } = setup();
    const read = deferred<ConfigChange | null>();
    load.mockReturnValueOnce(read.promise).mockResolvedValueOnce(result("unchanged"));
    const mount = checker.check();
    expect(checker.check()).toBe(mount);
    await vi.advanceTimersByTimeAsync(0);
    expect(checker.check()).toBe(mount);
    expect(load).toHaveBeenCalledExactlyOnceWith("one");
    read.resolve(result());
    await mount;
    expect(apply).toHaveBeenLastCalledWith(result());
    const focus = checker.check();
    await vi.advanceTimersByTimeAsync(0);
    await focus;
    expect(load).toHaveBeenCalledTimes(2);
    expect(apply).toHaveBeenLastCalledWith(result("unchanged"));
  });

  it.each([
    ["selection", { ...baseline, id: "two" }],
    ["source path", { ...baseline, source_path: "/tmp/replaced.yml" }],
    ["restart", { ...baseline, run: 5 }],
    ["internal successful reload", { ...baseline, config_revision: 3 }],
    ["stop or unsupported kind", null],
  ] as const)("discards a delayed result when the %s changes even before the next check", async (_, next) => {
    const { checker, load, apply, fail, select } = setup();
    const read = deferred<ConfigChange | null>();
    load.mockReturnValue(read.promise);
    const pending = checker.check();
    await vi.advanceTimersByTimeAsync(0);
    select(next);
    read.resolve(result());
    await pending;
    expect(apply).not.toHaveBeenCalled();
    expect(fail).not.toHaveBeenCalled();
  });

  it("keeps the new selection's reply when an older read completes afterward", async () => {
    const { checker, load, apply, select } = setup();
    const old = deferred<ConfigChange | null>();
    load.mockReturnValueOnce(old.promise).mockResolvedValueOnce({ ...result("unchanged"), run: 5 });
    const first = checker.check();
    await vi.advanceTimersByTimeAsync(0);
    select({ ...baseline, run: 5 });
    const second = checker.check();
    await vi.advanceTimersByTimeAsync(0);
    await second;
    old.resolve(result());
    await first;
    expect(apply).toHaveBeenCalledExactlyOnceWith({ ...result("unchanged"), run: 5 });
  });

  it("discards replies and errors after unmount and does not read a stopped target", async () => {
    const { checker, load, apply, fail, select } = setup();
    const read = deferred<ConfigChange | null>();
    load.mockReturnValueOnce(read.promise);
    const pending = checker.check();
    await vi.advanceTimersByTimeAsync(0);
    checker.cancel();
    read.reject(new Error("late read failure"));
    await pending;
    await vi.advanceTimersByTimeAsync(0);
    select(null);
    await checker.check();
    expect(load).toHaveBeenCalledTimes(1);
    expect(apply).not.toHaveBeenCalled();
    expect(fail).not.toHaveBeenCalled();
  });

  it.each([
    null,
    { ...result(), run: 5 },
    { ...result(), config_revision: 3 },
    { ...result(), source_path: "/tmp/other.yml" },
  ])("does not treat a backend stale snapshot as unchanged: %j", async (reply) => {
    const { checker, load, apply } = setup();
    load.mockResolvedValueOnce(reply);
    const pending = checker.check();
    await vi.advanceTimersByTimeAsync(0);
    await pending;
    expect(apply).not.toHaveBeenCalled();
  });

  it("preserves unreadable state and surfaces IPC failure separately, without reporting unchanged", async () => {
    const { checker, load, apply, fail } = setup();
    const unreadable = { ...result("unreadable"), message: "Couldn't read the config file." };
    const failure = new Error("IPC unavailable");
    load.mockResolvedValueOnce(unreadable).mockRejectedValueOnce(failure);
    let pending = checker.check();
    await vi.advanceTimersByTimeAsync(0);
    await pending;
    expect(apply).toHaveBeenCalledExactlyOnceWith(unreadable);
    pending = checker.check();
    await vi.advanceTimersByTimeAsync(0);
    await pending;
    expect(fail).toHaveBeenCalledExactlyOnceWith(failure);
    expect(apply).toHaveBeenCalledTimes(1);
  });
});
