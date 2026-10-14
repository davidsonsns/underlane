import { describe, expect, it, vi } from "vitest";
import {
  appendLogWindow,
  logKeyAction,
  prependRequestWindow,
  requestKeyAction,
  scrollFollowState,
} from "./list-state";

const key = (value: string, modifiers = {}) => ({
  altKey: false,
  ctrlKey: false,
  key: value,
  metaKey: false,
  ...modifiers,
});
const rows = (...ids: string[]) => ids.map((id) => ({ id }));

describe("virtual windows retain the viewed rows until the user reaches the live edge", () => {
  it("holds bounded logs beyond the ceiling and flushes the newest cap on follow", () => {
    const paused = appendLogWindow([1, 2, 3], [4], [5, 6, 7], false, 3, 4);
    expect(paused).toEqual({ held: [4, 6, 7], lines: [1, 2, 3, 5] });
    expect(appendLogWindow(paused.lines, paused.held, [8], true, 3, 4)).toEqual(
      {
        held: [],
        lines: [6, 7, 8],
      }
    );
  });
  it("preserves an unchanged log window when no lines arrive or no space remains", () => {
    const lines = [1, 2],
      held = [3];
    expect(appendLogWindow(lines, held, [], true, 2, 2).lines).toBe(lines);
    expect(appendLogWindow(lines, held, [4], false, 2, 2)).toEqual({
      held: [3, 4],
      lines,
    });
  });
  it("replaces duplicate requests with newest data and caps the live edge", () => {
    const result = prependRequestWindow(
      rows("b", "a"),
      rows("c", "b"),
      0,
      false,
      2,
      4,
      28
    );
    expect(result).toEqual({
      anchor: null,
      rows: rows("c", "b"),
      shift: false,
      wait: false,
    });
  });
  it("uses virtua shift below the ceiling, then preserves the fractional row anchor", () => {
    expect(
      prependRequestWindow(rows("b", "a"), rows("c"), 30, false, 2, 4, 28)
    ).toEqual({
      anchor: null,
      rows: rows("c", "b", "a"),
      shift: true,
      wait: true,
    });
    expect(
      prependRequestWindow(rows("b", "a"), rows("d", "c"), 30, false, 2, 3, 28)
    ).toEqual({
      anchor: 86,
      rows: rows("d", "c", "b"),
      shift: false,
      wait: true,
    });
  });
  it("awaits the rendered trim even if an outdated viewport has no anchor", () => {
    const result = prependRequestWindow(
      rows("a"),
      rows("c", "b"),
      100,
      false,
      1,
      2,
      28
    );
    expect(result.anchor).toBeNull();
    expect(result.wait).toBe(true);
    expect(
      prependRequestWindow(rows("a"), rows("b"), null, false, 1, 2, 28).wait
    ).toBe(false);
    expect(
      prependRequestWindow(rows("a"), rows("b"), 28, true, 1, 2, 28).wait
    ).toBe(false);
  });
});

describe("composite list keyboard semantics", () => {
  it("starts logs at the visible edge and expands only an active row", () => {
    const edge = vi.fn((last: boolean) => (last ? 7 : 3));
    expect(logKeyAction(key("ArrowUp"), -1, 10, 4, edge)).toEqual({
      index: 7,
      type: "move",
    });
    expect(logKeyAction(key("PageDown"), -1, 10, 4, edge)).toEqual({
      index: 3,
      type: "move",
    });
    expect(logKeyAction(key("Enter"), -1, 10, 4, edge)).toBeNull();
    expect(logKeyAction(key("ArrowRight"), 3, 10, 4, edge)).toEqual({
      index: 3,
      open: true,
      type: "fields",
    });
    expect(logKeyAction(key("ArrowLeft"), 3, 10, 4, edge)).toEqual({
      index: 3,
      open: false,
      type: "fields",
    });
  });
  it("lets modifiers/unrelated keys through and preserves empty-list navigation", () => {
    const edge = vi.fn(() => 0);
    expect(
      logKeyAction(key("ArrowUp", { ctrlKey: true }), 3, 10, 4, edge)
    ).toBeNull();
    expect(logKeyAction(key("x"), 3, 10, 4, edge)).toBeNull();
    expect(logKeyAction(key("ArrowUp"), -1, 0, 4, edge)).toBeNull();
    expect(logKeyAction(key("End"), -1, 0, 4, edge)).toEqual({
      index: -1,
      type: "move",
    });
    expect(edge).not.toHaveBeenCalled();
    expect(requestKeyAction(key("Escape"), true, 3, 10, 4)).toEqual({
      type: "clear",
    });
    expect(requestKeyAction(key("Escape"), false, -1, 10, 4)).toBeNull();
    expect(
      requestKeyAction(key("End", { metaKey: true }), true, 3, 10, 4)
    ).toBeNull();
    expect(requestKeyAction(key("PageDown"), true, 8, 10, 4)).toEqual({
      index: 9,
      type: "move",
    });
  });
});

it("stops log following only after accumulated upward movement, ignoring trim jumps", () => {
  expect(scrollFollowState(false, false, 0, 100, 99)).toEqual({
    stop: false,
    up: 1,
  });
  expect(scrollFollowState(false, false, 1, 99, 98)).toEqual({
    stop: true,
    up: 2,
  });
  expect(scrollFollowState(false, true, 0, 100, 50)).toEqual({
    stop: false,
    up: 0,
  });
  expect(scrollFollowState(true, false, 2, 100, 100)).toEqual({
    stop: false,
    up: 0,
  });
});
