import type { Settings } from "./types";

/** Apply light/dark/system with a short cross-fade (skipped under reduce motion). */
export function applyTheme(theme: Settings["theme"], animate = true) {
  const root = document.documentElement;
  const body = document.body;
  const reduce = matchMedia("(prefers-reduced-motion: reduce)").matches;
  if (animate && !reduce) {
    body.classList.add("theme-transition");
    setTimeout(() => body.classList.remove("theme-transition"), 300);
  }
  if (theme === "system") delete root.dataset.theme;
  else root.dataset.theme = theme;
}

export const reducedMotion = () => matchMedia("(prefers-reduced-motion: reduce)").matches;

/** Duration helper for Svelte transitions that respects reduce motion. */
export const motion = (ms: number) => (reducedMotion() ? 0 : ms);
