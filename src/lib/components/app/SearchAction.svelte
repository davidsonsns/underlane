<script lang="ts">
  import { tick } from "svelte";
  import { cubicOut } from "svelte/easing";
  import SearchIcon from "@lucide/svelte/icons/search";
  import X from "@lucide/svelte/icons/x";
  import IconAction from "./IconAction.svelte";
  import * as InputGroup from "$lib/components/ui/input-group/index.js";
  import { motion } from "$lib/theme";

  let { value = $bindable(""), open = $bindable(false), label }: { value: string; open?: boolean; label: string } = $props();
  const id = $props.id();
  let input = $state<HTMLInputElement | null>(null);
  let trigger = $state<HTMLElement | null>(null);
  let box = $state<HTMLDivElement>();

  export async function focus() {
    open = true;
    await tick();
    input?.focus();
  }

  async function close(clear = true, restoreFocus = true) {
    if (clear) value = "";
    open = false;
    if (restoreFocus) { await tick(); trigger?.focus(); }
  }

  function outside(e: PointerEvent) {
    if (open && !box?.contains(e.target as Node)) void close(false, false);
  }

  function reveal(_node: HTMLElement) {
    return { duration: motion(220), easing: cubicOut, css: (t: number) => `width: ${t * 228}px; opacity: ${t}; overflow: hidden` };
  }
</script>

<svelte:document onpointerdown={outside} />

<div class="flex items-center" bind:this={box}>
  <IconAction {label} bind:ref={trigger} class={open ? "hidden" : ""} aria-expanded={open} aria-controls={open ? id : undefined} onclick={focus}><SearchIcon /></IconAction>
  {#if open}
    <div transition:reveal>
      <InputGroup.Root class="w-57">
        <InputGroup.Input {id} placeholder="{label}…" bind:value bind:ref={input} disabled={!open} aria-label={label} autocorrect="off" autocapitalize="off" spellcheck="false" onkeydown={(e) => { if (e.key === "Escape") { e.stopPropagation(); void close(); } }} />
        <InputGroup.Addon align="inline-end"><InputGroup.Button size="icon-sm" aria-label="Close search" onclick={() => close()}><X /></InputGroup.Button></InputGroup.Addon>
      </InputGroup.Root>
    </div>
  {/if}
</div>
