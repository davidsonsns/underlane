import type { Settings } from "./types";

/** Apply light/dark/system. Snaps like AppKit does: the native vibrancy material flips
 *  instantly on set_theme, so a CSS cross-fade would briefly disagree with it. */
export function applyTheme(theme: Settings["theme"]) {
  const root = document.documentElement;
  if (theme === "system") delete root.dataset.theme;
  else root.dataset.theme = theme;
}

/** Sets html[data-kb-nav] once the user presses Tab, clears it on any pointer use. The
 *  `kb-focus:` variant keys off it so buttons only show a focus ring during keyboard
 *  navigation, as on macOS (text fields always show theirs). Call once per window. */
export function installFocusModality(root = document.documentElement) {
  addEventListener("keydown", (e) => e.key === "Tab" && (root.dataset.kbNav = ""), true);
  addEventListener("pointerdown", () => delete root.dataset.kbNav, true);
}

const reducedMotion = () => matchMedia("(prefers-reduced-motion: reduce)").matches;

/** Duration helper for Svelte transitions that respects reduce motion. */
export const motion = (ms: number) => (reducedMotion() ? 0 : ms);
