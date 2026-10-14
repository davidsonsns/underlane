import { describe, expect, it } from "vitest";
import { matchesRequestFilters } from "./request-filters";

describe("request filters", () => {
  it("allows unfiltered requests, including exchanges without a response", () => {
    expect(matchesRequestFilters({ status: null, method: "GET" }, [], [])).toBe(true);
  });
  it("matches any selected value within each category and requires both categories", () => {
    const statuses = [200, 502], methods = ["GET", "POST"];
    expect(matchesRequestFilters({ status: 200, method: "GET" }, statuses, methods)).toBe(true);
    expect(matchesRequestFilters({ status: 502, method: "POST" }, statuses, methods)).toBe(true);
    expect(matchesRequestFilters({ status: 404, method: "GET" }, statuses, methods)).toBe(false);
    expect(matchesRequestFilters({ status: 200, method: "DELETE" }, statuses, methods)).toBe(false);
    expect(matchesRequestFilters({ status: null, method: "POST" }, statuses, methods)).toBe(false);
  });
  it("keeps categories independent and method matching exact", () => {
    expect(matchesRequestFilters({ status: 502, method: "PUT" }, [502], [])).toBe(true);
    expect(matchesRequestFilters({ status: null, method: "PATCH" }, [], ["PATCH"])).toBe(true);
    expect(matchesRequestFilters({ status: 200, method: "post" }, [], ["POST"])).toBe(false);
  });
});
