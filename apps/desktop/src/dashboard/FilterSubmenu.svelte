<script lang="ts">
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import { DropdownMenu } from "bits-ui";
  import type { Snippet } from "svelte";
  import { menuContent, menuItem } from "$lib/components/ui/mac";
  import { cn } from "$lib/utils";

  let {
    label,
    ariaLabel,
    count,
    boundary = null,
    width,
    icon,
    search,
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
    search?: Snippet;
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
      class={cn(
        menuContent,
        "flex max-h-[min(520px,var(--bits-floating-available-height,520px))] flex-col overflow-y-hidden",
        width
      )}
      collisionBoundary={boundary ?? []}
      collisionPadding={8}
      {onOpenAutoFocus}
      sideOffset={6}
      sticky="always"
      >{#if search}
        {@render search()}
        <DropdownMenu.Separator
          class="-mx-[5px] mb-1 h-px shrink-0 bg-border"
        />
      {/if}
      <div class="min-h-0 overflow-x-hidden overflow-y-auto">
        {@render children()}
      </div></DropdownMenu.SubContent
    ></DropdownMenu.Portal
  ></DropdownMenu.Sub
>
