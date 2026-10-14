import { describe, expect, it } from "vitest";
import { comboRows } from "./combobox";

const groups = ["Default", "Work", "website", "Homework"];

describe("comboRows", () => {
  it("lists every item, in order, for an empty or blank search", () => {
    expect(comboRows(groups, "")).toEqual({ create: null, matches: groups });
    expect(comboRows(groups, "   ")).toEqual({ create: null, matches: groups });
  });
  it("puts an exact match (trimmed, any case) first and offers no Create", () => {
    expect(comboRows(groups, " Work ")).toEqual({
      create: null,
      matches: ["Work", "Homework"],
    });
    expect(comboRows(groups, "WORK")).toEqual({
      create: null,
      matches: ["Work", "Homework"],
    });
  });
  it("offers Create for text that only matches as a substring", () => {
    expect(comboRows(groups, "web")).toEqual({
      create: "web",
      matches: ["website"],
    });
    expect(comboRows(groups, "  Wor")).toEqual({
      create: "Wor",
      matches: ["Work", "Homework"],
    });
  });
  it("matches substrings, not fuzzy subsequences", () => {
    expect(comboRows(groups, "wbs")).toEqual({ create: "wbs", matches: [] });
  });
  it("offers Create when there are no items", () => {
    expect(comboRows([], "Staging")).toEqual({
      create: "Staging",
      matches: [],
    });
  });
});
