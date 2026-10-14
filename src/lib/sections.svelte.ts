// Which sections of the request detail (Headers, Body) are collapsed. Shared by both tabs and
// remembered per window in localStorage, a convenience: if storage is unavailable every section opens.

const SECTIONS = ["headers", "body"] as const;
export type SectionId = (typeof SECTIONS)[number];

const KEY = "underlane.requests.collapsed-sections";

function load(): SectionId[] {
  try {
    const v: unknown = JSON.parse(localStorage.getItem(KEY) ?? "[]");
    if (Array.isArray(v)) {
      return SECTIONS.filter((s) => v.includes(s));
    }
  } catch {
    // Storage may be unavailable; keep the in-memory value.
  }
  return [];
}

export const sectionPrefs = $state({ collapsed: load() });

export function setOpen(id: SectionId, open: boolean) {
  const rest = sectionPrefs.collapsed.filter((s) => s !== id);
  sectionPrefs.collapsed = open ? rest : [...rest, id];
  try {
    localStorage.setItem(KEY, JSON.stringify(sectionPrefs.collapsed));
  } catch {
    // Storage may be unavailable; keep the in-memory value.
  }
}
