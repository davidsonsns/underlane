<script lang="ts">
  import Check from "@lucide/svelte/icons/check";

  import Waypoints from "@lucide/svelte/icons/waypoints";
  import { DropdownMenu } from "bits-ui";

  import { Input } from "$lib/components/ui/input/index.js";
  import { menuIndicator, menuItem, menuLabel } from "$lib/components/ui/mac";
  import { store } from "$lib/store.svelte";
  import type { Tunnel } from "$lib/types";
  import FilterSubmenu from "./FilterSubmenu.svelte";
  import { filterSearchKeys, focusFilterSearch } from "./filter-search";

  let {
    tunnelId = $bindable(null),
    boundary = null,
    includeToken = false,
  }: {
    tunnelId: string | null;
    boundary?: HTMLElement | null;
    includeToken?: boolean;
  } = $props();
  let tunnelSearch = $state("");
  let tunnelInput = $state<HTMLInputElement | null>(null);
  const tunnelGroups = $derived(
    (store.overview?.groups ?? [])
      .map((group) => ({
        name: group.name,
        tunnels: store.tunnels.filter(
          (t) =>
            (includeToken || t.kind !== "token") &&
            t.group_id === group.id &&
            `${t.name} ${group.name}`
              .toLowerCase()
              .includes(tunnelSearch.toLowerCase())
        ),
      }))
      .filter((group) => group.tunnels.length)
  );
  const ungrouped = $derived(
    store.tunnels.filter(
      (t) =>
        (includeToken || t.kind !== "token") &&
        !t.group_id &&
        t.name.toLowerCase().includes(tunnelSearch.toLowerCase())
    )
  );
</script>
{#snippet tunnelChoice(
  t: Tunnel
)}
  <DropdownMenu.RadioItem
    class={menuItem}
    closeOnSelect={false}
    textValue={t.name}
    value={t.id}
  >
    <span class={menuIndicator}
      >{#if tunnelId === t.id}
        <Check />
      {/if}</span
    ><span class="truncate">{t.name}</span>
  </DropdownMenu.RadioItem>
{/snippet}

<FilterSubmenu
  ariaLabel="Project and tunnel filters"
  {boundary}
  count={Number(tunnelId !== null)}
  label="Projects / tunnels"
  onOpenAutoFocus={(event) => focusFilterSearch(event, () => tunnelInput)}
  onOpenChange={(value) => {
    if (!value) {
      tunnelSearch = "";
    }
  }}
  width="w-64"
>
  {#snippet icon()}
    <Waypoints />
  {/snippet}
  <Input
    aria-label="Search projects and tunnels"
    class="mb-1"
    onkeydown={filterSearchKeys}
    placeholder="Filter projects / tunnels…"
    bind:ref={tunnelInput}
    bind:value={tunnelSearch}
  />
  <DropdownMenu.RadioGroup
    onValueChange={(value) => {
      tunnelId = value === "*" ? null : value;
    }}
    value={tunnelId ?? "*"}
  >
    <DropdownMenu.RadioItem class={menuItem} closeOnSelect={false} value="*">
      <span class={menuIndicator}
        >{#if tunnelId === null}
          <Check />
        {/if}</span
      >All projects / tunnels
    </DropdownMenu.RadioItem>
    {#each tunnelGroups as group (group.name)}
      <DropdownMenu.Group>
        <DropdownMenu.GroupHeading class={menuLabel}
          >{group.name}</DropdownMenu.GroupHeading
        >
        {#each group.tunnels as t (t.id)}
          {@render tunnelChoice(t)}
        {/each}
      </DropdownMenu.Group>
    {/each}
    {#each ungrouped as t (t.id)}
      {@render tunnelChoice(t)}
    {/each}
  </DropdownMenu.RadioGroup>
  {#if !(tunnelGroups.length || ungrouped.length)}
    <p class="px-2 py-3 text-xs text-muted-foreground">
      No matching projects or tunnels
    </p>
  {/if}
</FilterSubmenu>
