import { describe, expect, it, vi } from "vitest";
import { returnFocusOnClose, returnFocusTo } from "./focus";

const el = (connected: boolean) =>
  ({ focus: vi.fn(), isConnected: connected }) as unknown as HTMLElement;
const ev = () => ({ preventDefault: vi.fn() }) as unknown as Event;

describe("returnFocusTo", () => {
  it("focuses a connected trigger and stops the library's own restore", () => {
    const t = el(true),
      e = ev();
    returnFocusTo(() => t)(e);
    expect(e.preventDefault).toHaveBeenCalled();
    expect(t.focus).toHaveBeenCalledWith({ preventScroll: true });
  });
  it("does nothing for a missing or detached trigger", () => {
    const t = el(false),
      e = ev();
    returnFocusTo(() => t)(e);
    returnFocusTo(() => null)(e);
    expect(e.preventDefault).not.toHaveBeenCalled();
    expect(t.focus).not.toHaveBeenCalled();
  });
});

describe("returnFocusOnClose", () => {
  // The outside click's target: `closest` finds the field it hit, or nothing (blank space, a button).
  const hit = (field: HTMLElement | null) =>
    ({ target: { closest: () => field } }) as unknown as Event;

  it("returns focus to the trigger after Escape or a pick", () => {
    const t = el(true),
      e = ev();
    returnFocusOnClose(() => t).onCloseAutoFocus(e);
    expect(t.focus).toHaveBeenCalled();
  });
  it("focuses the field an outside click went to, not the trigger", () => {
    const t = el(true),
      field = el(true),
      e = ev();
    const h = returnFocusOnClose(() => t);
    h.onInteractOutside(hit(field));
    h.onCloseAutoFocus(e);
    expect(e.preventDefault).toHaveBeenCalled();
    expect(field.focus).toHaveBeenCalledWith({ preventScroll: true });
    expect(t.focus).not.toHaveBeenCalled();
  });
  it("falls back to the trigger after a click on nothing focusable, and forgets the click", () => {
    const t = el(true),
      field = el(true);
    const h = returnFocusOnClose(() => t);
    h.onInteractOutside(hit(null));
    h.onCloseAutoFocus(ev());
    expect(t.focus).toHaveBeenCalledTimes(1);
    h.onInteractOutside(hit(field));
    h.onCloseAutoFocus(ev());
    h.onCloseAutoFocus(ev());
    expect(field.focus).toHaveBeenCalledTimes(1);
    expect(t.focus).toHaveBeenCalledTimes(2);
  });
});
