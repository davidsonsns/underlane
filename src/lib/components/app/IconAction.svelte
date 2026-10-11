<script lang="ts">
  // Icon-only button: always an aria-label and a tooltip (02 §2, .icon-btn). The glyph is
  // the children snippet, e.g. <IconAction label="Copy public URL" onclick={…}><Copy /></IconAction>.
  import type { Snippet } from "svelte";
  import { mergeProps } from "bits-ui";
  import { Button, type ButtonProps, type ButtonSize, type ButtonVariant } from "$lib/components/ui/button/index.js";
  import Tip from "./Tip.svelte";

  let {
    label,
    tip = label,
    shortcut,
    variant = "ghost",
    size = "icon",
    ref = $bindable(null),
    children: glyph,
    ...rest
  }: Omit<ButtonProps, "children" | "variant" | "size" | "ref" | "href"> & {
    /** Accessible name; also the tooltip unless `tip` is given. */
    label: string;
    tip?: string;
    shortcut?: string;
    variant?: ButtonVariant;
    size?: ButtonSize;
    ref?: HTMLElement | null;
    children: Snippet;
  } = $props();
</script>

<Tip text={tip} {shortcut}>
  {#snippet children({ props })}
    <Button bind:ref {...mergeProps(props, rest)} {variant} {size} aria-label={label}>{@render glyph()}</Button>
  {/snippet}
</Tip>
