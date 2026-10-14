import { describe, expect, it } from "vitest";
import {
  inspectionSubtitle,
  requestEmptyMessage,
  routeReach,
  routeReachTip,
} from "./view-state";

const tunnel = {
  inspect: true,
  state: {
    inspecting: true,
    public_url: "https://test.example",
    status: "connected",
  },
};
describe("inspection view state", () => {
  it("prioritizes pause and distinguishes unavailable, disabled, waiting, and active inspection", () => {
    expect(inspectionSubtitle(tunnel, true)).toBe("Capture paused");
    expect(inspectionSubtitle(null, false)).toBe("");
    expect(inspectionSubtitle({ ...tunnel, inspect: false }, false)).toBe(
      "Inspection is off for this tunnel"
    );
    expect(
      inspectionSubtitle(
        { ...tunnel, state: { ...tunnel.state, inspecting: false } },
        false
      )
    ).toBe("Inspection on, starts with the tunnel");
    expect(inspectionSubtitle(tunnel, false)).toBe("Inspecting");
  });
  it("gives filtered and paused views precedence over a connected tunnel", () => {
    expect(requestEmptyMessage(tunnel, true, true)).toBe(
      "Try a different filter."
    );
    expect(requestEmptyMessage(tunnel, false, true)).toContain(
      "Resume capture"
    );
    expect(requestEmptyMessage(tunnel, false, false)).toContain(
      tunnel.state.public_url
    );
    expect(
      requestEmptyMessage(
        { ...tunnel, state: { ...tunnel.state, public_url: null } },
        false,
        false
      )
    ).toContain("this tunnel");
    expect(requestEmptyMessage(null, false, false)).toContain(
      "Start the tunnel"
    );
  });
});
describe("local route reachability", () => {
  it.each([
    [true, true, "up"],
    [true, false, "ready"],
    [false, true, "down"],
    [false, false, "idle"],
    [null, true, "none"],
    [undefined, false, "none"],
  ] as const)(
    "maps reachable=%s live=%s to %s",
    (reachable, live, expected) => {
      expect(routeReach(reachable, live)).toBe(expected);
    }
  );
  it("keeps DNS separate and explains local service failures", () => {
    expect(routeReachTip("http://localhost:3000", "up")).toContain(
      "doesn't manage this service"
    );
    expect(routeReachTip("http://localhost:3000", "ready")).toContain(
      "tunnel is stopped"
    );
    expect(routeReachTip("http://localhost:3000", "down")).toContain("502");
    expect(routeReachTip("http://localhost:3000", "idle")).toContain(
      "before starting the tunnel"
    );
    expect(routeReachTip("http://localhost:3000", "none")).toBeUndefined();
  });
});
