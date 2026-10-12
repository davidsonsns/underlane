<script lang="ts">
  // Underlane's arch, the brand mark (02 §1.3): lucide has no equivalent. Used by StatePanel's
  // empty state (40px) and, from Phase 2h, the Tunnels nav item. Colour is currentColor.
  import { cn } from "$lib/utils";
  let { size = 40, strokeWidth = 2.5, class: className }: { size?: number; strokeWidth?: number; class?: string } = $props();
</script>

<svg
  class={cn("fill-none stroke-current", className)}
  width={size}
  height={size}
  viewBox="0 0 48 48"
  stroke-width={strokeWidth}
  stroke-linecap="round"
  aria-hidden="true"><path d="M12 38V24a12 12 0 0 1 24 0v14M6 38h36" /></svg>
