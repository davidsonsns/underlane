<script lang="ts">
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import Ellipsis from "@lucide/svelte/icons/ellipsis";
  import Eye from "@lucide/svelte/icons/eye";
  import Play from "@lucide/svelte/icons/play";
  import Plus from "@lucide/svelte/icons/plus";
  import RotateCw from "@lucide/svelte/icons/rotate-cw";
  import Square from "@lucide/svelte/icons/square";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import { DropdownMenu } from "bits-ui";
  import { tick } from "svelte";
  import { api, errorText } from "$lib/api";
  import IconAction from "$lib/components/app/IconAction.svelte";
  import StatusDot from "$lib/components/app/StatusDot.svelte";
  import { Button } from "$lib/components/ui/button/index.js";
  import { kbRing, menuContent, menuItem } from "$lib/components/ui/mac";
  import { connectionWarning } from "$lib/connection";
  import { isOn, statusLabel } from "$lib/format";
  import { store } from "$lib/store.svelte";
  import { groupTunnels } from "$lib/tunnel-groups";
  import { cn } from "$lib/utils";

  let {
    selectedId,
    active = true,
    onselect,
    onadd,
    ongroup,
    collapsed = $bindable([]),
  }: {
    selectedId: string | null;
    active?: boolean;
    onselect: (id: string) => void;
    onadd: (trigger: HTMLElement, group?: string) => void;
    ongroup: (
      trigger: HTMLElement,
      group?: { id: string; name: string }
    ) => void;
    collapsed?: string[];
  } = $props();

  let list = $state<HTMLElement | null>(null);
  let createTrigger = $state<HTMLButtonElement | null>(null);
  let projectMenu = $state<"create" | "bulk" | null>(null);
  let draggedId = $state<string | null>(null);
  let dropTarget = $state<string | null>(null);
  let busy = $state(false);
  let failureMessage = $state<string | null>(null);
  let refreshFailed = $state(false);
  let announcement = $state("");
  let bulkBusy = $state(false);
  let bulkError = $state<string | null>(null);
  const anyOn = $derived(store.tunnels.some(isOn));
  const groupTriggers = new Map<string, HTMLButtonElement>();
  const groups = $derived.by(() => {
    const grouped = groupTunnels(store.tunnels, store.overview?.groups ?? []);
    if (
      draggedId &&
      grouped.some((group) => group.id !== null) &&
      !grouped.some((group) => group.id === null)
    ) {
      grouped.push({ id: null, name: "Ungrouped", tunnels: [] });
    }
    return grouped;
  });
  const hasGroups = $derived(groups.some((group) => group.id !== null));
  const visibleTunnels = $derived(
    groups.flatMap((group) =>
      hasGroups && collapsed.includes(group.id ?? "ungrouped")
        ? []
        : group.tunnels
    )
  );
  const tabStop = $derived(
    visibleTunnels.some((tunnel) => tunnel.id === selectedId)
      ? selectedId
      : visibleTunnels[0]?.id
  );

  function setProjectMenu(menu: "create" | "bulk", open: boolean) {
    if (open) {
      projectMenu = menu;
    } else if (projectMenu === menu) {
      projectMenu = null;
    }
  }

  function onProjectMenuCloseAutoFocus(event: Event) {
    if (projectMenu) {
      event.preventDefault();
    }
  }

  async function bulk(action: () => Promise<void>) {
    if (bulkBusy) {
      return;
    }
    bulkBusy = true;
    bulkError = null;
    try {
      await action();
    } catch (error) {
      bulkError = errorText(error);
    } finally {
      bulkBusy = false;
    }
  }

  function toggle(id: string) {
    collapsed = collapsed.includes(id)
      ? collapsed.filter((value) => value !== id)
      : [...collapsed, id];
  }

  function clearDrag() {
    draggedId = null;
    dropTarget = null;
  }

  function startDrag(event: DragEvent, id: string) {
    if (busy || refreshFailed || !event.dataTransfer) {
      event.preventDefault();
      return;
    }
    event.dataTransfer.setData("application/x-underlane-project", id);
    event.dataTransfer.effectAllowed = "move";
    draggedId = id;
  }

  function canDrop(group: (typeof groups)[number]) {
    const tunnel = store.tunnels.find((item) => item.id === draggedId);
    return !(busy || refreshFailed) && tunnel && tunnel.group_id !== group.id;
  }

  function dragOver(event: DragEvent, group: (typeof groups)[number]) {
    if (!canDrop(group)) {
      return;
    }
    event.preventDefault();
    if (event.dataTransfer) {
      event.dataTransfer.dropEffect = "move";
    }
    dropTarget = group.id ?? "ungrouped";
  }

  function dragLeave(event: DragEvent) {
    if (
      !(event.currentTarget as HTMLElement).contains(
        event.relatedTarget as Node | null
      )
    ) {
      dropTarget = null;
    }
  }

  async function change(action: () => Promise<void>) {
    if (busy || refreshFailed) {
      return false;
    }
    busy = true;
    failureMessage = null;
    try {
      await action();
      await store.refresh();
      if (store.error) {
        refreshFailed = true;
        failureMessage = `Change saved. Couldn't refresh projects: ${store.error}`;
      }
      return true;
    } catch (e) {
      failureMessage = errorText(e);
      return false;
    } finally {
      busy = false;
    }
  }

  async function drop(event: DragEvent, group: (typeof groups)[number]) {
    if (!(canDrop(group) && draggedId)) {
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    const id = draggedId;
    const name = store.tunnels.find((tunnel) => tunnel.id === id)?.name;
    clearDrag();
    if (
      await change(() =>
        api.updateTunnel(id, { group: group.id === null ? null : group.name })
      )
    ) {
      collapsed = collapsed.filter((key) => key !== (group.id ?? "ungrouped"));
      announcement = `${name} moved to ${group.name}.`;
      await tick();
      list
        ?.querySelector<HTMLElement>(`[data-tunnel-id="${id}"]`)
        ?.focus({ preventScroll: true });
    }
  }

  async function deleteGroup(group: (typeof groups)[number]) {
    const { id } = group;
    if (id === null || group.tunnels.length) {
      return;
    }
    if (await change(() => api.deleteGroup(id))) {
      collapsed = collapsed.filter((key) => key !== id);
      announcement = `${group.name} deleted.`;
      await tick();
      createTrigger?.focus({ preventScroll: true });
    }
  }

  async function retryRefresh() {
    if (busy) {
      return;
    }
    busy = true;
    await store.refresh();
    refreshFailed = !!store.error;
    failureMessage = store.error
      ? `Couldn't refresh projects: ${store.error}`
      : null;
    busy = false;
    if (!refreshFailed) {
      await tick();
      (
        list?.querySelector<HTMLElement>(`[data-tunnel-id="${selectedId}"]`) ??
        createTrigger
      )?.focus({ preventScroll: true });
    }
  }

  function onListKey(e: KeyboardEvent) {
    const rows = [
      ...(list?.querySelectorAll<HTMLElement>(
        "[data-tunnel-id]:not([inert])"
      ) ?? []),
    ].filter((element) => !element.closest("[inert]"));
    const i = rows.indexOf(document.activeElement as HTMLElement);
    const to = (
      {
        ArrowDown: i + 1,
        ArrowUp: i - 1,
        End: rows.length - 1,
        Home: 0,
      } as Record<string, number>
    )[e.key];
    if (i < 0 || to === undefined || e.metaKey || e.altKey || e.ctrlKey) {
      return;
    }
    e.preventDefault();
    const row = rows[Math.max(0, Math.min(rows.length - 1, to))];
    document.documentElement.dataset.kbNav = "";
    row.focus();
    if (row.dataset.tunnelId) {
      onselect(row.dataset.tunnelId);
    }
  }
</script>

<svelte:window ondragend={clearDrag} />

{#snippet projectRows(
  group: (typeof groups)[number]
)}
  {#each group.tunnels as tunnel (tunnel.id)}
    <button
      aria-current={active && tunnel.id === selectedId ? "page" : undefined}
      aria-label={`${tunnel.name}, ${statusLabel[tunnel.state.status]}${connectionWarning(tunnel) ? `, ${connectionWarning(tunnel)}` : ""}`}
      class={cn(
        "flex h-8 w-full items-center gap-2.5 rounded-md px-2.5 text-left transition-colors",
        !(busy || refreshFailed) && "cursor-grab active:cursor-grabbing",
        draggedId === tunnel.id && "opacity-50",
        active && tunnel.id === selectedId
          ? "bg-(--selected)"
          : "hover:bg-fill",
        kbRing
      )}
      data-tunnel-id={tunnel.id}
      draggable={!(busy || refreshFailed)}
      onclick={(event) => {
        event.currentTarget.focus({ preventScroll: true });
        onselect(tunnel.id);
      }}
      ondragstart={(event) => startDrag(event, tunnel.id)}
      tabindex={tunnel.id === tabStop ? 0 : -1}
      type="button"
    >
      <StatusDot status={tunnel.state.status} />
      <span class="min-w-0 flex-1 truncate font-medium">{tunnel.name}</span>
      {#if connectionWarning(tunnel)}
        <span
          aria-label={connectionWarning(tunnel) ?? undefined}
          class="text-degraded-text"
          role="img"
          title={connectionWarning(tunnel) ?? undefined}
          ><TriangleAlert aria-hidden="true" class="size-3.5" /></span
        >
      {/if}
      {#if tunnel.inspect}
        <span
          aria-label="Requests are inspected"
          class="flex text-muted-foreground"
          role="img"
          ><Eye aria-hidden="true" class="size-3" /></span
        >
      {/if}
    </button>
  {/each}
{/snippet}

<div class="flex min-h-0 flex-1 flex-col">
  <div class="flex h-8 shrink-0 items-center justify-between gap-2 pl-2.5">
    <h2 class="min-w-0 flex-1 text-xs font-semibold text-muted-foreground">
      Projects
    </h2>
    <div
      class="pointer-events-none flex shrink-0 items-center gap-px opacity-0 transition-opacity duration-(--dur) group-hover/sidebar:pointer-events-auto group-hover/sidebar:opacity-100 focus-within:pointer-events-auto focus-within:opacity-100 [&:has([aria-expanded=true])]:pointer-events-auto [&:has([aria-expanded=true])]:opacity-100"
    >
      <DropdownMenu.Root
        onOpenChange={(open) => setProjectMenu("create", open)}
        open={projectMenu === "create"}
      >
        <DropdownMenu.Trigger>
          {#snippet child({
            props,
          })}
            <Button
              {...props}
              aria-label="New project or group"
              data-add-tunnel
              data-project-menu
              size="icon"
              variant="ghost"
              bind:ref={createTrigger}
              ><Plus class="size-3.5" /></Button
            >
          {/snippet}
        </DropdownMenu.Trigger>
        <DropdownMenu.Portal>
          <DropdownMenu.Content
            align="end"
            class={menuContent}
            onCloseAutoFocus={onProjectMenuCloseAutoFocus}
            sideOffset={3}
          >
            <DropdownMenu.Item
              class={menuItem}
              onSelect={() => createTrigger && onadd(createTrigger)}
              >New project</DropdownMenu.Item
            >
            <DropdownMenu.Item
              class={menuItem}
              onSelect={() => createTrigger && ongroup(createTrigger)}
              >New group</DropdownMenu.Item
            >
          </DropdownMenu.Content>
        </DropdownMenu.Portal>
      </DropdownMenu.Root>
      <DropdownMenu.Root
        onOpenChange={(open) => setProjectMenu("bulk", open)}
        open={projectMenu === "bulk"}
      >
        <DropdownMenu.Trigger>
          {#snippet child({
            props,
          })}
            <IconAction
              {...props}
              disabled={bulkBusy || !store.tunnels.length}
              label="All project actions"
              ><Ellipsis class="size-3.5" /></IconAction
            >
          {/snippet}
        </DropdownMenu.Trigger>
        <DropdownMenu.Portal>
          <DropdownMenu.Content
            align="end"
            class={menuContent}
            onCloseAutoFocus={onProjectMenuCloseAutoFocus}
            sideOffset={3}
          >
            {#if anyOn}
              <DropdownMenu.Item
                class={cn(menuItem, "ps-2")}
                disabled={bulkBusy}
                onSelect={() => bulk(api.restartAll)}
                ><RotateCw />Restart all running</DropdownMenu.Item
              >
              <DropdownMenu.Item
                class={cn(menuItem, "ps-2")}
                disabled={bulkBusy}
                onSelect={() => bulk(api.stopAll)}
                ><Square />Stop all</DropdownMenu.Item
              >
            {:else}
              <DropdownMenu.Item
                class={cn(menuItem, "ps-2")}
                disabled={bulkBusy}
                onSelect={() => bulk(api.startAll)}
                ><Play />Start all</DropdownMenu.Item
              >
            {/if}
          </DropdownMenu.Content>
        </DropdownMenu.Portal>
      </DropdownMenu.Root>
    </div>
  </div>
  {#if bulkError}
    <p class="px-2.5 py-2 text-xs text-bad-text" role="alert">{bulkError}</p>
  {/if}

  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <!-- biome-ignore lint/a11y/noNoninteractiveElementInteractions: Arrow keys move focus between the project buttons. -->
  <nav
    aria-busy={busy}
    aria-label="Projects"
    class="min-h-0 flex-1 overflow-y-auto pt-1"
    onkeydown={onListKey}
    bind:this={list}
  >
    {#if store.loading}
      <p class="px-2.5 py-3 text-xs text-muted-foreground" role="status">
        Loading projects…
      </p>
    {:else if store.error && !store.overview}
      <p class="px-2.5 py-3 text-xs text-bad-text" role="status">
        Couldn't load projects
      </p>
    {:else}
      {#each groups as group (group.id)}
        {@const key = group.id ?? "ungrouped"}
        {@const folded = hasGroups && collapsed.includes(key)}
        {@const running = group.tunnels.filter(isOn).length}
        <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
        <!-- biome-ignore lint/a11y/noNoninteractiveElementInteractions: Group regions accept pointer drops; the project actions menu provides keyboard movement. -->
        <section
          aria-label={`${group.name} projects`}
          class={cn(
            "min-w-0 rounded-md transition-colors",
            hasGroups && "mb-2",
            dropTarget === key && "bg-fill"
          )}
          data-drop-target={dropTarget === key ? "true" : undefined}
          data-project-group={key}
          ondragleave={dragLeave}
          ondragover={(event) => dragOver(event, group)}
          ondrop={(event) => drop(event, group)}
        >
          {#if hasGroups}
            <div
              class={cn(
                "flex min-w-0 items-center rounded-md text-muted-foreground transition-colors hover:bg-fill [&:has([data-group-menu][aria-expanded=true])]:bg-fill",
                active &&
                  folded &&
                  group.tunnels.some((t) => t.id === selectedId) &&
                  "bg-fill text-foreground"
              )}
            >
              <button
                aria-current={active &&
                folded &&
                group.tunnels.some((t) => t.id === selectedId)
                  ? "location"
                  : undefined}
                aria-expanded={!folded}
                aria-label={`${group.name}, ${group.tunnels.length} projects${folded && running ? `, ${running} running` : ""}`}
                class={cn(
                  "flex h-7 min-w-0 flex-1 items-center gap-1.5 rounded-md px-2 text-xs",
                  kbRing
                )}
                onclick={() => toggle(key)}
                type="button"
              >
                {#if folded}
                  <ChevronRight class="size-3 shrink-0" />
                {:else}
                  <ChevronDown class="size-3 shrink-0" />
                {/if}
                <span class="truncate font-medium">{group.name}</span>
                {#if folded && running}
                  <span
                    class="ml-auto flex shrink-0 items-center gap-1.5 pl-1 tabular-nums text-ok-text"
                  >
                    <span
                      aria-hidden="true"
                      class="size-1.5 rounded-full bg-current"
                    ></span>{running}
                    running
                  </span>
                {:else}
                  <span class="ml-auto pl-1 tabular-nums"
                    >{group.tunnels.length}</span
                  >
                {/if}
              </button>
              <DropdownMenu.Root>
                <DropdownMenu.Trigger>
                  {#snippet child({
                    props,
                  })}
                    <Button
                      {...props}
                      aria-label={`${group.name} actions`}
                      class="hover:bg-transparent aria-expanded:bg-transparent"
                      data-group-menu={key}
                      size="icon"
                      variant="ghost"
                      bind:ref={
                        () => groupTriggers.get(key) ?? null,
                        (element) => {
    if (element) {
      groupTriggers.set(key, element);
    } else {
      groupTriggers.delete(key);
    }
  }
                      }
                      ><Ellipsis class="size-3.5" /></Button
                    >
                  {/snippet}
                </DropdownMenu.Trigger>
                <DropdownMenu.Portal>
                  <DropdownMenu.Content
                    align="end"
                    class={menuContent}
                    sideOffset={3}
                  >
                    <DropdownMenu.Item
                      class={menuItem}
                      onSelect={() => {
                        const trigger = groupTriggers.get(key);
                        if (trigger) {
                          onadd(
                            trigger,
                            group.id === null ? undefined : group.name
                          );
                        }
                      }}
                      >New project here</DropdownMenu.Item
                    >
                    {#if group.id !== null}
                      <DropdownMenu.Item
                        class={menuItem}
                        onSelect={() => {
                          const trigger = groupTriggers.get(key);
                          if (trigger && group.id) {
                            ongroup(trigger, {
                              id: group.id,
                              name: group.name,
                            });
                          }
                        }}
                        >Rename group</DropdownMenu.Item
                      >
                      {#if !group.tunnels.length}
                        <DropdownMenu.Item
                          class={cn(menuItem, "text-bad-text")}
                          disabled={busy || refreshFailed}
                          onSelect={() => deleteGroup(group)}
                          >Delete group</DropdownMenu.Item
                        >
                      {/if}
                    {/if}
                  </DropdownMenu.Content>
                </DropdownMenu.Portal>
              </DropdownMenu.Root>
            </div>
          {/if}
          {#if !folded}
            <div class={cn("flex flex-col gap-px", hasGroups && "pt-1 pl-3")}>
              {@render projectRows(group)}
              {#if hasGroups && !group.tunnels.length}
                <p class="px-2.5 py-1.5 text-xs text-muted-foreground">
                  No projects
                </p>
              {/if}
            </div>
          {/if}
        </section>
      {/each}
    {/if}
  </nav>
  {#if failureMessage}
    <div class="px-2.5 py-2 text-xs text-bad-text" role="alert">
      <p>{failureMessage}</p>
      {#if refreshFailed}
        <Button
          disabled={busy}
          onclick={retryRefresh}
          size="sm"
          variant="ghost"
        >
          Retry refresh
        </Button>
      {/if}
    </div>
  {/if}
  <span class="sr-only" role="status">{announcement}</span>
</div>
