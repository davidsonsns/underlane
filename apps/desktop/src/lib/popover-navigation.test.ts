import { describe, expect, it } from "vitest";
import {
  popoverKeyAction,
  popoverRowIndex,
  tunnelSummary,
} from "./popover-navigation";

describe("tray keyboard navigation", () => {
  const key = {
    altKey: false,
    ctrlKey: false,
    key: "ArrowDown",
    metaKey: false,
  };
  it("keeps modified arrows native and always permits Escape", () => {
    expect(popoverKeyAction(key)).toBe("navigate");
    expect(popoverKeyAction({ ...key, metaKey: true })).toBeNull();
    expect(popoverKeyAction({ ...key, ctrlKey: true })).toBeNull();
    expect(popoverKeyAction({ ...key, altKey: true })).toBeNull();
    expect(popoverKeyAction({ ...key, key: "Tab" })).toBeNull();
    expect(popoverKeyAction({ ...key, key: "Escape", metaKey: true })).toBe(
      "hide"
    );
  });
  it("enters from either end outside the rows and keeps existing row bounds", () => {
    expect(popoverRowIndex("ArrowUp", -1, 3)).toBe(2);
    expect(popoverRowIndex("ArrowDown", -1, 3)).toBe(0);
    expect(popoverRowIndex("Home", 2, 3)).toBe(0);
    expect(popoverRowIndex("End", 0, 3)).toBe(2);
    expect(popoverRowIndex("ArrowDown", -1, 0)).toBe(-1);
  });
  it("reports running and starting counts without suggesting stopped tunnels are active", () => {
    expect(tunnelSummary(0, 0)).toBe("No tunnels running");
    expect(tunnelSummary(0, 2)).toBe("2 starting");
    expect(tunnelSummary(3, 0)).toBe("3 running");
    expect(tunnelSummary(3, 2)).toBe("3 running, 2 starting");
  });
});
