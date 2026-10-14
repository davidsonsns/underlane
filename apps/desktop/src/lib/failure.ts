import type { FailureExplanation } from "./types";

// Older state/validation responses still contain a redacted diagnostic, without a diagnosis.
export function failurePresentation(
  failure: FailureExplanation | null | undefined,
  raw: string | null,
  action: string | null
): FailureExplanation {
  return (
    failure ?? {
      action: action ?? "Review the diagnostic below, then try again.",
      cause: "The tunnel couldn't complete this operation.",
      raw: raw ?? "No diagnostic was reported.",
    }
  );
}
