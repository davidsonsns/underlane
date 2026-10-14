import { tick } from "svelte";

/** Publish only mounted virtual-list rows; cancel when the selection or result set changes. */
export function observeDescendant(
  box: () => HTMLElement | undefined,
  id: string | null,
  publish: (value: string | undefined) => void
) {
  let live = true;
  const mounted = () => id && box()?.querySelector(`[id="${id}"]`);
  const frame = () =>
    new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
  async function waitForRow() {
    await tick();
    for (
      let attempt = 0;
      live && attempt < 4 && id && !mounted();
      attempt += 1
    ) {
      // biome-ignore lint/performance/noAwaitInLoops: Each frame lets virtua mount the row before retrying.
      await frame();
    }
    if (live) {
      publish(mounted() ? (id ?? undefined) : undefined);
    }
  }
  waitForRow();
  return () => {
    live = false;
  };
}
