<script lang="ts">
  import type { Status } from "$lib/types";
  import { statusHint, statusLabel } from "$lib/format";
  import { tooltip } from "$lib/tooltip";
  import { cn } from "$lib/utils";
  // Ported screens put the hint in a Tip around the dot and pass `hint={false}`:
  //   <Tip text={statusHint[status]}>{#snippet children({ props })}<span {...{ ...props, tabindex: undefined }}><StatusDot {status} hint={false} /></span>{/snippet}</Tip>
  // The popover keeps use:tooltip until 2i: Tip in here would put ui/tooltip (floating-ui,
  // ~20 KB) into the popover, past its JS budget.
  let { status, size = 8, hint = true }: { status: Status; size?: number; hint?: boolean } = $props();

  const look: Record<Status, string> = {
    stopped: "bg-transparent shadow-[inset_0_0_0_1.5px_var(--off)]",
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
  use:tooltip={hint ? statusHint[status] : undefined}
></span>
