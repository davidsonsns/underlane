import { describe, expect, it } from "vitest";
import { groupNameError, normalizeGroupName } from "./group-name";

describe("project names", () => {
  it("trims and collapses whitespace, including pasted lines and Unicode spaces", () => {
    expect(normalizeGroupName("  My   project\n\t name\u00a0\u0085 ")).toBe("My project name");
    expect(groupNameError(normalizeGroupName(" \n\t "))).toBe("Enter a name.");
  });

  it("accepts 64 characters and rejects longer names without splitting Unicode characters", () => {
    expect(groupNameError("a".repeat(64))).toBeNull();
    expect(groupNameError("a".repeat(65))).toBe("Use 64 characters or fewer.");
    expect(groupNameError("🚀".repeat(64))).toBeNull();
    expect(groupNameError("🚀".repeat(65))).not.toBeNull();
  });
});
