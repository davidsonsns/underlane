import { describe, expect, it } from "vitest";
import { next } from "./listnav";

describe("next", () => {
  it("moves by one with the arrows and clamps at the ends", () => {
    expect(next("ArrowDown", 3, 10)).toBe(4);
    expect(next("ArrowUp", 3, 10)).toBe(2);
    expect(next("ArrowDown", 9, 10)).toBe(9);
    expect(next("ArrowUp", 0, 10)).toBe(0);
  });
  it("moves by a page and clamps", () => {
    expect(next("PageDown", 2, 100, 10)).toBe(12);
    expect(next("PageUp", 12, 100, 10)).toBe(2);
    expect(next("PageDown", 95, 100, 10)).toBe(99);
    expect(next("PageUp", 4, 100, 10)).toBe(0);
  });
  it("treats a page smaller than one row as one row", () => {
    expect(next("PageDown", 2, 10, 0)).toBe(3);
    expect(next("PageDown", 2, 10, 0.4)).toBe(3);
    expect(next("PageUp", 2, 10)).toBe(1);
  });
  it("goes to the ends with Home and End", () => {
    expect(next("Home", 7, 10)).toBe(0);
    expect(next("End", 2, 10)).toBe(9);
    expect(next("End", -1, 10)).toBe(9);
  });
  it("starts at the first row when nothing is selected", () => {
    for (const k of ["ArrowDown", "ArrowUp", "PageDown", "PageUp", "Home"]) {
      expect(next(k, -1, 10, 5)).toBe(0);
    }
  });
  it("clamps an index that is past the end (the list shrank)", () => {
    expect(next("ArrowUp", 20, 10)).toBe(9);
    expect(next("ArrowDown", 20, 10)).toBe(9);
  });
  it("gives -1 for an empty list", () => {
    for (const k of [
      "ArrowDown",
      "ArrowUp",
      "PageDown",
      "PageUp",
      "Home",
      "End",
    ]) {
      expect(next(k, 0, 0, 5)).toBe(-1);
    }
    expect(next("ArrowDown", -1, 0)).toBe(-1);
  });
  it("returns null for keys it doesn't handle", () => {
    expect(next("Enter", 3, 10)).toBeNull();
    expect(next("ArrowLeft", 3, 10)).toBeNull();
    expect(next("a", 3, 10)).toBeNull();
  });
});
