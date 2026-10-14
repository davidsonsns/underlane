<script lang="ts">
  import ListFilter from "@lucide/svelte/icons/list-filter";
  import { DropdownMenu } from "bits-ui";
  import type { Snippet } from "svelte";
  import IconAction from "$lib/components/app/IconAction.svelte";
  import { menuContent, menuLabel } from "$lib/components/ui/mac";
  import { returnFocusOnClose } from "$lib/focus";

  let {
    open = $bindable(false),
    boundary = null,
    count,
    title,
    onOpenChange,
    children,
  }: {
    open?: boolean;
    boundary?: HTMLElement | null;
    count: number;
    title: string;
    onOpenChange?: (value: boolean) => void;
    children: Snippet;
  } = $props();
  let trigger = $state<HTMLElement | null>(null);
</script>
<DropdownMenu.Root {onOpenChange} bind:open>
  <DropdownMenu.Trigger
    >{#snippet child({
      props,
    })}
      <IconAction
        {...props}
        class={count ? "w-auto gap-1.5 px-2" : ""}
        label={count ? `Filters, ${count} active` : "Filters"}
        tip={title}
        bind:ref={trigger}
        ><ListFilter />
        {#if count}
          <span
            class="rounded-sm bg-fill-strong px-1 text-xs tabular-nums text-foreground"
            >{count}</span
          >
        {/if}</IconAction
      >
    {/snippet}</DropdownMenu.Trigger
  >
  <DropdownMenu.Portal
    ><DropdownMenu.Content
      align="end"
      class="{menuContent} w-52"
      collisionBoundary={boundary ?? []}
      collisionPadding={8}
      sideOffset={4}
      sticky="always"
      {...returnFocusOnClose(() => trigger)}
      ><DropdownMenu.Group
        ><DropdownMenu.GroupHeading class={menuLabel}
          >{title}</DropdownMenu.GroupHeading
        >{@render children()}</DropdownMenu.Group
      ></DropdownMenu.Content
    ></DropdownMenu.Portal
  >
</DropdownMenu.Root>
