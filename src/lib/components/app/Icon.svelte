<script lang="ts">
  // A small set of SF Symbols-like glyphs drawn as 16px strokes. Only the popover still uses it
  // (lucide replaces it there in Phase 2i), so it keeps just the popover's four glyphs.
  let { name, size = 15 }: { name: string; size?: number } = $props();
  const paths: Record<string, string> = {
    copy: "M5.5 5.5h7v7h-7zM3.5 10.5v-7h7",
    open: "M9 3.5h3.5V7M12.5 3.5 7 9M11 9.5v3H3.5V5h3",
    warn: "M8 2.5 14 13H2zM8 6.5v3M8 11.2v.01",
    dashboard: "M2.5 3.5h11v9h-11zM6 3.5v9",
  };
</script>

<svg width={size} height={size} viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
  <path d={paths[name] ?? ""} />
</svg>
