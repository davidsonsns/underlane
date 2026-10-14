// The Requests list's columns and which of them show (the Columns popover in Requests.svelte).
// Hidden columns are remembered per window in localStorage, a convenience: if storage is
// unavailable every column shows. Path always shows; it's the one flexible column.

export const COLUMNS = [
  { id: "method", label: "Method", width: "46px" },
  { id: "route", label: "Route", width: "minmax(0,0.7fr)" },
  { always: true, id: "path", label: "Path", width: "minmax(60px,1fr)" },
  { end: true, id: "status", label: "Status", width: "44px" },
  { end: true, id: "time", label: "Time", width: "56px" },
  { end: true, id: "size", label: "Size", width: "56px" },
  { end: true, id: "at", label: "At", width: "" },
] as const satisfies readonly {
  id: string;
  label: string;
  width: string;
  always?: boolean;
  end?: boolean;
}[];

export type ColumnId = (typeof COLUMNS)[number]["id"];

const KEY = "underlane.requests.hidden-columns";

function load(): ColumnId[] {
  try {
    const v: unknown = JSON.parse(localStorage.getItem(KEY) ?? "[]");
    if (Array.isArray(v)) {
      return COLUMNS.filter((c) => !("always" in c) && v.includes(c.id)).map(
        (c) => c.id
      );
    }
  } catch {
    // Storage may be unavailable; keep the in-memory value.
  }
  return [];
}

export const columnPrefs = $state({ hidden: load() });

export function setHidden(ids: ColumnId[]) {
  columnPrefs.hidden = ids;
  try {
    localStorage.setItem(KEY, JSON.stringify(ids));
  } catch {
    // Storage may be unavailable; keep the in-memory value.
  }
}

/** The grid-template-columns shared by the header and the rows. `atChars` is the longest At
 *  label in the list, so the column widens only while it shows a date. */
export function gridTemplate(
  hidden: readonly ColumnId[],
  atChars: number
): string {
  return COLUMNS.filter((c) => !hidden.includes(c.id))
    .map((c) => (c.id === "at" ? `${atChars}ch` : c.width))
    .join(" ");
}
