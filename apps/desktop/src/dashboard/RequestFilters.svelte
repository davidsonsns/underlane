<script lang="ts">
  import Braces from "@lucide/svelte/icons/braces";
  import Check from "@lucide/svelte/icons/check";

  import CircleDot from "@lucide/svelte/icons/circle-dot";

  import { DropdownMenu } from "bits-ui";

  import { Input } from "$lib/components/ui/input/index.js";
  import { menuIndicator, menuItem, menuLabel } from "$lib/components/ui/mac";

  import {
    OTHER_REQUEST_METHODS,
    REQUEST_METHODS,
    REQUEST_STATUSES,
  } from "$lib/request-filters";
  import FilterMenu from "./FilterMenu.svelte";
  import FilterSubmenu from "./FilterSubmenu.svelte";
  import { filterSearchKeys, focusFilterSearch } from "./filter-search";
  import TunnelFilter from "./TunnelFilter.svelte";

  let {
    statuses = $bindable([]),
    methods = $bindable([]),
    tunnelId = $bindable(null),
    open = $bindable(false),
    boundary = null,
  }: {
    statuses: number[];
    methods: string[];
    tunnelId: string | null;
    open?: boolean;
    boundary?: HTMLElement | null;
  } = $props();
  let statusSearch = $state("");
  let methodSearch = $state("");
  let statusInput = $state<HTMLInputElement | null>(null);
  let methodInput = $state<HTMLInputElement | null>(null);
  const count = $derived(
    Number(statuses.length > 0) +
      Number(methods.length > 0) +
      Number(tunnelId !== null)
  );
  const statusGroups = [
    { code: 2, label: "Success" },
    { code: 3, label: "Redirection" },
    { code: 4, label: "Client errors" },
    { code: 5, label: "Server errors" },
  ];
  const statusOptions = $derived(
    REQUEST_STATUSES.filter(([code, label]) =>
      `${code} ${label}`.toLowerCase().includes(statusSearch.toLowerCase())
    )
  );
  const methodGroups = $derived([
    {
      label: "Common methods",
      values: REQUEST_METHODS.filter((v) =>
        v.toLowerCase().includes(methodSearch.toLowerCase())
      ),
    },
    {
      label: "Other methods",
      values: OTHER_REQUEST_METHODS.filter((v) =>
        v.toLowerCase().includes(methodSearch.toLowerCase())
      ),
    },
  ]);
</script>
{#snippet methodMenu()}
  <FilterSubmenu
    ariaLabel="Method filters"
    {boundary}
    count={methods.length}
    label="Method"
    onOpenAutoFocus={(event) => focusFilterSearch(event, () => methodInput)}
    width="w-56"
  >
    {#snippet icon()}
      <Braces />
    {/snippet}
    {#snippet search()}
      <Input
        aria-label="Search method options"
        class="mb-1 shrink-0 border-0 bg-fill"
        onkeydown={filterSearchKeys}
        placeholder="Filter method…"
        bind:ref={methodInput}
        bind:value={methodSearch}
      />
    {/snippet}
    {#each methodGroups as group (group.label)}
      {#if group.values.length}
        <DropdownMenu.Group>
          <DropdownMenu.GroupHeading class={menuLabel}
            >{group.label}</DropdownMenu.GroupHeading
          >
          {#each group.values as method (method)}
            <DropdownMenu.CheckboxItem
              checked={methods.includes(method)}
              class={menuItem}
              closeOnSelect={false}
              onCheckedChange={(checked) => {
                methods = checked
                  ? [...methods, method]
                  : methods.filter((v) => v !== method);
              }}
              textValue={method}
            >
              <span
                class="{menuIndicator} size-3 rounded-xs border border-input"
                >{#if methods.includes(method)}
                  <Check />
                {/if}</span
              >
              <span class="font-mono text-xs">{method}</span>
            </DropdownMenu.CheckboxItem>
          {/each}
        </DropdownMenu.Group>
      {/if}
    {/each}
    {#if methodGroups.every((group) => !group.values.length)}
      <p class="px-2 py-3 text-xs text-muted-foreground">No matching methods</p>
    {/if}
  </FilterSubmenu>
{/snippet}

{#snippet statusMenu()}
  <FilterSubmenu
    ariaLabel="Status filters"
    {boundary}
    count={statuses.length}
    label="Status"
    onOpenAutoFocus={(event) => focusFilterSearch(event, () => statusInput)}
    width="w-64"
  >
    {#snippet icon()}
      <CircleDot />
    {/snippet}
    {#snippet search()}
      <Input
        aria-label="Search status options"
        class="mb-1 shrink-0 border-0 bg-fill"
        onkeydown={filterSearchKeys}
        placeholder="Filter status…"
        bind:ref={statusInput}
        bind:value={statusSearch}
      />
    {/snippet}
    {#each statusGroups as group (group.code)}
      {@const options = statusOptions.filter(
        ([code]) => Math.floor(code / 100) === group.code
      )}
      {#if options.length}
        <DropdownMenu.Group>
          <DropdownMenu.GroupHeading class={menuLabel}
            >{group.code}xx · {group.label}</DropdownMenu.GroupHeading
          >
          {#each options as [code, label] (code)}
            <DropdownMenu.CheckboxItem
              checked={statuses.includes(code)}
              class={menuItem}
              closeOnSelect={false}
              onCheckedChange={(checked) => {
                statuses = checked
                  ? [...statuses, code]
                  : statuses.filter((v) => v !== code);
              }}
              textValue={`${code} ${label}`}
            >
              <span
                class="{menuIndicator} size-3 rounded-xs border border-input"
                >{#if statuses.includes(code)}
                  <Check />
                {/if}</span
              >
              <span class="w-7 font-mono text-xs text-muted-foreground"
                >{code}</span
              ><span class="truncate">{label}</span>
            </DropdownMenu.CheckboxItem>
          {/each}
        </DropdownMenu.Group>
      {/if}
    {/each}
    {#if !statusOptions.length}
      <p class="px-2 py-3 text-xs text-muted-foreground">
        No matching statuses
      </p>
    {/if}
  </FilterSubmenu>
{/snippet}

<FilterMenu
  {boundary}
  {count}
  onOpenChange={(value) => {
    if (!value) {
      statusSearch = "";
      methodSearch = "";
    }
  }}
  title="Filter requests"
  bind:open
>
  {@render statusMenu()}
  {@render methodMenu()}
  <TunnelFilter {boundary} bind:tunnelId />
</FilterMenu>
