import { tick } from "svelte";

/** Search inputs bypass menu typeahead; arrows enter the submenu's selectable options. */
export function filterSearchKeys(event: KeyboardEvent) {
  if (event.key === "ArrowDown" || event.key === "ArrowUp") {
    const items = (event.currentTarget as HTMLElement)
      .closest('[role="menu"]')
      ?.querySelectorAll<HTMLElement>(
        '[role="menuitemcheckbox"], [role="menuitemradio"]'
      );
    const item =
      event.key === "ArrowDown" ? items?.[0] : items?.[items.length - 1];
    if (item) {
      event.preventDefault();
      event.stopPropagation();
      item.focus();
    }
    return;
  }
  if (!["Escape", "Tab"].includes(event.key)) {
    event.stopPropagation();
  }
}
export async function focusFilterSearch(
  event: Event,
  getInput: () => HTMLInputElement | null
) {
  event.preventDefault();
  await tick();
  // Bits focuses its first option after the mount tick; search owns focus on the next frame.
  requestAnimationFrame(() => getInput()?.focus());
}
