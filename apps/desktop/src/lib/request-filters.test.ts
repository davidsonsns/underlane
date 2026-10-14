import { describe, expect, it } from "vitest";
import { matchesRequestFilters } from "./request-filters";

describe("request filters", () => {
  it("allows unfiltered requests, including exchanges without a response", () => {
    expect(matchesRequestFilters({ method: "GET", status: null }, [], [])).toBe(
      true
    );
  });
  it("matches any selected value within each category and requires both categories", () => {
    const statuses = [200, 502],
      methods = ["GET", "POST"];
    expect(
      matchesRequestFilters({ method: "GET", status: 200 }, statuses, methods)
    ).toBe(true);
    expect(
      matchesRequestFilters({ method: "POST", status: 502 }, statuses, methods)
    ).toBe(true);
    expect(
      matchesRequestFilters({ method: "GET", status: 404 }, statuses, methods)
    ).toBe(false);
    expect(
      matchesRequestFilters(
        { method: "DELETE", status: 200 },
        statuses,
        methods
      )
    ).toBe(false);
    expect(
      matchesRequestFilters({ method: "POST", status: null }, statuses, methods)
    ).toBe(false);
  });
  it("keeps categories independent and method matching exact", () => {
    expect(
      matchesRequestFilters({ method: "PUT", status: 502 }, [502], [])
    ).toBe(true);
    expect(
      matchesRequestFilters({ method: "PATCH", status: null }, [], ["PATCH"])
    ).toBe(true);
    expect(
      matchesRequestFilters({ method: "post", status: 200 }, [], ["POST"])
    ).toBe(false);
  });
});
