<script lang="ts">
  import { tick } from "svelte";
  import { DropdownMenu } from "bits-ui";
  import ListFilter from "@lucide/svelte/icons/list-filter";
  import CircleDot from "@lucide/svelte/icons/circle-dot";
  import Braces from "@lucide/svelte/icons/braces";
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import Check from "@lucide/svelte/icons/check";
  import TunnelFilter from "./TunnelFilter.svelte";
  import IconAction from "$lib/components/app/IconAction.svelte";
  import { Input } from "$lib/components/ui/input/index.js";
  import { menuContent, menuItem, menuLabel, menuIndicator } from "$lib/components/ui/mac";
  import { REQUEST_STATUSES, REQUEST_METHODS, OTHER_REQUEST_METHODS } from "$lib/request-filters";
  import { returnFocusOnClose } from "$lib/focus";

  let { statuses = $bindable([]), methods = $bindable([]), tunnelId = $bindable(null), open = $bindable(false), boundary = null }: { statuses: number[]; methods: string[]; tunnelId: string | null; open?: boolean; boundary?: HTMLElement | null } = $props();
  let trigger = $state<HTMLElement | null>(null);
  let statusSearch = $state("");
  let methodSearch = $state("");
  let statusInput = $state<HTMLInputElement | null>(null);
  let methodInput = $state<HTMLInputElement | null>(null);
  const count = $derived(Number(statuses.length > 0) + Number(methods.length > 0) + Number(tunnelId != null));
  const submenuClass = `${menuContent} max-h-[min(520px,var(--bits-floating-available-height,520px))]`;
  const statusGroups = [
    { code: 2, label: "Success" }, { code: 3, label: "Redirection" }, { code: 4, label: "Client errors" }, { code: 5, label: "Server errors" },
  ];
  const statusOptions = $derived(REQUEST_STATUSES.filter(([code, label]) => `${code} ${label}`.toLowerCase().includes(statusSearch.toLowerCase())));
  const methodGroups = $derived([
    { label: "Common methods", values: REQUEST_METHODS.filter((v) => v.toLowerCase().includes(methodSearch.toLowerCase())) },
    { label: "Other methods", values: OTHER_REQUEST_METHODS.filter((v) => v.toLowerCase().includes(methodSearch.toLowerCase())) },
  ]);

  function inputKeys(e: KeyboardEvent) {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      const items = (e.currentTarget as HTMLElement).closest('[role="menu"]')?.querySelectorAll<HTMLElement>('[role="menuitemcheckbox"], [role="menuitemradio"]');
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

<DropdownMenu.Root bind:open onOpenChange={(value) => { if (!value) { statusSearch = ""; methodSearch = ""; } }}>
  <DropdownMenu.Trigger>
    {#snippet child({ props })}
      <IconAction {...props} bind:ref={trigger} label={count ? `Filters, ${count} active` : "Filters"} tip="Filter requests" class={count ? "w-auto gap-1.5 px-2" : ""}>
        <ListFilter />
        {#if count}<span class="rounded-sm bg-fill-strong px-1 text-xs tabular-nums text-foreground">{count}</span>{/if}
      </IconAction>
    {/snippet}
  </DropdownMenu.Trigger>
  <DropdownMenu.Portal>
    <DropdownMenu.Content align="end" sideOffset={4} collisionBoundary={boundary ?? []} collisionPadding={8} sticky="always" class="{menuContent} w-52" {...returnFocusOnClose(() => trigger)}>
      <DropdownMenu.Group>
        <DropdownMenu.GroupHeading class={menuLabel}>Filter requests</DropdownMenu.GroupHeading>
        <DropdownMenu.Sub>
          <DropdownMenu.SubTrigger class="{menuItem} ps-2">
            <CircleDot />Status
            {#if statuses.length}<span class="ml-auto text-xs text-muted-foreground">{statuses.length}</span>{/if}
            <ChevronRight class="ml-auto size-3.5" />
          </DropdownMenu.SubTrigger>
          <DropdownMenu.Portal>
            <DropdownMenu.SubContent sideOffset={6} collisionBoundary={boundary ?? []} collisionPadding={8} sticky="always" class="{submenuClass} w-64" aria-label="Status filters" onOpenAutoFocus={(e) => focusSearch(e, () => statusInput)}>
              <Input placeholder="Filter status…" aria-label="Search status options" bind:value={statusSearch} bind:ref={statusInput} onkeydown={inputKeys} class="mb-1" />
              {#each statusGroups as group (group.code)}
                {@const options = statusOptions.filter(([code]) => Math.floor(code / 100) === group.code)}
                {#if options.length}
                  <DropdownMenu.Group>
                    <DropdownMenu.GroupHeading class={menuLabel}>{group.code}xx · {group.label}</DropdownMenu.GroupHeading>
                    {#each options as [code, label] (code)}
                      <DropdownMenu.CheckboxItem checked={statuses.includes(code)} onCheckedChange={(checked) => { statuses = checked ? [...statuses, code] : statuses.filter((v) => v !== code); }} closeOnSelect={false} textValue={`${code} ${label}`} class={menuItem}>
                        <span class="{menuIndicator} size-3 rounded-xs border border-input">{#if statuses.includes(code)}<Check />{/if}</span>
                        <span class="w-7 font-mono text-xs text-muted-foreground">{code}</span><span class="truncate">{label}</span>
                      </DropdownMenu.CheckboxItem>
                    {/each}
                  </DropdownMenu.Group>
                {/if}
              {/each}
              {#if !statusOptions.length}<p class="px-2 py-3 text-xs text-muted-foreground">No matching statuses</p>{/if}
            </DropdownMenu.SubContent>
          </DropdownMenu.Portal>
        </DropdownMenu.Sub>
        <DropdownMenu.Sub>
          <DropdownMenu.SubTrigger class="{menuItem} ps-2">
            <Braces />Method
            {#if methods.length}<span class="ml-auto text-xs text-muted-foreground">{methods.length}</span>{/if}
            <ChevronRight class="ml-auto size-3.5" />
          </DropdownMenu.SubTrigger>
          <DropdownMenu.Portal>
            <DropdownMenu.SubContent sideOffset={6} collisionBoundary={boundary ?? []} collisionPadding={8} sticky="always" class="{submenuClass} w-56" aria-label="Method filters" onOpenAutoFocus={(e) => focusSearch(e, () => methodInput)}>
              <Input placeholder="Filter method…" aria-label="Search method options" bind:value={methodSearch} bind:ref={methodInput} onkeydown={inputKeys} class="mb-1" />
              {#each methodGroups as group (group.label)}
                {#if group.values.length}
                  <DropdownMenu.Group>
                    <DropdownMenu.GroupHeading class={menuLabel}>{group.label}</DropdownMenu.GroupHeading>
                    {#each group.values as method (method)}
                      <DropdownMenu.CheckboxItem checked={methods.includes(method)} onCheckedChange={(checked) => { methods = checked ? [...methods, method] : methods.filter((v) => v !== method); }} closeOnSelect={false} textValue={method} class={menuItem}>
                        <span class="{menuIndicator} size-3 rounded-xs border border-input">{#if methods.includes(method)}<Check />{/if}</span>
                        <span class="font-mono text-xs">{method}</span>
                      </DropdownMenu.CheckboxItem>
                    {/each}
                  </DropdownMenu.Group>
                {/if}
              {/each}
              {#if methodGroups.every((group) => !group.values.length)}<p class="px-2 py-3 text-xs text-muted-foreground">No matching methods</p>{/if}
            </DropdownMenu.SubContent>
          </DropdownMenu.Portal>
        </DropdownMenu.Sub>
        <TunnelFilter bind:tunnelId {boundary} />
      </DropdownMenu.Group>
    </DropdownMenu.Content>
  </DropdownMenu.Portal>
</DropdownMenu.Root>
