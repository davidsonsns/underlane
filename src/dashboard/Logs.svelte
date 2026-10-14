<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { store } from "../lib/store.svelte";
  import { api, errorText } from "../lib/api";
  import type { LogLine } from "../lib/types";
  import SlidersHorizontal from "@lucide/svelte/icons/sliders-horizontal";
  import Pause from "@lucide/svelte/icons/pause";
  import Play from "@lucide/svelte/icons/play";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import X from "@lucide/svelte/icons/x";
  import { Button } from "$lib/components/ui/button/index.js";
  import * as ToggleGroup from "$lib/components/ui/toggle-group/index.js";
  import * as Popover from "$lib/components/ui/popover/index.js";
  import * as AlertDialog from "$lib/components/ui/alert-dialog/index.js";
  import Toolbar from "$lib/components/app/Toolbar.svelte";
  import IconAction from "$lib/components/app/IconAction.svelte";
  import SearchAction from "$lib/components/app/SearchAction.svelte";
  import StatePanel from "$lib/components/app/StatePanel.svelte";
  import { returnFocusOnClose, returnFocusTo } from "$lib/focus";
  import { logLevels, matchesLogFilters } from "$lib/log-filters";
  import LogFilters from "./LogFilters.svelte";
  import LogList from "./LogList.svelte";

  let { focusId = null }: { focusId?: string | null } = $props();
  let tunnelId = $state<string | null>(null);
  $effect(() => { tunnelId = focusId; });
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
  /** Only the first load shows the loading panel; later reloads keep the current view. */
  let loading = $state(true);
  let error = $state<string | null>(null);
  let actionError = $state<string | null>(null);
  let clearing = $state(false);
  let clearOpen = $state(false);
  let clearTarget = $state<{ id: string | null; name: string } | null>(null);
  let list = $state<LogList>();
  const MAX = 3000;
  const activeLevels = $derived(logLevels(levels));
  const filtered = $derived(Boolean(tunnelId || levels.length || search));
  const names = $derived(Object.fromEntries(store.tunnels.map((t) => [t.id, t.name])));

  const COLUMNS = [{ id: "time", label: "Time" }, { id: "level", label: "Level" }, { id: "tunnel", label: "Project / tunnel" }, { id: "message", label: "Message" }] as const;
  const COLUMN_KEY = "underlane.logs.hidden-columns";
  let hidden = $state<string[]>(loadHidden());
  function loadHidden(): string[] {
    try {
      const saved: unknown = JSON.parse(localStorage.getItem(COLUMN_KEY) ?? "[]");
      if (Array.isArray(saved)) return COLUMNS.filter((c) => c.id !== "message" && saved.includes(c.id)).map((c) => c.id);
    } catch {}
    return [];
  }
  function showColumns(ids: string[]) {
    hidden = COLUMNS.filter((c) => c.id !== "message" && !ids.includes(c.id)).map((c) => c.id);
    try { localStorage.setItem(COLUMN_KEY, JSON.stringify(hidden)); } catch {}
  }

  function setHeaderMenu(name: "filters" | "options", open: boolean) {
    if (open) headerMenu = name;
    else if (headerMenu === name) headerMenu = null;
  }

  function matches(l: LogLine, id = tunnelId, lv = activeLevels, q = search) {
    return matchesLogFilters(l, id, lv, q);
  }

  let loadGen = 0;
  let loadKey: string | undefined;
  let inflight: Promise<void> | null = null;
  let loadEvents = new Map<number, LogLine>();
  function load(): Promise<void> {
    if (clearing) return Promise.resolve();
    const lv = [...activeLevels];
    const id = tunnelId;
    const searchText = search;
    const key = JSON.stringify([id, lv, searchText]);
    if (key === loadKey && inflight) return inflight;
    if (key !== loadKey) { loadGen++; loadKey = key; list?.discardHeld(); }
    const g = loadGen;
    const current = () => g === loadGen;
    const events = new Map<number, LogLine>();
    loadEvents = events;
    error = null;
    const run = (async () => {
      try {
        const r = await api.queryLogs({ tunnelId: id, levels: lv.length === LEVELS.length ? [] : lv, search: searchText, limit: 1000 });
        if (current()) {
          lines = [...r.filter((l) => !events.has(l.id)), ...events.values()].filter((l) => matches(l, id, lv, searchText)).sort((a, b) => a.id - b.id).slice(-MAX);
          list?.reset();
        }
      } catch (e) {
        if (current()) error = errorText(e);
      } finally {
        if (current()) loading = false;
      }
    })();
    inflight = run;
    run.finally(() => { if (inflight === run) inflight = null; });
    return run;
  }

  $effect(() => {
    void tunnelId;
    void search;
    void activeLevels.join();
    const t = setTimeout(load, search ? 180 : 0);
    return () => clearTimeout(t);
  });

  onMount(() => {
    const un = listen<LogLine[]>("logs", (e) => {
      if (paused || clearing) return;
      const matching = e.payload.filter((l) => matches(l));
      if (inflight) {
        for (const l of matching) loadEvents.set(l.id, l);
        while (loadEvents.size > MAX) loadEvents.delete(loadEvents.keys().next().value!);
      }
      const last = lines.at(-1)?.id ?? 0;
      const add = matching.filter((l) => l.id > last);
      if (!add.length) return;
      if (list) list.append(add);
      else lines = [...lines, ...add].slice(-MAX);
    });
    return () => un.then((f) => f());
  });

  function toggleLive() {
    paused = !paused;
    headerMenu = null;
    list?.discardHeld();
    if (paused) {
      // Freeze an already-started snapshot too; recording in the backend continues.
      loadGen++; loadKey = undefined; inflight = null; loading = false;
    } else void load();
  }

  async function clear() {
    if (!clearTarget || clearing) return;
    const target = clearTarget;
    clearing = true;
    actionError = null;
    loadGen++; loadKey = undefined; inflight = null;
    list?.discardHeld();
    try {
      await api.clearLogs(target.id);
      if (target.id === tunnelId) lines = [];
      clearOpen = false;
    } catch (e) {
      actionError = errorText(e);
    } finally {
      clearing = false;
      void load();
    }
  }
</script>

<div class="relative flex h-full flex-col">
  <div class="pointer-events-none absolute inset-x-0 bottom-0 top-(--toolbar-h)" bind:this={filterBoundary} aria-hidden="true"></div>
  <Toolbar title="Logs" subtitle={paused ? "Live updates paused" : ""}>
    <SearchAction label="Search logs" bind:value={search} bind:open={searchOpen} bind:this={searchAction} />
    <LogFilters bind:levels bind:tunnelId boundary={filterBoundary} bind:open={() => headerMenu === "filters", (value) => setHeaderMenu("filters", value)} />
    <span class="mx-0.5 h-4 border-l border-border" aria-hidden="true"></span>
    <Popover.Root bind:open={() => headerMenu === "options", (value) => setHeaderMenu("options", value)}>
      <Popover.Trigger>
        {#snippet child({ props })}<IconAction {...props} bind:ref={optionsTrigger} label="Columns and actions"><SlidersHorizontal /></IconAction>{/snippet}
      </Popover.Trigger>
      <Popover.Content align="end" class="w-64 gap-2 p-2.5" {...returnFocusOnClose(() => optionsTrigger)}>
        <span id="log-columns-label" class="text-xs text-muted-foreground">Display columns</span>
        <ToggleGroup.Root type="multiple" spacing={1} aria-labelledby="log-columns-label" class="flex-wrap" bind:value={() => COLUMNS.filter((c) => !hidden.includes(c.id)).map((c) => c.id as string), showColumns}>
          {#each COLUMNS as c (c.id)}<ToggleGroup.Item value={c.id} disabled={c.id === "message"} class="rounded-full text-muted-foreground data-[state=on]:text-foreground">{c.label}</ToggleGroup.Item>{/each}
        </ToggleGroup.Root>
        <div class="-mx-2.5 mt-1 border-t border-border px-0.5 pt-1.5">
          <Button variant="ghost" class="w-full justify-start" onclick={toggleLive}>{#if paused}<Play />Resume live updates{:else}<Pause />Pause live updates{/if}</Button>
          <Button variant="ghost" class="w-full justify-start" disabled={clearing} onclick={() => { clearTarget = { id: tunnelId, name: tunnelId ? names[tunnelId] ?? tunnelId : "all projects / tunnels" }; headerMenu = null; clearOpen = true; actionError = null; }}><Trash2 />Clear log history…</Button>
        </div>
      </Popover.Content>
    </Popover.Root>
  </Toolbar>

  {#if filtered}
    <div class="flex min-h-11 flex-none flex-wrap items-center gap-2 border-b-[0.5px] border-border px-3 py-1.5">
      {#if tunnelId}
        <div class="flex items-center gap-1 rounded-md border border-input bg-fill/50 pl-2 text-xs">
          <span class="text-muted-foreground">Project / tunnel</span><span class="max-w-48 truncate">{names[tunnelId] ?? tunnelId}</span><IconAction label="Remove project or tunnel filter" size="icon-sm" onclick={() => tunnelId = null}><X /></IconAction>
        </div>
      {/if}
      {#if levels.length}
        <div class="flex items-center gap-1 rounded-md border border-input bg-fill/50 pl-2 text-xs" title={levels.join(", ")}>
          <span class="text-muted-foreground">Level</span><span class="max-w-48 truncate">{levels.join(", ")}</span><IconAction label="Remove level filter" size="icon-sm" onclick={() => levels = []}><X /></IconAction>
        </div>
      {/if}
      {#if search && !searchOpen}
        <div class="flex items-center gap-1 rounded-md border border-input bg-fill/50 pl-2 text-xs" title={search}>
          <span class="text-muted-foreground">Search</span><button class="max-w-48 truncate rounded-sm kb-focus:ring-[3px] kb-focus:ring-ring/50 outline-none" onclick={() => searchAction?.focus()}>{search}</button><IconAction label="Remove search filter" size="icon-sm" onclick={() => search = ""}><X /></IconAction>
        </div>
      {/if}
      <Button variant="ghost" class="ml-auto text-xs" onclick={() => { search = ""; levels = []; tunnelId = null; }}>Reset filters</Button>
    </div>
  {/if}
  {#if actionError && !clearOpen}<div role="alert" class="border-b border-border px-3 py-2 text-xs text-destructive">{actionError}</div>{/if}

  <div class="flex min-h-0 flex-1 flex-col">
    {#if loading && lines.length === 0}
      <StatePanel kind="loading" title="Loading logs…" />
    {:else if error}
      <StatePanel kind="error" title="Couldn't load logs" message={error}><Button variant="outline" onclick={load}>Try again</Button></StatePanel>
    {:else if lines.length === 0}
      <StatePanel kind="empty" title={search || levels.length ? "No matching log lines" : paused ? "Live updates paused" : "No logs yet"} message={search || levels.length ? "Try a different search or level filter." : paused ? "Resume live updates from Columns and actions. Logs continue recording while paused." : "Start a tunnel and cloudflared's output streams here live."} />
    {:else}
      <LogList bind:this={list} bind:lines {names} showTunnel={!hidden.includes("tunnel")} showTime={!hidden.includes("time")} showLevel={!hidden.includes("level")} cap={MAX} />
    {/if}
  </div>
</div>

<AlertDialog.Root bind:open={clearOpen}>
  <AlertDialog.Content onCloseAutoFocus={returnFocusTo(() => optionsTrigger)} onEscapeKeydown={(e) => { if (clearing) e.preventDefault(); }} onInteractOutside={(e) => { if (clearing) e.preventDefault(); }}>
    <AlertDialog.Header>
      <AlertDialog.Title>Clear log history?</AlertDialog.Title>
      <AlertDialog.Description>Delete all log lines for “{clearTarget?.name}”, including lines hidden by filters. This can't be undone.</AlertDialog.Description>
    </AlertDialog.Header>
    {#if actionError}<p role="alert" class="text-xs text-destructive">{actionError}</p>{/if}
    <AlertDialog.Footer><AlertDialog.Cancel disabled={clearing}>Cancel</AlertDialog.Cancel><Button variant="destructive" disabled={clearing} onclick={clear}>{clearing ? "Clearing…" : "Clear history"}</Button></AlertDialog.Footer>
  </AlertDialog.Content>
</AlertDialog.Root>
