<script lang="ts">
  import Check from "@lucide/svelte/icons/check";

  import CircleDot from "@lucide/svelte/icons/circle-dot";

  import { DropdownMenu } from "bits-ui";

  import { menuIndicator, menuItem } from "$lib/components/ui/mac";

  import FilterMenu from "./FilterMenu.svelte";
  import FilterSubmenu from "./FilterSubmenu.svelte";
  import TunnelFilter from "./TunnelFilter.svelte";

  let {
    levels = $bindable([]),
    tunnelId = $bindable(null),
    tunnelLocked = false,
    open = $bindable(false),
    boundary = null,
  }: {
    levels: string[];
    tunnelId: string | null;
    tunnelLocked?: boolean;
    open?: boolean;
    boundary?: HTMLElement | null;
  } = $props();
  const count = $derived(
    Number(levels.length > 0) + Number(!tunnelLocked && tunnelId !== null)
  );
  const levelOptions = [
    { color: "var(--text-3)", label: "Debug", value: "debug" },
    { color: "var(--info)", label: "Info", value: "info" },
    { color: "var(--degraded)", label: "Warn", value: "warn" },
    { color: "var(--bad)", label: "Error", value: "error" },
  ];
</script>

<FilterMenu {boundary} {count} title="Filter logs" bind:open>
  <FilterSubmenu
    ariaLabel="Level filters"
    {boundary}
    count={levels.length}
    label="Level"
    width="w-44"
  >
    {#snippet icon()}
      <CircleDot />
    {/snippet}
    {#each levelOptions as level (level.value)}
      <DropdownMenu.CheckboxItem
        checked={levels.includes(level.value)}
        class={menuItem}
        closeOnSelect={false}
        onCheckedChange={(checked) => {
          levels = checked
            ? [...levels, level.value]
            : levels.filter((value) => value !== level.value);
        }}
        textValue={level.label}
      >
        <span class="{menuIndicator} size-3 rounded-xs border border-input"
          >{#if levels.includes(level.value)}
            <Check />
          {/if}</span
        >
        <span
          aria-hidden="true"
          class="size-1.5 rounded-full"
          style:background-color={level.color}
        ></span>{level.label}
      </DropdownMenu.CheckboxItem>
    {/each}
  </FilterSubmenu>
  {#if !tunnelLocked}
    <TunnelFilter {boundary} includeToken bind:tunnelId />
  {/if}
</FilterMenu>
