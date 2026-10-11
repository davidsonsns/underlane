<script lang="ts">
  // A small set of SF Symbols-like glyphs drawn as 16px strokes.
  let { name, size = 15 }: { name: string; size?: number } = $props();
  const paths: Record<string, string> = {
    play: "M5 3.5v9l7.5-4.5z",
    stop: "M4.5 4.5h7v7h-7z",
    restart: "M12.5 8a4.5 4.5 0 1 1-1.4-3.27M12.8 2.8v2.6h-2.6",
    copy: "M5.5 5.5h7v7h-7zM3.5 10.5v-7h7",
    open: "M9 3.5h3.5V7M12.5 3.5 7 9M11 9.5v3H3.5V5h3",
    plus: "M8 3v10M3 8h10",
    trash: "M3.5 4.5h9M6.5 4.5V3h3v1.5M5 4.5l.5 8h5l.5-8",
    gear: "M8 5.6a2.4 2.4 0 1 0 0 4.8 2.4 2.4 0 0 0 0-4.8zM8 1.8v1.6M8 12.6v1.6M3.6 3.6l1.1 1.1M11.3 11.3l1.1 1.1M1.8 8h1.6M12.6 8h1.6M3.6 12.4l1.1-1.1M11.3 4.7l1.1-1.1",
    tunnel: "M3 13V8a5 5 0 0 1 10 0v5M1.5 13h13",
    list: "M5.5 4.5h8M5.5 8h8M5.5 11.5h8M2.5 4.5h.01M2.5 8h.01M2.5 11.5h.01",
    logs: "M3 3.5h10M3 6.5h7M3 9.5h10M3 12.5h5",
    search: "M7 11.5a4.5 4.5 0 1 0 0-9 4.5 4.5 0 0 0 0 9zM10.3 10.3 13.5 13.5",
    folder: "M2 4.5h4l1.2 1.5H14v6.5H2z",
    file: "M4 2.5h5l3 3v8H4zM9 2.5v3h3",
    check: "M3.5 8.5l3 3 6-7",
    warn: "M8 2.5 14 13H2zM8 6.5v3M8 11.2v.01",
    eye: "M1.5 8S4 3.5 8 3.5 14.5 8 14.5 8 12 12.5 8 12.5 1.5 8 1.5 8zM8 10a2 2 0 1 0 0-4 2 2 0 0 0 0 4z",
    bolt: "M9 1.5 3.5 9H8l-1 5.5L12.5 7H8z",
    dashboard: "M2.5 3.5h11v9h-11zM6 3.5v9",
    key: "M10.5 2.5a3 3 0 1 1-2.6 4.5L3 11.9V13.5h1.6v-1.2h1.2v-1.2h1.2l1.4-1.4",
  };
</script>

<svg width={size} height={size} viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
  <path d={paths[name] ?? ""} fill={name === "play" || name === "stop" || name === "bolt" ? "currentColor" : "none"} />
</svg>
