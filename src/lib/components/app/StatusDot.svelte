<script lang="ts">
  // Status colour as a dot. The hint is a Tip at the call site, with the dot as its trigger
  // (it takes the trigger props):
  //   <Tip text={statusHint[status]}>{#snippet children({ props })}<StatusDot {status} {...props} tabindex={undefined} legacyTip={false} />{/snippet}</Tip>
  // Tip can't live in here yet: the popover renders StatusDot too, and importing ui/tooltip
  // there adds floating-ui (~20 KB) past its JS budget. Until Phase 2i solves that, the
  // unported screens (popover, Tunnels list) keep the old use:tooltip hint (legacyTip).
  import type { HTMLAttributes } from "svelte/elements";
  import type { Status } from "$lib/types";
  import { statusHint, statusLabel } from "$lib/format";
  import { tooltip } from "$lib/tooltip";
  import { cn } from "$lib/utils";
  let {
    status,
    size = 8,
    legacyTip = true,
    class: className,
    ...rest
  }: HTMLAttributes<HTMLSpanElement> & { status: Status; size?: number; legacyTip?: boolean } = $props();

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
  {...rest}
  class={cn(
    "relative inline-block size-(--size) flex-none rounded-full bg-off",
    // One-off: each property has its own easing.
    "[transition:background-color_240ms_ease,box-shadow_240ms_ease,transform_240ms_var(--ease-spring)]",
    look[status],
    className
  )}
  style="--size: {size}px"
  role="img"
  aria-label={statusLabel[status]}
  use:tooltip={legacyTip ? statusHint[status] : undefined}
></span>
