import { describe, expect, it } from "vitest";
import { errorText } from "./api";

describe("backend failure messages", () => {
  it("preserves string, Error, and structured Tauri failures", () => {
    expect(errorText("not connected")).toBe("not connected");
    expect(errorText(new Error("denied"))).toBe("denied");
    expect(errorText({ message: 503 })).toBe("503");
  });
  it("handles unexpected null and primitive rejection values", () => {
    expect(errorText(null)).toBe("null");
    expect(errorText(undefined)).toBe("undefined");
    expect(errorText(42)).toBe("42");
  });
});
