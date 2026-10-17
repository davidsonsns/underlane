import { execFileSync } from "node:child_process";
import { mkdirSync } from "node:fs";
import { join } from "node:path";
import { captureWindow, sleep } from "./harness.mjs";

const EXTRA = /^(\d+) x=(-?\d+) w=(\d+)/;
export const SHOTS = "e2e/out/screenshots";
mkdirSync(SHOTS, { recursive: true });
export const sh = (command, args) =>
  execFileSync(command, args).toString().trim();
export const windows = () =>
  sh("swift", ["scripts/window-bounds.swift", "Underlane"])
    .split("\n")
    .filter(Boolean)
    .map((line) => JSON.parse(line));
function extras(application) {
  return sh("swift", ["scripts/menubar-extras.swift", application])
    .split("\n")
    .map((line) => line.match(EXTRA))
    .filter(Boolean)
    .map((match) => ({ i: +match[1], w: +match[3], x: +match[2] }));
}
export const click = (x, y) =>
  execFileSync("swift", [
    "scripts/click.swift",
    String(Math.round(x)),
    String(Math.round(y)),
  ]);
export const capture = (name, min = 300, max = 4000) =>
  captureWindow(join(SHOTS, name), min, max);
function safeExtras(application) {
  try {
    return extras(application);
  } catch {
    return [];
  }
}
export function toggleHiddenBar(bestEffort = false) {
  const [chevron] = (bestEffort ? safeExtras : extras)("Hidden Bar");
  if (!chevron) {
    return false;
  }
  click(chevron.x + chevron.w / 2, 12);
  return true;
}
export function revealMenuItem({ bestEffort = false } = {}) {
  const readExtras = bestEffort ? safeExtras : extras;
  const [item] = readExtras("Underlane");
  if (item && item.x >= 0) {
    return { expanded: false, item };
  }
  return expandMenuItem(readExtras, bestEffort, item);
}
async function expandMenuItem(readExtras, bestEffort, previousItem) {
  const expanded = toggleHiddenBar(bestEffort);
  if (!expanded) {
    return { expanded, item: previousItem };
  }
  try {
    await sleep(1200);
    const [item] = readExtras("Underlane");
    return { expanded, item };
  } catch (error) {
    toggleHiddenBar(bestEffort);
    throw error;
  }
}

export function captureMenuStrip(item, file) {
  execFileSync("screencapture", [
    "-x",
    "-R",
    `${Math.max(0, item.x - 110)},0,${item.w + 220},30`,
    file,
  ]);
}

export async function openDashboard() {
  const popover = windows().find((window) => window.w <= 420);
  click(popover.x + 80, popover.y + popover.h - 20);
  await sleep(2500);
  return windows().find((window) => window.w > 700);
}
