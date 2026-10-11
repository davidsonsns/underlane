// Keyboard movement in a single-selection list (Requests, Logs, the popover rows).
// Pure and dependency-free: the popover imports it too, so keep it tiny.

/**
 * The index a key moves to, or null when the key isn't a list key (let it through).
 * Up/Down move by 1, PageUp/PageDown by `pageSize`, Home/End go to the ends; the result is
 * clamped to the list. From no selection (index < 0) Down, PageDown and Home go to the first
 * row, Up and PageUp too. An empty list gives -1.
 */
export function next(key: string, index: number, count: number, pageSize = 1): number | null {
  const page = Math.max(1, Math.floor(pageSize));
  let i: number;
  switch (key) {
    case "ArrowDown": i = index < 0 ? 0 : index + 1; break;
    case "ArrowUp": i = index < 0 ? 0 : index - 1; break;
    case "PageDown": i = index < 0 ? 0 : index + page; break;
    case "PageUp": i = index < 0 ? 0 : index - page; break;
    case "Home": i = 0; break;
    case "End": i = count - 1; break;
    default: return null;
  }
  return count <= 0 ? -1 : Math.min(count - 1, Math.max(0, i));
}
