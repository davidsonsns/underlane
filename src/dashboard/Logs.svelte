<script lang="ts">
  import Pause from "@lucide/svelte/icons/pause";
  import Play from "@lucide/svelte/icons/play";

  import Trash2 from "@lucide/svelte/icons/trash-2";

  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";

  import SearchAction from "$lib/components/app/SearchAction.svelte";
  import StatePanel from "$lib/components/app/StatePanel.svelte";
  import Toolbar from "$lib/components/app/Toolbar.svelte";

  import { Button } from "$lib/components/ui/button/index.js";

  import {
    Item as ToggleGroupItem,
    Root as ToggleGroupRoot,
  } from "$lib/components/ui/toggle-group/index.js";

  import { logLevels, matchesLogFilters } from "$lib/log-filters";
  import { api, errorText } from "../lib/api";
  import { store } from "../lib/store.svelte";
  import type { LogLine } from "../lib/types";
  import ClearHistoryDialog from "./ClearHistoryDialog.svelte";
  import FilterChip from "./FilterChip.svelte";
  import { queryTracker } from "./guarded-query";
  import LogFilters from "./LogFilters.svelte";
  import LogList from "./LogList.svelte";
  import OptionsMenu from "./OptionsMenu.svelte";
  import { track } from "./reactivity";

  let { focusId = null }: { focusId?: string | null } = $props();
  let tunnelId = $state<string | null>(null);
  $effect(() => {
    tunnelId = focusId;
  });
  const LEVELS = ["debug", "info", "warn", "error", "fatal"];
  let levels = $state<string[]>([]);
  let search = $state("");
  let searchOpen = $state(false);
  let searchAction = $state<SearchAction>();
  let headerMenu = $state<"filters" | "options" | null>(null);
  let filterBoundary = $state<HTMLDivElement | null>(null);
  let optionsTrigger = $state<HTMLElement | null>(null);
  let paused = $state(false);
  let lines = $state<LogLine[]>([]);
  /** Only the first load shows the queryStatus.loading panel; later reloads keep the current view. */
  const queryStatus = $state<{ failure: string | null; loading: boolean }>({
    failure: null,
    loading: true,
  });
  let actionError = $state<string | null>(null);
  let clearing = $state(false);
  let clearOpen = $state(false);
  let clearTarget = $state<{ id: string | null; name: string } | null>(null);
  let list = $state<LogList>();
  const MAX = 3000;
  const activeLevels = $derived(logLevels(levels));
  const filtered = $derived(Boolean(tunnelId || levels.length || search));
  const names = $derived(
    Object.fromEntries(store.tunnels.map((t) => [t.id, t.name]))
  );

  const COLUMNS = [
    { id: "time", label: "Time" },
    { id: "level", label: "Level" },
    { id: "tunnel", label: "Project / tunnel" },
    { id: "message", label: "Message" },
  ] as const;
  const COLUMN_KEY = "underlane.logs.hidden-columns";
  let hidden = $state<string[]>(loadHidden());
  function loadHidden(): string[] {
    try {
      const saved: unknown = JSON.parse(
        localStorage.getItem(COLUMN_KEY) ?? "[]"
      );
      if (Array.isArray(saved)) {
        return COLUMNS.filter(
          (c) => c.id !== "message" && saved.includes(c.id)
        ).map((c) => c.id);
      }
    } catch {
      /* Storage is optional; defaults remain usable. */
    }
    return [];
  }
  function showColumns(ids: string[]) {
    hidden = COLUMNS.filter(
      (c) => c.id !== "message" && !ids.includes(c.id)
    ).map((c) => c.id);
    try {
      localStorage.setItem(COLUMN_KEY, JSON.stringify(hidden));
    } catch {
      /* Storage is optional; defaults remain usable. */
    }
  }

  function setHeaderMenu(name: "filters" | "options", open: boolean) {
    if (open) {
      headerMenu = name;
    } else if (headerMenu === name) {
      headerMenu = null;
    }
  }

  function matches(l: LogLine, id = tunnelId, lv = activeLevels, q = search) {
    return matchesLogFilters(l, id, lv, q);
  }

  let loadGen = 0;
  let loadKey: string | undefined;
  const queries = queryTracker(queryStatus);
  let loadEvents = new Map<number, LogLine>();
  function load(): Promise<void> {
    if (clearing) {
      return Promise.resolve();
    }
    const lv = [...activeLevels];
    const id = tunnelId;
    const searchText = search;
    const key = JSON.stringify([id, lv, searchText]);
    if (key === loadKey && queries.active) {
      return queries.active;
    }
    if (key !== loadKey) {
      loadGen += 1;
      loadKey = key;
      list?.discardHeld();
    }
    const g = loadGen;
    const current = () => g === loadGen;
    const events = new Map<number, LogLine>();
    loadEvents = events;
    queryStatus.failure = null;
    return queries.run(async () => {
      const r = await api.queryLogs({
        levels: lv.length === LEVELS.length ? [] : lv,
        limit: 1000,
        search: searchText,
        tunnelId: id,
      });
      if (current()) {
        lines = [...r.filter((l) => !events.has(l.id)), ...events.values()]
          .filter((l) => matches(l, id, lv, searchText))
          .sort((a, b) => a.id - b.id)
          .slice(-MAX);
        list?.reset();
      }
    }, current);
  }

  $effect(() => {
    track(tunnelId);
    track(search);
    track(activeLevels.join());
    const t = setTimeout(load, search ? 180 : 0);
    return () => clearTimeout(t);
  });

  function recordLoadEvents(matching: LogLine[]) {
    for (const l of matching) {
      loadEvents.set(l.id, l);
    }
    for (const eventId of loadEvents.keys()) {
      if (loadEvents.size <= MAX) {
        break;
      }
      loadEvents.delete(eventId);
    }
  }

  onMount(() => {
    const un = listen<LogLine[]>("logs", function receiveLogs(e) {
      if (paused || clearing) {
        return;
      }
      const matching = e.payload.filter((l) => matches(l));
      if (queries.active) {
        recordLoadEvents(matching);
      }
      const last = lines.at(-1)?.id ?? 0;
      const add = matching.filter((l) => l.id > last);
      if (!add.length) {
        return;
      }
      if (list) {
        list.append(add);
      } else {
        lines = [...lines, ...add].slice(-MAX);
      }
    });
    return () => un.then((f) => f());
  });

  function toggleLive() {
    paused = !paused;
    headerMenu = null;
    list?.discardHeld();
    if (paused) {
      // Freeze an already-started snapshot too; recording in the backend continues.
      loadGen += 1;
      loadKey = undefined;
      queries.cancel();
      queryStatus.loading = false;
    } else {
      load();
    }
  }

  async function clear() {
    if (!clearTarget || clearing) {
      return;
    }
    const target = clearTarget;
    clearing = true;
    actionError = null;
    loadGen += 1;
    loadKey = undefined;
    queries.cancel();
    list?.discardHeld();
    try {
      await api.clearLogs(target.id);
      if (target.id === tunnelId) {
        lines = [];
      }
      clearOpen = false;
    } catch (e) {
      actionError = errorText(e);
    } finally {
      clearing = false;
      load();
    }
  }

  const emptyMessage = $derived.by(() => {
    if (search || levels.length) {
      return "Try a different search or level filter.";
    }
    if (paused) {
      return "Resume live updates from Columns and actions. Logs continue recording while paused.";
    }
    return "Start a tunnel and cloudflared's output streams here live.";
  });

  const emptyTitle = $derived.by(() => {
    if (search || levels.length) {
      return "No matching log lines";
    }
    if (paused) {
      return "Live updates paused";
    }
    return "No logs yet";
  });
</script>

{#snippet logFilterSummary()}
  {#if filtered}
    <div
      class="flex min-h-11 flex-none flex-wrap items-center gap-2 border-b-[0.5px] border-border px-3 py-1.5"
    >
      {#if tunnelId}
        <FilterChip
          label="Project / tunnel"
          onremove={() => {
            tunnelId = null;
          }}
          removeLabel="Remove project or tunnel filter"
          value={names[tunnelId] ?? tunnelId}
        />
      {/if}
      {#if levels.length}
        <FilterChip
          label="Level"
          onremove={() => {
            levels = [];
          }}
          removeLabel="Remove level filter"
          value={levels.join(", ")}
        />
      {/if}
      <FilterChip
        hidden={!search || searchOpen}
        label="Search"
        onfocus={() => searchAction?.focus()}
        bind:value={search}
      />
      <Button
        class="ml-auto text-xs"
        onclick={() => {
          search = "";
          levels = [];
          tunnelId = null;
        }}
        variant="ghost"
        >Reset filters</Button
      >
    </div>
  {/if}
{/snippet}

{#snippet logToolbar()}
  <Toolbar subtitle={paused ? "Live updates paused" : ""} title="Logs">
    <SearchAction
      label="Search logs"
      bind:this={searchAction}
      bind:open={searchOpen}
      bind:value={search}
    />
    <LogFilters
      boundary={filterBoundary}
      bind:levels
      bind:open={
        () => headerMenu === "filters",
        (value) => setHeaderMenu("filters", value)
      }
      bind:tunnelId
    />
    <span aria-hidden="true" class="mx-0.5 h-4 border-l border-border"></span>
    <OptionsMenu bind:menu={headerMenu} bind:trigger={optionsTrigger}>
      <span class="text-xs text-muted-foreground" id="log-columns-label"
        >Display columns</span
      >
      <ToggleGroupRoot
        aria-labelledby="log-columns-label"
        class="flex-wrap"
        spacing={1}
        type="multiple"
        bind:value={
          () =>
            COLUMNS.filter((c) => !hidden.includes(c.id)).map(
              (c) => c.id as string
            ),
          showColumns
        }
      >
        {#each COLUMNS as c (c.id)}
          <ToggleGroupItem
            class="rounded-full text-muted-foreground data-[state=on]:text-foreground"
            disabled={c.id === "message"}
            value={c.id}
            >{c.label}</ToggleGroupItem
          >
        {/each}
      </ToggleGroupRoot>
      <div class="-mx-2.5 mt-1 border-t border-border px-0.5 pt-1.5">
        <Button
          class="w-full justify-start"
          onclick={toggleLive}
          variant="ghost"
          >{#if paused}
            <Play />Resume live updates
          {:else}
            <Pause />Pause live updates
          {/if}</Button
        >
        <Button
          class="w-full justify-start"
          disabled={clearing}
          onclick={() => {
            clearTarget = {
              id: tunnelId,
              name: tunnelId
                ? (names[tunnelId] ?? tunnelId)
                : "all projects / tunnels",
            };
            headerMenu = null;
            clearOpen = true;
            actionError = null;
          }}
          variant="ghost"
          ><Trash2 />Clear log history…</Button
        >
      </div>
    </OptionsMenu>
  </Toolbar>
{/snippet}

<div class="relative flex h-full flex-col">
  <div
    aria-hidden="true"
    class="pointer-events-none absolute inset-x-0 bottom-0 top-(--toolbar-h)"
    bind:this={filterBoundary}
  ></div>
  {@render logToolbar()}

  {@render logFilterSummary()}
  {#if actionError && !clearOpen}
    <div
      class="border-b border-border px-3 py-2 text-xs text-destructive"
      role="alert"
    >
      {actionError}
    </div>
  {/if}

  <div class="flex min-h-0 flex-1 flex-col">
    {#if queryStatus.loading && lines.length === 0}
      <StatePanel kind="loading" title="Loading logs…" />
    {:else if queryStatus.failure}
      <StatePanel
        kind="error"
        message={queryStatus.failure}
        title="Couldn't load logs"
        ><Button onclick={load} variant="outline">Try again</Button></StatePanel
      >
    {:else if lines.length === 0}
      <StatePanel kind="empty" message={emptyMessage} title={emptyTitle} />
    {:else}
      <LogList
        cap={MAX}
        {names}
        showLevel={!hidden.includes("level")}
        showTime={!hidden.includes("time")}
        showTunnel={!hidden.includes("tunnel")}
        bind:this={list}
        bind:lines
      />
    {/if}
  </div>
</div>

<ClearHistoryDialog
  {clearing}
  failure={actionError}
  kind="logs"
  onclear={clear}
  target={clearTarget?.name}
  trigger={() => optionsTrigger}
  bind:open={clearOpen}
/>
