<script lang="ts">
  import SearchIcon from "@lucide/svelte/icons/search";
  import X from "@lucide/svelte/icons/x";
  import { tick } from "svelte";
  import { cubicOut } from "svelte/easing";
  import {
    Addon as InputGroupAddon,
    Button as InputGroupButton,
    Input as InputGroupInput,
    Root as InputGroupRoot,
  } from "$lib/components/ui/input-group/index.js";
  import { motion } from "$lib/theme";
  import IconAction from "./IconAction.svelte";

  let {
    value = $bindable(""),
    open = $bindable(false),
    label,
  }: { value: string; open?: boolean; label: string } = $props();
  const id = $props.id();
  let input = $state<HTMLInputElement | null>(null);
  let trigger = $state<HTMLElement | null>(null);
  let box = $state<HTMLDivElement>();

  // Component instances expose focus for their own search input.
  export async function focus() {
    open = true;
    await tick();
    input?.focus();
  }

  async function close(clear = true, restoreFocus = true) {
    if (clear) {
      value = "";
    }
    open = false;
    if (restoreFocus) {
      await tick();
      trigger?.focus();
    }
  }

  function outside(e: PointerEvent) {
    if (open && !box?.contains(e.target as Node)) {
      close(false, false);
    }
  }

  function reveal(_node: HTMLElement) {
    return {
      css: (t: number) =>
        `width: ${t * 228}px; opacity: ${t}; overflow: hidden`,
      duration: motion(220),
      easing: cubicOut,
    };
  }
</script>

<svelte:document onpointerdown={outside} />

<div class="flex items-center" bind:this={box}>
  <IconAction
    aria-controls={open ? id : undefined}
    aria-expanded={open}
    class={open ? "hidden" : ""}
    {label}
    onclick={focus}
    bind:ref={trigger}
    ><SearchIcon /></IconAction
  >
  {#if open}
    <div transition:reveal>
      <InputGroupRoot class="w-57">
        <InputGroupInput
          aria-label={label}
          autocapitalize="off"
          autocorrect="off"
          disabled={!open}
          {id}
          onkeydown={(e) => {
            if (e.key === "Escape") {
              e.stopPropagation();
              close();
            }
          }}
          placeholder="{label}…"
          spellcheck="false"
          bind:ref={input}
          bind:value
        />
        <InputGroupAddon align="inline-end"
          ><InputGroupButton
            aria-label="Close search"
            onclick={() => close()}
            size="icon-sm"
            ><X /></InputGroupButton
          ></InputGroupAddon
        >
      </InputGroupRoot>
    </div>
  {/if}
</div>
