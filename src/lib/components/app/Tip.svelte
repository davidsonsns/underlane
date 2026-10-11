<script lang="ts">
  // The one tooltip wrapper (02 §1.1). The trigger is the caller's own element: Tip hands it
  // the trigger props through the children snippet, so no extra wrapper element is added.
  //   <Tip text="Copy public URL">{#snippet children({ props })}<button {...props}>…</button>{/snippet}</Tip>
  // Needs a <Tooltip.Provider delayDuration={250} ignoreNonKeyboardFocus> above it (mounted once per window).
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

  // Open on focus only during keyboard navigation (html[data-kb-nav], installFocusModality).
  // WebKit matches :focus-visible on programmatic focus, so bits' ignoreNonKeyboardFocus alone
  // still opened the tooltip when returnFocusTo() refocused a trigger after a dialog closed.
  function keyboardFocusOnly(props: Record<string, unknown>) {
    const onfocus = props.onfocus as ((e: FocusEvent) => void) | undefined;
    return { ...props, onfocus: (e: FocusEvent) => document.documentElement.hasAttribute("data-kb-nav") && onfocus?.(e) };
  }
</script>

<Tooltip.Root {disabled}>
  <Tooltip.Trigger>
    {#snippet child({ props })}{@render children({ props: keyboardFocusOnly(props) })}{/snippet}
  </Tooltip.Trigger>
  <!-- 8px keeps tooltips inside the 360x500 popover window (tooltip.ts EDGE). -->
  <Tooltip.Content {side} collisionPadding={8}>
    {text}{#if shortcut}<Kbd class="ms-2">{shortcut}</Kbd>{/if}
  </Tooltip.Content>
</Tooltip.Root>
