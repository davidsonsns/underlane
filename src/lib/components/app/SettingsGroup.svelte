<script lang="ts">
  // A titled card of SettingsRows with hairline dividers (Settings, TunnelDetail).
  import type { Snippet } from "svelte";
  import { cn } from "$lib/utils";

  let {
    title,
    aside,
    footer,
    class: className,
    children,
  }: {
    title?: string;
    /** Right-aligned next to the title (e.g. "Local service running"). */
    aside?: Snippet;
    /** Small note under the card. */
    footer?: Snippet;
    class?: string;
    children: Snippet;
  } = $props();
  const id = $props.id();
</script>

<section class={className} aria-labelledby={title ? `${id}-title` : undefined}>
  {#if title || aside}
    <div class="mt-5 mb-2 ms-0.5 flex items-baseline justify-between gap-3">
      {#if title}<h2 id="{id}-title" class="text-sm font-semibold">{title}</h2>{/if}
      {#if aside}<div class="text-xs text-muted-foreground">{@render aside()}</div>{/if}
    </div>
  {/if}
  <div class={cn("rounded-lg bg-card border-[0.5px] border-border *:not-first:border-t-[0.5px] *:not-first:border-border")}>
    {@render children()}
  </div>
  {#if footer}<div class="mt-1.5 ms-0.5 text-xs text-muted-foreground">{@render footer()}</div>{/if}
</section>
