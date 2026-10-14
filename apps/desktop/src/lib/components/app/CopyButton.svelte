<script lang="ts">
  import CopyIcon from "@lucide/svelte/icons/copy";
  import { writeText } from "@tauri-apps/plugin-clipboard-manager";
  import { mergeProps } from "bits-ui";
  // Copies text and confirms with a toast (notify.copied). Icon-only by default; pass `label`
  // for a text button (RequestDetail's "Copy as curl", which the e2e reads, stays a text button).
  import type {
    ButtonSize,
    ButtonVariant,
  } from "$lib/components/ui/button/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { notify } from "$lib/toast";
  import IconAction from "./IconAction.svelte";
  import Tip from "./Tip.svelte";

  let {
    value,
    what,
    label,
    tip,
    variant,
    size,
    disabled = false,
    class: className,
  }: {
    /** The text to copy, or a function that builds it on click. */
    value: string | (() => string);
    /** Names the thing in "Copied <what>" and in the default tooltip. */
    what: string;
    /** Visible text; without it the button is icon-only. */
    label?: string;
    tip?: string;
    variant?: ButtonVariant;
    size?: ButtonSize;
    disabled?: boolean;
    class?: string;
  } = $props();

  async function copy() {
    const text = typeof value === "function" ? value() : value;
    try {
      await writeText(text);
    } catch (e) {
      // Outside Tauri (the dev browser) the plugin isn't there.
      try {
        await navigator.clipboard.writeText(text);
      } catch {
        notify.error(e, "Couldn't copy");
        return;
      }
    }
    notify.copied(what);
  }
</script>

{#if label}
  <Tip text={tip ?? `Copy ${what}`}>
    {#snippet children({
      props,
    })}
      <Button
        {...mergeProps(props, { onclick: copy })}
        class={className}
        {disabled}
        {size}
        variant={variant ?? "outline"}
        ><CopyIcon class="size-3.5" />{label}</Button
      >
    {/snippet}
  </Tip>
{:else}
  <IconAction
    class={className}
    {disabled}
    label={`Copy ${what}`}
    onclick={copy}
    {size}
    {tip}
    {variant}
    ><CopyIcon /></IconAction
  >
{/if}
