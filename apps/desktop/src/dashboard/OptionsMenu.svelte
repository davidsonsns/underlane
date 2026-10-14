<script lang="ts">
  import SlidersHorizontal from "@lucide/svelte/icons/sliders-horizontal";
  import type { Snippet } from "svelte";
  import IconAction from "$lib/components/app/IconAction.svelte";
  import { Content, Root, Trigger } from "$lib/components/ui/popover/index.js";
  import { returnFocusOnClose } from "$lib/focus";

  let {
    menu = $bindable(null),
    trigger = $bindable(null),
    children,
  }: {
    menu?: "filters" | "options" | null;
    trigger?: HTMLElement | null;
    children: Snippet;
  } = $props();
</script>
<Root
  bind:open={
    () => menu === "options",
    (value) => {
      if (value) {
        menu = "options";
      } else if (menu === "options") {
        menu = null;
      }
    }
  }
>
  <Trigger
    >{#snippet child({
      props,
    })}
      <IconAction {...props} label="Columns and actions" bind:ref={trigger}
        ><SlidersHorizontal /></IconAction
      >
    {/snippet}</Trigger
  >
  <Content
    align="end"
    class="w-64 gap-2 p-2.5"
    {...returnFocusOnClose(() => trigger)}
    >{@render children()}</Content
  >
</Root>
