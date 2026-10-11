import { describe, expect, it, vi } from "vitest";
import { returnFocusTo } from "./focus";

const el = (connected: boolean) => ({ isConnected: connected, focus: vi.fn() }) as unknown as HTMLElement;
const ev = () => ({ preventDefault: vi.fn() }) as unknown as Event;

describe("returnFocusTo", () => {
  it("focuses a connected trigger and stops the library's own restore", () => {
    const t = el(true), e = ev();
    returnFocusTo(() => t)(e);
    expect(e.preventDefault).toHaveBeenCalled();
    expect(t.focus).toHaveBeenCalledWith({ preventScroll: true });
  });
  it("does nothing for a missing or detached trigger", () => {
    const t = el(false), e = ev();
    returnFocusTo(() => t)(e);
    returnFocusTo(() => null)(e);
    expect(e.preventDefault).not.toHaveBeenCalled();
    expect(t.focus).not.toHaveBeenCalled();
  });
});
