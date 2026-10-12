<script lang="ts">
  import logo from "../../../../src-tauri/icons/logo.svg?raw";
  import { cn } from "$lib/utils";
  let { size = 40, class: className }: { size?: number; class?: string } = $props();
</script>

<!-- The SVG is a bundled, static brand asset; currentColor follows the surrounding UI. -->
<span class={cn("inline-flex shrink-0", className)} style:width="{size}px" style:height="{size}px" aria-hidden="true">{@html logo}</span>
