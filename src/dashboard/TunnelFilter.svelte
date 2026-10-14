<script lang="ts">
  import { tick } from "svelte";
  import { DropdownMenu } from "bits-ui";
  import Waypoints from "@lucide/svelte/icons/waypoints";
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import Check from "@lucide/svelte/icons/check";
  import { Input } from "$lib/components/ui/input/index.js";
  import { menuContent, menuItem, menuLabel, menuIndicator } from "$lib/components/ui/mac";
  import { store } from "$lib/store.svelte";

  let { tunnelId = $bindable(null), boundary = null, includeToken = false }: { tunnelId: string | null; boundary?: HTMLElement | null; includeToken?: boolean } = $props();
  let tunnelSearch = $state("");
  let tunnelInput = $state<HTMLInputElement | null>(null);
  const tunnelGroups = $derived((store.overview?.groups ?? []).map((group) => ({
    name: group.name,
    tunnels: store.tunnels.filter((t) => (includeToken || t.kind !== "token") && t.group_id === group.id && `${t.name} ${group.name}`.toLowerCase().includes(tunnelSearch.toLowerCase())),
  })).filter((group) => group.tunnels.length));
  const ungrouped = $derived(store.tunnels.filter((t) => (includeToken || t.kind !== "token") && !t.group_id && t.name.toLowerCase().includes(tunnelSearch.toLowerCase())));
  const submenuClass = `${menuContent} max-h-[min(520px,var(--bits-floating-available-height,520px))]`;

  function inputKeys(e: KeyboardEvent) {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      const items = (e.currentTarget as HTMLElement).closest('[role="menu"]')?.querySelectorAll<HTMLElement>('[role="menuitemradio"]');
      const item = e.key === "ArrowDown" ? items?.[0] : items?.[items.length - 1];
      if (item) { e.preventDefault(); e.stopPropagation(); item.focus(); }
      return;
    }
    // Text entry must not activate menu typeahead or select an item on Space.
    if (!["ArrowDown", "ArrowUp", "Escape", "Tab"].includes(e.key)) e.stopPropagation();
  }

  async function focusSearch(e: Event, getInput: () => HTMLInputElement | null) {
    e.preventDefault();
    await tick();
    // Bits' submenu-open event focuses its first item after the mount tick.
    requestAnimationFrame(() => getInput()?.focus());
  }
</script>

<DropdownMenu.Sub onOpenChange={(value) => { if (!value) tunnelSearch = ""; }}>
  <DropdownMenu.SubTrigger class="{menuItem} ps-2">
    <Waypoints />Projects / tunnels
    {#if tunnelId}<span class="ml-auto text-xs text-muted-foreground">1</span>{/if}
    <ChevronRight class="ml-auto size-3.5" />
  </DropdownMenu.SubTrigger>
  <DropdownMenu.Portal>
    <DropdownMenu.SubContent sideOffset={6} collisionBoundary={boundary ?? []} collisionPadding={8} sticky="always" class="{submenuClass} w-64" aria-label="Project and tunnel filters" onOpenAutoFocus={(e) => focusSearch(e, () => tunnelInput)}>
      <Input placeholder="Filter projects / tunnels…" aria-label="Search projects and tunnels" bind:value={tunnelSearch} bind:ref={tunnelInput} onkeydown={inputKeys} class="mb-1" />
      <DropdownMenu.RadioGroup value={tunnelId ?? "*"} onValueChange={(value) => tunnelId = value === "*" ? null : value}>
        <DropdownMenu.RadioItem value="*" closeOnSelect={false} class={menuItem}>
          <span class={menuIndicator}>{#if tunnelId == null}<Check />{/if}</span>All projects / tunnels
        </DropdownMenu.RadioItem>
        {#each tunnelGroups as group (group.name)}
          <DropdownMenu.Group>
            <DropdownMenu.GroupHeading class={menuLabel}>{group.name}</DropdownMenu.GroupHeading>
            {#each group.tunnels as t (t.id)}
              <DropdownMenu.RadioItem value={t.id} closeOnSelect={false} textValue={t.name} class={menuItem}>
                <span class={menuIndicator}>{#if tunnelId === t.id}<Check />{/if}</span><span class="truncate">{t.name}</span>
              </DropdownMenu.RadioItem>
            {/each}
          </DropdownMenu.Group>
        {/each}
        {#each ungrouped as t (t.id)}
          <DropdownMenu.RadioItem value={t.id} closeOnSelect={false} textValue={t.name} class={menuItem}>
            <span class={menuIndicator}>{#if tunnelId === t.id}<Check />{/if}</span><span class="truncate">{t.name}</span>
          </DropdownMenu.RadioItem>
        {/each}
      </DropdownMenu.RadioGroup>
      {#if !tunnelGroups.length && !ungrouped.length}<p class="px-2 py-3 text-xs text-muted-foreground">No matching projects or tunnels</p>{/if}
    </DropdownMenu.SubContent>
  </DropdownMenu.Portal>
</DropdownMenu.Sub>
