<script lang="ts" module>
  import type TooltipContent from "$lib/components/ui/tooltip/tooltip-content.svelte";

  // Tooltip.Content (bits' popper layer and floating-ui, ~20 KB gzip) loads on the first hover or
  // keyboard focus of any Tip, so the popover's first open doesn't pay for it (02 O10). The
  // dashboard imports it statically elsewhere, so there it's already loaded.
  let Content = $state<typeof TooltipContent | null>(null);
  let loading: Promise<unknown> | undefined;
  function load() {
    loading ??= import("$lib/lazy").then((m) => (Content = m.TooltipContent));
  }
</script>

<script lang="ts">
  // The one tooltip wrapper (02 §1.1). The trigger is the caller's own element: Tip hands it
  // the trigger props through the children snippet, so no extra wrapper element is added.
  //   <Tip text="Copy public URL">{#snippet children({ props })}<button {...props}>…</button>{/snippet}</Tip>
  // Needs a <Tooltip.Provider delayDuration={250} ignoreNonKeyboardFocus> above it (mounted once per window).
  import type { Snippet } from "svelte";
  import { mergeProps } from "bits-ui";
  import TooltipRoot from "$lib/components/ui/tooltip/tooltip.svelte";
  import TooltipTrigger from "$lib/components/ui/tooltip/tooltip-trigger.svelte";
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
    const kbNav = () => document.documentElement.hasAttribute("data-kb-nav");
    return mergeProps(
      { ...props, onfocus: (e: FocusEvent) => kbNav() && onfocus?.(e) },
      // Start loading the content during the 250ms open delay.
      { onpointerenter: load, onfocus: () => kbNav() && load() }
    );
  }
</script>

<TooltipRoot {disabled}>
  <TooltipTrigger>
    {#snippet child({ props })}{@render children({ props: keyboardFocusOnly(props) })}{/snippet}
  </TooltipTrigger>
  {#if Content}
    <!-- 8px keeps tooltips inside the 360x500 popover window (tooltip.ts EDGE). -->
    <Content {side} collisionPadding={8}>
      {text}{#if shortcut}<Kbd class="ms-2">{shortcut}</Kbd>{/if}
    </Content>
  {/if}
</TooltipRoot>
