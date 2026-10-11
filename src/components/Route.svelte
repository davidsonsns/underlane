<script lang="ts">
  // hostname ⟶ local service, joined by a conduit. When a request flows through an inspected
  // route, a pulse travels along the conduit (motion that answers real activity).
  import { shortService } from "../lib/format";
  let {
    hostname,
    service,
    pulse = 0,
    inspected = false,
    compact = false,
  }: { hostname: string | null; service: string; pulse?: number; inspected?: boolean; compact?: boolean } = $props();
  let key = $derived(pulse);
</script>

<span class="route" class:compact title="{hostname ?? 'any hostname'} → {service}">
  <span class="host mono">{hostname ?? "*"}</span>
  <span class="conduit" class:inspected aria-hidden="true">
    {#key key}
      {#if pulse > 0}<span class="pulse"></span>{/if}
    {/key}
  </span>
  <span class="svc mono">{shortService(service, compact)}</span>
</span>

<style>
  .route {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    font-size: 11.5px;
  }
  .host {
    color: var(--text-1);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
    flex: 0 1 auto;
    font-size: inherit;
  }
  .svc {
    color: var(--text-2);
    white-space: nowrap;
    flex: none;
    font-size: inherit;
  }
  .conduit {
    position: relative;
    flex: 1 0 18px;
    max-width: 56px;
    height: 1px;
    background: var(--line-strong);
    overflow: visible;
  }
  .conduit::after {
    content: "";
    position: absolute;
    right: -1px;
    top: -2.5px;
    border: 3px solid transparent;
    border-left: 4px solid var(--line-strong);
    border-right: 0;
  }
  .conduit.inspected {
    background: repeating-linear-gradient(90deg, var(--text-3) 0 3px, transparent 3px 5px);
  }
  .pulse {
    position: absolute;
    top: -2px;
    left: 0;
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--tint);
    box-shadow: 0 0 6px var(--tint);
    animation: travel 650ms var(--ease-out) forwards;
  }
  @keyframes travel {
    from { left: 0; opacity: 1; }
    to { left: calc(100% - 5px); opacity: 0; }
  }
  .compact {
    font-size: 11px;
  }
  .compact .conduit {
    flex-basis: 14px;
    max-width: 26px;
  }
  @media (prefers-reduced-motion: reduce) {
    .pulse { display: none; }
  }
</style>
