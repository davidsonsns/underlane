<script lang="ts">
  // The one tooltip wrapper (02 §1.1). The trigger is the caller's own element: Tip hands it
  // the trigger props through the children snippet, so no extra wrapper element is added.
  //   <Tip text="Copy public URL">{#snippet children({ props })}<button {...props}>…</button>{/snippet}</Tip>
  // Needs a <Tooltip.Provider delayDuration={250}> above it (mounted once per window).
  import type { Snippet } from "svelte";
  import * as Tooltip from "$lib/components/ui/tooltip/index.js";
  import { Kbd } from "$lib/components/ui/kbd/index.js";

  let {
    text,
    shortcut,
    side = "bottom",
    disabled = false,
    children,
  }: {
    text: string;
    /** Shown after the text, menu-shortcut style (e.g. "⌘N"). */
    shortcut?: string;
    side?: "top" | "right" | "bottom" | "left";
    disabled?: boolean;
    children: Snippet<[{ props: Record<string, unknown> }]>;
  } = $props();
</script>

<Tooltip.Root {disabled}>
  <Tooltip.Trigger>
    {#snippet child({ props })}{@render children({ props })}{/snippet}
  </Tooltip.Trigger>
  <!-- 8px keeps tooltips inside the 360x500 popover window (tooltip.ts EDGE). -->
  <Tooltip.Content {side} collisionPadding={8}>
    {text}{#if shortcut}<Kbd class="ms-2">{shortcut}</Kbd>{/if}
  </Tooltip.Content>
</Tooltip.Root>
