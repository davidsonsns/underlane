// The Combobox's rows for a search (02 F6). Matching is a case-insensitive substring of the
// trimmed text (Bits' fuzzy scorer hid "Work" for " Work" and picked "website" for "web").
// An exact match comes first, so Enter picks it; otherwise a non-empty search offers to create
// the trimmed text, shown first, with the substring matches after it.
export function comboRows(
  items: string[],
  search: string
): { create: string | null; matches: string[] } {
  const typed = search.trim();
  const q = typed.toLowerCase();
  if (!q) {
    return { create: null, matches: items };
  }
  const matches = items.filter((x) => x.toLowerCase().includes(q));
  const exact = matches.find((x) => x.toLowerCase() === q);
  if (exact === undefined) {
    return { create: typed, matches };
  }
  return {
    create: null,
    matches: [exact, ...matches.filter((x) => x !== exact)],
  };
}
