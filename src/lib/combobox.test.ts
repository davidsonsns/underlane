import { describe, expect, it } from "vitest";
import { comboRows } from "./combobox";

const projects = ["Default", "Work", "website", "Homework"];

describe("comboRows", () => {
  it("lists every item, in order, for an empty or blank search", () => {
    expect(comboRows(projects, "")).toEqual({ create: null, matches: projects });
    expect(comboRows(projects, "   ")).toEqual({ create: null, matches: projects });
  });
  it("puts an exact match (trimmed, any case) first and offers no Create", () => {
    expect(comboRows(projects, " Work ")).toEqual({ create: null, matches: ["Work", "Homework"] });
    expect(comboRows(projects, "WORK")).toEqual({ create: null, matches: ["Work", "Homework"] });
  });
  it("offers Create for text that only matches as a substring", () => {
    expect(comboRows(projects, "web")).toEqual({ create: "web", matches: ["website"] });
    expect(comboRows(projects, "  Wor")).toEqual({ create: "Wor", matches: ["Work", "Homework"] });
  });
  it("matches substrings, not fuzzy subsequences", () => {
    expect(comboRows(projects, "wbs")).toEqual({ create: "wbs", matches: [] });
  });
  it("offers Create when there are no items", () => {
    expect(comboRows([], "Staging")).toEqual({ create: "Staging", matches: [] });
  });
});
