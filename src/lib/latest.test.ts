import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { latest } from "./latest";

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

describe("latest", () => {
  it("runs the job after the delay and resolves when it's done", async () => {
    const l = latest();
    const seen: string[] = [];
    const p = l.run(async (current) => void (current() && seen.push("a")), 100);
    await vi.advanceTimersByTimeAsync(99);
    expect(seen).toEqual([]);
    await vi.advanceTimersByTimeAsync(1);
    await p;
    expect(seen).toEqual(["a"]);
  });

  it("a newer run supersedes a pending one, which resolves without running", async () => {
    const l = latest();
    const seen: string[] = [];
    let first = false;
    l.run(async () => void seen.push("a"), 100).then(() => (first = true));
    const p = l.run(async () => void seen.push("b"), 100);
    await vi.advanceTimersByTimeAsync(0);
    expect(first).toBe(true);
    await vi.advanceTimersByTimeAsync(100);
    await p;
    expect(seen).toEqual(["b"]);
  });

  it("cancel() resolves the pending promise and the job never starts (select(id) then select(null))", async () => {
    const l = latest();
    const job = vi.fn(async () => {});
    const p = l.run(job, 0);
    l.cancel();
    await expect(p).resolves.toBeUndefined();
    await vi.runAllTimersAsync();
    expect(job).not.toHaveBeenCalled();
  });

  it("a job in flight when superseded sees current() turn false and its result is dropped", async () => {
    const l = latest();
    let release!: () => void;
    const gate = new Promise<void>((r) => (release = r));
    const applied: string[] = [];
    const p = l.run(async (current) => {
      await gate;
      if (current()) applied.push("stale");
    }, 0);
    await vi.advanceTimersByTimeAsync(0); // the job is now waiting on its "IPC"
    l.cancel();
    await p; // resolved by cancel(), not left hanging on the gate
    release();
    await vi.runAllTimersAsync();
    expect(applied).toEqual([]);
  });

  it("the latest of several runs is the only one that applies", async () => {
    const l = latest();
    const applied: number[] = [];
    const ps = [1, 2, 3].map((n) => l.run(async (current) => void (current() && applied.push(n)), 100));
    await vi.advanceTimersByTimeAsync(100);
    await Promise.all(ps);
    expect(applied).toEqual([3]);
  });
});
