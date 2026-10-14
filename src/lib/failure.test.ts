import { describe, expect, it } from "vitest";
import { failurePresentation } from "./failure";

describe("failure presentation", () => {
  it("uses the backend explanation without reclassifying its message", () => {
    const failure = {
      action: "Restore the credentials file.",
      cause: "Credentials are missing.",
      raw: "credentials-file=[redacted]",
    };
    expect(failurePresentation(failure, "older detail", "older hint")).toEqual(
      failure
    );
  });

  it("keeps unknown diagnostics useful without inferring a cause from their contents", () => {
    for (const raw of [
      "unexpected output",
      "{malformed",
      "/tmp/invalid-token.yml",
      '{"name":"unauthorized"}',
    ]) {
      expect(failurePresentation(null, raw, null)).toEqual({
        action: "Review the diagnostic below, then try again.",
        cause: "The tunnel couldn't complete this operation.",
        raw,
      });
    }
  });

  it("retains a legacy safe hint and handles a missing diagnostic", () => {
    expect(
      failurePresentation(undefined, null, "Check the local service.")
    ).toEqual({
      action: "Check the local service.",
      cause: "The tunnel couldn't complete this operation.",
      raw: "No diagnostic was reported.",
    });
  });
});
