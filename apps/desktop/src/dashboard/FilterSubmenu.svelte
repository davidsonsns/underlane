<script lang="ts">
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import { DropdownMenu } from "bits-ui";
  import type { Snippet } from "svelte";
  import { menuContent, menuItem } from "$lib/components/ui/mac";

  let {
    label,
    ariaLabel,
    count,
    boundary = null,
    width,
    icon,
    onOpenAutoFocus,
    onOpenChange,
    children,
  }: {
    label: string;
    ariaLabel: string;
    count: number;
    boundary?: HTMLElement | null;
    width: string;
    icon: Snippet;
    onOpenAutoFocus?: (event: Event) => void;
    onOpenChange?: (value: boolean) => void;
    children: Snippet;
  } = $props();
</script>
<DropdownMenu.Sub {onOpenChange}
  ><DropdownMenu.SubTrigger class="{menuItem} ps-2"
    >{@render icon()}
    {label}
    {#if count}
      <span class="ml-auto text-xs text-muted-foreground">{count}</span>
    {/if}
    <ChevronRight class="ml-auto size-3.5" /></DropdownMenu.SubTrigger
  ><DropdownMenu.Portal
    ><DropdownMenu.SubContent
      aria-label={ariaLabel}
      class="{menuContent} max-h-[min(520px,var(--bits-floating-available-height,520px))] {width}"
      collisionBoundary={boundary ?? []}
      collisionPadding={8}
      {onOpenAutoFocus}
      sideOffset={6}
      sticky="always"
      >{@render children()}</DropdownMenu.SubContent
    ></DropdownMenu.Portal
  ></DropdownMenu.Sub
>
