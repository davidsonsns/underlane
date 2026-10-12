<script lang="ts">
  // hostname ⟶ local service, joined by a conduit. When a request flows through an inspected
  // route, a pulse travels along the conduit (motion that answers real activity).
  import { shortService } from "$lib/format";
  import { cn } from "$lib/utils";
  let {
    hostname,
    service,
    pulse = 0,
    inspected = false,
    compact = false,
  }: { hostname: string | null; service: string; pulse?: number; inspected?: boolean; compact?: boolean } = $props();
  let key = $derived(pulse);
</script>

<span class={cn("flex min-w-0 items-center gap-1.5", compact ? "text-xs" : "text-code")} title="{hostname ?? 'any hostname'} → {service}">
  <span class="min-w-0 flex-[0_1_auto] cursor-text truncate font-mono text-foreground select-text">{hostname ?? "*"}</span>
  <span
    class={cn(
      "relative h-px max-w-14 flex-[1_0_18px] overflow-visible",
      // Arrowhead: a 4px-wide triangle at the end of the line.
      "after:absolute after:-top-[2.5px] after:-right-px after:border-[3px] after:border-r-0 after:border-l-4 after:border-transparent after:border-l-(color:--line-strong)",
      compact && "max-w-[26px] basis-3.5",
      // Inspected: dashed conduit (3px dash, 2px gap) instead of the solid line.
      inspected ? "bg-[repeating-linear-gradient(90deg,var(--text-3)_0_3px,transparent_3px_5px)]" : "bg-(color:--line-strong)"
    )}
    aria-hidden="true"
  >
    {#key key}
      {#if pulse > 0}<span class="absolute -top-0.5 left-0 size-[5px] animate-travel rounded-full bg-tint shadow-[0_0_6px_var(--tint)] motion-reduce:hidden"></span>{/if}
    {/key}
  </span>
  <span class="flex-none cursor-text font-mono whitespace-nowrap text-muted-foreground select-text">{shortService(service, compact)}</span>
</span>
