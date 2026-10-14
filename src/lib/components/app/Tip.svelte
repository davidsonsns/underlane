<script lang="ts" module>
  import type TooltipContent from "$lib/components/ui/tooltip/tooltip-content.svelte";

  // Tooltip.Content (bits' popper layer and floating-ui, ~20 KB gzip) loads on the first hover or
  // keyboard focus of any Tip, so the popover's first open doesn't pay for it (02 O10). A hover
  // loads it during the 250ms delay; keyboard focus opens at once, so there it fades in once
  // loaded. The dashboard imports it statically elsewhere, so there it's already loaded.
  let Content = $state<typeof TooltipContent | null>(null);
  let _loading: Promise<unknown> | undefined;
  /** Load the tooltip content ahead of the first hover (the popover calls this once it's idle
   *  after opening): content mounted late, mid-hover, can't run its exit animation and stays. */
  export function preloadTipContent() {
    load();
  }
  function load() {
    _loading ??= import("$lib/lazy").then(
      (m) => {
        Content = m.TooltipContent;
      },
      () => {
        _loading = undefined;
      } // retry on the next hover instead of never showing tooltips
    );
  }
</script>

<script lang="ts">
  import { mergeProps } from "bits-ui";
  // The one tooltip wrapper (02 §1.1). The trigger is the caller's own element: Tip hands it
  // the trigger props through the children snippet, so no extra wrapper element is added.
  //   <Tip text="Copy public URL">{#snippet children({ props })}<button {...props}>…</button>{/snippet}</Tip>
  // Needs a <Tooltip.Provider delayDuration={250} ignoreNonKeyboardFocus> above it (mounted once per window).
  import type { Snippet } from "svelte";
  import { Kbd } from "$lib/components/ui/kbd/index.js";
  import TooltipRoot from "$lib/components/ui/tooltip/tooltip.svelte";
  import TooltipTrigger from "$lib/components/ui/tooltip/tooltip-trigger.svelte";

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

  // Until Content has loaded, bits can't close an open tooltip when the pointer leaves (its
  // safe-polygon lives in Content), so a late load would show a tooltip after the user moved
  // away. Close it ourselves on leave/blur while Content is missing.
  let open = $state(false);
  const closeIfNotLoaded = () => {
    if (!Content) {
      open = false;
    }
  };

  // Open on focus only during keyboard navigation (html[data-kb-nav], installFocusModality).
  // WebKit matches :focus-visible on programmatic focus, so bits' ignoreNonKeyboardFocus alone
  // still opened the tooltip when returnFocusTo() refocused a trigger after a dialog closed.
  function keyboardFocusOnly(props: Record<string, unknown>) {
    const onfocus = props.onfocus as ((e: FocusEvent) => void) | undefined;
    const kbNav = () => document.documentElement.hasAttribute("data-kb-nav");
    return mergeProps(
      { ...props, onfocus: (e: FocusEvent) => kbNav() && onfocus?.(e) },
      // Start loading the content on the first hover or keyboard focus.
      {
        onblur: closeIfNotLoaded,
        onfocus: () => kbNav() && load(),
        onpointerenter: load,
        onpointerleave: closeIfNotLoaded,
      }
    );
  }
</script>

<TooltipRoot {disabled} bind:open>
  <TooltipTrigger>
    {#snippet child({
      props,
    })}
      {@render children({ props: keyboardFocusOnly(props) })}
    {/snippet}
  </TooltipTrigger>
  {#if Content}
    <!-- 8px keeps tooltips inside the 360x500 popover window. -->
    <Content collisionPadding={8} {side}>
      {text}
      {#if shortcut}
        <Kbd class="ms-2">{shortcut}</Kbd>
      {/if}
    </Content>
  {/if}
</TooltipRoot>
