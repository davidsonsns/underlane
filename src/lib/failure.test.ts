import { describe, expect, it } from "vitest";
import { failurePresentation } from "./failure";

describe("failure presentation", () => {
  it("uses the backend explanation without reclassifying its message", () => {
    const failure = { cause: "Credentials are missing.", action: "Restore the credentials file.", raw: "credentials-file=[redacted]" };
    expect(failurePresentation(failure, "older detail", "older hint")).toEqual(failure);
  });

  it("keeps unknown diagnostics useful without inferring a cause from their contents", () => {
    for (const raw of ["unexpected output", "{malformed", "/tmp/invalid-token.yml", '{"name":"unauthorized"}']) {
      expect(failurePresentation(null, raw, null)).toEqual({
        cause: "The tunnel couldn't complete this operation.",
        action: "Review the diagnostic below, then try again.",
        raw,
      });
    }
  });

  it("retains a legacy safe hint and handles a missing diagnostic", () => {
    expect(failurePresentation(undefined, null, "Check the local service.")).toEqual({
      cause: "The tunnel couldn't complete this operation.",
      action: "Check the local service.",
      raw: "No diagnostic was reported.",
    });
  });
});
