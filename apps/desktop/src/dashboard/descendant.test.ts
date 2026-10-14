import { afterEach, expect, it, vi } from "vitest";
import { observeDescendant } from "./descendant";

vi.mock("svelte", () => ({ tick: () => Promise.resolve() }));
afterEach(() => vi.unstubAllGlobals());

function fixture() {
  const frames: FrameRequestCallback[] = [];
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
    frames.push(callback);
    return frames.length;
  });
  let mounted = false;
  const box = {
    querySelector: () => (mounted ? {} : null),
  } as unknown as HTMLElement;
  const publish = vi.fn();
  return {
    box,
    frames,
    mount: () => {
      mounted = true;
    },
    publish,
  };
}

it("waits for a virtual row to mount before publishing its descendant", async () => {
  const f = fixture();
  observeDescendant(() => f.box, "req-one", f.publish);
  await Promise.resolve();
  expect(f.publish).not.toHaveBeenCalled();
  f.mount();
  f.frames.shift()?.(0);
  await Promise.resolve();
  expect(f.publish).toHaveBeenCalledWith("req-one");
});

it("a changed selection cancels a pending descendant publication", async () => {
  const f = fixture();
  const cancel = observeDescendant(() => f.box, "req-one", f.publish);
  await Promise.resolve();
  cancel();
  f.mount();
  f.frames.shift()?.(0);
  await Promise.resolve();
  expect(f.publish).not.toHaveBeenCalled();
});

it("clears the descendant when selection is empty", async () => {
  const f = fixture();
  observeDescendant(() => f.box, null, f.publish);
  await Promise.resolve();
  expect(f.publish).toHaveBeenCalledWith(undefined);
});
