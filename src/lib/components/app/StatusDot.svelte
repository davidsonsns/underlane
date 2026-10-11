<script lang="ts">
  import type { Status } from "$lib/types";
  import { statusHint, statusLabel } from "$lib/format";
  import { tooltip } from "$lib/tooltip";
  let { status, size = 8 }: { status: Status; size?: number } = $props();
</script>

<span class="dot {status}" style="--size: {size}px" role="img" aria-label={statusLabel[status]} use:tooltip={statusHint[status]}></span>

<style>
  .dot {
    position: relative;
    display: inline-block;
    flex: none;
    width: var(--size);
    height: var(--size);
    border-radius: 50%;
    background: var(--off);
    transition: background-color 240ms ease, box-shadow 240ms ease, transform 240ms var(--ease-spring);
  }
  .stopped {
    background: transparent;
    box-shadow: inset 0 0 0 1.5px var(--off);
  }
  .connected {
    background: var(--ok);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--ok) 18%, transparent);
  }
  .degraded {
    background: var(--degraded);
  }
  .crashed {
    background: var(--bad);
  }
  .starting {
    background: var(--starting);
  }
  /* Starting breathes; the only ambient motion in the app, and it stops once connected. */
  .starting::after {
    content: "";
    position: absolute;
    inset: 0;
    border-radius: 50%;
    background: var(--starting);
    animation: ping 1.3s var(--ease-out) infinite;
  }
  @keyframes ping {
    0% { transform: scale(1); opacity: 0.6; }
    100% { transform: scale(2.6); opacity: 0; }
  }
</style>
