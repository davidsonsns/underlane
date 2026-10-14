import { describe, expect, it } from "vitest";
import { logLevels, matchesLogFilters } from "./log-filters";

const line = {
  fields: '{"reason":"timeout"}',
  level: "fatal" as const,
  message: "Connection failed",
  tunnel_id: "a",
};

describe("log filters", () => {
  it("shows every severity by default and includes fatal under Error", () => {
    expect(logLevels([])).toEqual(["debug", "info", "warn", "error", "fatal"]);
    expect(logLevels(["warn", "error"])).toEqual(["warn", "error", "fatal"]);
    expect(matchesLogFilters(line, null, logLevels(["error"]), "")).toBe(true);
  });
  it("combines scope, severity and trimmed case-insensitive text", () => {
    expect(matchesLogFilters(line, "a", logLevels(["error"]), " FAILED ")).toBe(
      true
    );
    expect(matchesLogFilters(line, "b", logLevels([]), "")).toBe(false);
    expect(matchesLogFilters(line, null, logLevels(["info"]), "")).toBe(false);
    expect(matchesLogFilters(line, null, logLevels([]), "unknown")).toBe(false);
  });
  it("searches fields independently of the message, as the history query does", () => {
    expect(matchesLogFilters(line, null, logLevels([]), "TIMEOUT")).toBe(true);
    expect(
      matchesLogFilters(
        { ...line, fields: "bar", message: "foo" },
        null,
        logLevels([]),
        "oob"
      )
    ).toBe(false);
  });
});
