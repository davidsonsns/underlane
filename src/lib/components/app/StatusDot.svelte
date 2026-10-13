<script lang="ts">
  import type { Status } from "$lib/types";
  import { statusLabel } from "$lib/format";
  import { cn } from "$lib/utils";
  // The hint is the caller's: wrap the dot in a Tip (no tab stop; the status word is text nearby):
  //   <Tip text={statusHint[status]}>{#snippet children({ props })}<span {...{ ...props, tabindex: undefined }} class="flex"><StatusDot {status} /></span>{/snippet}</Tip>
  let { status, size = 8 }: { status: Status; size?: number } = $props();

  const look: Record<Status, string> = {
    stopped: "bg-transparent border border-off",
    connected: "bg-ok shadow-[0_0_0_3px_color-mix(in_srgb,var(--ok)_18%,transparent)]",
    degraded: "bg-degraded",
    crashed: "bg-bad",
    // Starting breathes; the only ambient motion in the app, and it stops once connected.
    starting: "bg-starting after:absolute after:inset-0 after:animate-status-ping after:rounded-full after:bg-starting",
  };
</script>

<span
  class={cn(
    "relative inline-block size-(--size) flex-none rounded-full bg-off",
    // One-off: each property has its own easing.
    "[transition:background-color_240ms_ease,box-shadow_240ms_ease,transform_240ms_var(--ease-spring)]",
    look[status]
  )}
  style="--size: {size}px"
  role="img"
  aria-label={statusLabel[status]}
></span>
