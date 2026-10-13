/** Display the same filename as the backend's project draft writer; creation remains authoritative. */
export function projectFilename(name: string): string {
  let safe = "";
  let bytes = 0;
  const encoder = new TextEncoder();
  for (const character of Array.from(name.trim()).map((c) => c.toLowerCase()).join("")) {
    if (/[\p{Alphabetic}\p{Number}]/u.test(character)) {
      const size = encoder.encode(character).length;
      if (bytes + size > 120) break;
      safe += character;
      bytes += size;
    } else if (safe && !safe.endsWith("-") && bytes < 120) {
      safe += "-";
      bytes++;
    }
  }
  safe = safe.replace(/-+$/, "") || "project";
  if (/^(con|prn|aux|nul|com[1-9]|lpt[1-9])$/.test(safe)) safe = `project-${safe}`;
  return `${safe}.yml`;
}
