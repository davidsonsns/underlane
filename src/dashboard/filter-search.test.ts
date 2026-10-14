import { describe, expect, it, vi } from "vitest";
import { filterSearchKeys, focusFilterSearch } from "./filter-search";

function keyboardEvent(key: string, items: { focus: () => void }[]) {
  return {
    currentTarget: { closest: () => ({ querySelectorAll: () => items }) },
    key,
    preventDefault: vi.fn(),
    stopPropagation: vi.fn(),
  } as unknown as KeyboardEvent;
}
describe("filter search keyboard handoff", () => {
  it("moves arrows to the first or last selectable item", () => {
    const first = { focus: vi.fn() },
      last = { focus: vi.fn() };
    const down = keyboardEvent("ArrowDown", [first, last]);
    filterSearchKeys(down);
    expect(first.focus).toHaveBeenCalledOnce();
    expect(down.preventDefault).toHaveBeenCalledOnce();
    expect(down.stopPropagation).toHaveBeenCalledOnce();
    filterSearchKeys(keyboardEvent("ArrowUp", [first, last]));
    expect(last.focus).toHaveBeenCalledOnce();
  });
  it("preserves escape and tab while blocking menu typeahead for text and spaces", () => {
    for (const key of ["Escape", "Tab", "a", " ", "ArrowDown"]) {
      const event = keyboardEvent(key, []);
      filterSearchKeys(event);
      expect(event.preventDefault).not.toHaveBeenCalled();
      expect(event.stopPropagation).toHaveBeenCalledTimes(
        Number(key === "a" || key === " ")
      );
    }
  });
  it("defers focus until Bits has mounted its options", async () => {
    const event = new Event("open", { cancelable: true }),
      input = { focus: vi.fn() };
    const frame = vi.fn();
    vi.stubGlobal("requestAnimationFrame", frame);
    try {
      await focusFilterSearch(
        event,
        () => input as unknown as HTMLInputElement
      );
      expect(event.defaultPrevented).toBe(true);
      expect(input.focus).not.toHaveBeenCalled();
      frame.mock.calls[0][0]();
      expect(input.focus).toHaveBeenCalledOnce();
    } finally {
      vi.unstubAllGlobals();
    }
  });
});
