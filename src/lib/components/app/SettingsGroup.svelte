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
    <div class="mt-8 mb-3 ms-3.5 flex items-baseline justify-between gap-3">
      {#if title}<h2 id="{id}-title" class="text-sm font-medium">{title}</h2>{/if}
      {#if aside}<div class="text-xs text-muted-foreground">{@render aside()}</div>{/if}
    </div>
  {/if}
  <div class={cn("rounded-xl bg-card px-4 *:not-first:border-t *:not-first:border-border")}>
    {@render children()}
  </div>
  {#if footer}<div class="mt-2.5 mx-3.5 text-xs text-muted-foreground">{@render footer()}</div>{/if}
</section>
