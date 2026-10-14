<script lang="ts">
  import { DropdownMenu } from "bits-ui";
  import ListFilter from "@lucide/svelte/icons/list-filter";
  import CircleDot from "@lucide/svelte/icons/circle-dot";
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import Check from "@lucide/svelte/icons/check";
  import IconAction from "$lib/components/app/IconAction.svelte";
  import { menuContent, menuItem, menuLabel, menuIndicator } from "$lib/components/ui/mac";
  import { returnFocusOnClose } from "$lib/focus";
  import TunnelFilter from "./TunnelFilter.svelte";

  let { levels = $bindable([]), tunnelId = $bindable(null), open = $bindable(false), boundary = null }: { levels: string[]; tunnelId: string | null; open?: boolean; boundary?: HTMLElement | null } = $props();
  let trigger = $state<HTMLElement | null>(null);
  const count = $derived(Number(levels.length > 0) + Number(tunnelId != null));
  const levelOptions = [
    { value: "debug", label: "Debug", color: "var(--text-3)" },
    { value: "info", label: "Info", color: "var(--info)" },
    { value: "warn", label: "Warn", color: "var(--degraded)" },
    { value: "error", label: "Error", color: "var(--bad)" },
  ];
</script>

<DropdownMenu.Root bind:open>
  <DropdownMenu.Trigger>
    {#snippet child({ props })}
      <IconAction {...props} bind:ref={trigger} label={count ? `Filters, ${count} active` : "Filters"} tip="Filter logs" class={count ? "w-auto gap-1.5 px-2" : ""}>
        <ListFilter />
        {#if count}<span class="rounded-sm bg-fill-strong px-1 text-xs tabular-nums text-foreground">{count}</span>{/if}
      </IconAction>
    {/snippet}
  </DropdownMenu.Trigger>
  <DropdownMenu.Portal>
    <DropdownMenu.Content align="end" sideOffset={4} collisionBoundary={boundary ?? []} collisionPadding={8} sticky="always" class="{menuContent} w-52" {...returnFocusOnClose(() => trigger)}>
      <DropdownMenu.Group>
        <DropdownMenu.GroupHeading class={menuLabel}>Filter logs</DropdownMenu.GroupHeading>
        <DropdownMenu.Sub>
          <DropdownMenu.SubTrigger class="{menuItem} ps-2">
            <CircleDot />Level
            {#if levels.length}<span class="ml-auto text-xs text-muted-foreground">{levels.length}</span>{/if}
            <ChevronRight class="ml-auto size-3.5" />
          </DropdownMenu.SubTrigger>
          <DropdownMenu.Portal>
            <DropdownMenu.SubContent sideOffset={6} collisionBoundary={boundary ?? []} collisionPadding={8} sticky="always" class="{menuContent} max-h-[min(520px,var(--bits-floating-available-height,520px))] w-44" aria-label="Level filters">
              {#each levelOptions as level (level.value)}
                <DropdownMenu.CheckboxItem checked={levels.includes(level.value)} onCheckedChange={(checked) => { levels = checked ? [...levels, level.value] : levels.filter((value) => value !== level.value); }} closeOnSelect={false} textValue={level.label} class={menuItem}>
                  <span class="{menuIndicator} size-3 rounded-xs border border-input">{#if levels.includes(level.value)}<Check />{/if}</span>
                  <span class="size-1.5 rounded-full" style:background-color={level.color} aria-hidden="true"></span>{level.label}
                </DropdownMenu.CheckboxItem>
              {/each}
            </DropdownMenu.SubContent>
          </DropdownMenu.Portal>
        </DropdownMenu.Sub>
        <TunnelFilter bind:tunnelId {boundary} includeToken />
      </DropdownMenu.Group>
    </DropdownMenu.Content>
  </DropdownMenu.Portal>
</DropdownMenu.Root>
