<script lang="ts">
  import { cn } from "$lib/utils";
  import logo from "../../../../src-tauri/icons/logo.svg?raw";

  let { size = 40, class: className }: { size?: number; class?: string } =
    $props();
</script>

<!-- The SVG is a bundled, static brand asset; currentColor follows the surrounding UI. -->
<span
  aria-hidden="true"
  class={cn("inline-flex shrink-0", className)}
  style:height="{size}px"
  style:width="{size}px"
  >{@html logo}</span
>
