import type { LogLine } from "./types";

export function logLevels(selected: readonly string[]): string[] {
  return selected.length
    ? selected.flatMap((level) =>
        level === "error" ? ["error", "fatal"] : [level]
      )
    : ["debug", "info", "warn", "error", "fatal"];
}

export function matchesLogFilters(
  line: Pick<LogLine, "tunnel_id" | "level" | "message" | "fields">,
  tunnelId: string | null,
  levels: readonly string[],
  search: string
): boolean {
  const query = search.trim().toLowerCase();
  return (
    (!tunnelId || line.tunnel_id === tunnelId) &&
    levels.includes(line.level) &&
    (!query ||
      line.message.toLowerCase().includes(query) ||
      line.fields.toLowerCase().includes(query))
  );
}
