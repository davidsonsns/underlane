const FILENAME_CHARACTER = /[\p{Alphabetic}\p{Number}]/u;
const TRAILING_DASHES = /-+$/;
const RESERVED_FILENAME = /^(con|prn|aux|nul|com[1-9]|lpt[1-9])$/;
/** Display the same filename as the backend's project draft writer; creation remains authoritative. */
export function projectFilename(name: string): string {
  let safe = "";
  let bytes = 0;
  const encoder = new TextEncoder();
  for (const character of Array.from(name.trim())
    .map((c) => c.toLowerCase())
    .join("")) {
    if (FILENAME_CHARACTER.test(character)) {
      const size = encoder.encode(character).length;
      if (bytes + size > 120) {
        break;
      }
      safe += character;
      bytes += size;
    } else if (safe && !safe.endsWith("-") && bytes < 120) {
      safe += "-";
      bytes += 1;
    }
  }
  safe = safe.replace(TRAILING_DASHES, "") || "project";
  if (RESERVED_FILENAME.test(safe)) {
    safe = `project-${safe}`;
  }
  return `${safe}.yml`;
}
