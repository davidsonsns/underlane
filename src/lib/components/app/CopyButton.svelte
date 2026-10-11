<script lang="ts">
  // Copies text and confirms with a toast (notify.copied). Icon-only by default; pass `label`
  // for a text button (RequestDetail's "Copy as curl", which the e2e reads, stays a text button).
  import type { ButtonSize, ButtonVariant } from "$lib/components/ui/button/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { writeText } from "@tauri-apps/plugin-clipboard-manager";
  import CopyIcon from "@lucide/svelte/icons/copy";
  import { mergeProps } from "bits-ui";
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
  } = $props();

  async function copy() {
    const text = typeof value === "function" ? value() : value;
    try {
      await writeText(text);
    } catch (e) {
      // Outside Tauri (ui-lab, the dev browser) the plugin isn't there.
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
    {#snippet children({ props })}
      <Button {...mergeProps(props, { onclick: copy })} variant={variant ?? "outline"} {size} {disabled}><CopyIcon class="size-3.5" />{label}</Button>
    {/snippet}
  </Tip>
{:else}
  <IconAction label={`Copy ${what}`} {tip} {variant} {size} {disabled} onclick={copy}><CopyIcon /></IconAction>
{/if}
