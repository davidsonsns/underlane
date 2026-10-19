<script lang="ts">
  import Pause from "@lucide/svelte/icons/pause";
  import Play from "@lucide/svelte/icons/play";

  import Trash2 from "@lucide/svelte/icons/trash-2";

  import { listen } from "@tauri-apps/api/event";
  import { onDestroy, onMount, untrack } from "svelte";
  import { cubicOut } from "svelte/easing";
  import {
    COLUMNS,
    type ColumnId,
    columnPrefs,
    gridTemplate,
    setHidden,
  } from "$lib/columns.svelte";

  import SearchAction from "$lib/components/app/SearchAction.svelte";
  import StatePanel from "$lib/components/app/StatePanel.svelte";
  import Toolbar from "$lib/components/app/Toolbar.svelte";

  import { Button } from "$lib/components/ui/button/index.js";

  import {
    Item as ToggleGroupItem,
    Root as ToggleGroupRoot,
  } from "$lib/components/ui/toggle-group/index.js";

  import { formatAt } from "$lib/format";
  import { matchesRequestFilters } from "$lib/request-filters";
  import { motion } from "$lib/theme";
  import { api, errorText } from "../lib/api";
  import { latest } from "../lib/latest";
  import { store } from "../lib/store.svelte";
  import type { RequestDetail as Detail, RequestSummary } from "../lib/types";
  import ClearHistoryDialog from "./ClearHistoryDialog.svelte";
  import FilterChip from "./FilterChip.svelte";
  import { queryTracker } from "./guarded-query";
  import OptionsMenu from "./OptionsMenu.svelte";
  import RequestDetail from "./RequestDetail.svelte";
  import RequestFilters from "./RequestFilters.svelte";
  import RequestList, { columns } from "./RequestList.svelte";
  import { track } from "./reactivity";
  import {
    inspectionSubtitle as getInspectionSubtitle,
    requestEmptyMessage,
  } from "./view-state";

  const CAP = 500;
  let {
    focusId = null,
  }: {
    focusId?: string | null;
  } = $props();
  let tunnelId = $state<string | null>(null);
  let rows = $state<RequestSummary[]>([]);
  /** Only the first load shows the queryStatus.loading panel; later reloads keep the current view until they land. */
  const queryStatus = $state<{ failure: string | null; loading: boolean }>({
    failure: null,
    loading: true,
  });
  let search = $state("");
  let searchOpen = $state(false);
  let searchAction = $state<SearchAction>();
  let filterBoundary = $state<HTMLDivElement | null>(null);
  let statuses = $state<number[]>([]);
  let methods = $state<string[]>([]);
  const filtered = $derived(
    Boolean(search || statuses.length || methods.length || tunnelId)
  );
  const matchingFiltered = $derived(
    Boolean(search || statuses.length || methods.length)
  );
  let headerMenu = $state<"filters" | "options" | null>(null);
  let optionsTrigger = $state<HTMLElement | null>(null);
  let paused = $state(false);
  let pauseKnown = $state(false);
  let pauseBusy = $state(false);
  let inspectionBusy = $state(false);
  let actionError = $state<string | null>(null);
  let clearOpen = $state(false);
  let clearTarget = $state<{ id: string | null; name: string } | null>(null);
  let clearing = $state(false);
  /** The active request, by id (02 §4.2): the list derives its index from `rows`. */
  let selected = $state<string | null>(null);
  let detail = $state<Detail | null>(null);
  let detailError = $state<string | null>(null);
  let list = $state<RequestList>();

  // At labels show the date for requests from another day: re-check each minute, so the
  // labels gain their date once the day turns.
  let now = $state(Date.now());
  onMount(() => {
    const t = setInterval(() => {
      now = Date.now();
    }, 60_000);
    return () => clearInterval(t);
  });
  const shown = $derived(
    COLUMNS.filter((c) => !columnPrefs.hidden.includes(c.id))
  );
  const template = $derived(
    gridTemplate(
      columnPrefs.hidden,
      rows.reduce((n, r) => Math.max(n, formatAt(r.ts, now).length), 2)
    )
  );

  const inspectable = (t: { kind: string }) => t.kind !== "token";
  $effect(() => {
    tunnelId = focusId;
  });
  const tunnel = $derived(store.tunnels.find((t) => t.id === tunnelId) ?? null);
  const canInspect = $derived(tunnel !== null && inspectable(tunnel));

  let pauseGen = 0;
  $effect(() => {
    const id = tunnelId;
    pauseGen += 1;
    const g = pauseGen;
    paused = false;
    pauseKnown = false;
    actionError = null;
    if (!(id && canInspect)) {
      return;
    }
    api.requestCapturePaused(id).then(
      (value) => {
        if (g === pauseGen) {
          paused = value;
          pauseKnown = true;
        }
      },
      (e) => {
        if (g === pauseGen) {
          actionError = errorText(e);
        }
      }
    );
  });

  function setHeaderMenu(name: "filters" | "options", open: boolean) {
    if (open) {
      headerMenu = name;
    } else if (headerMenu === name) {
      headerMenu = null;
    }
  }

  async function toggleCapture() {
    if (!(tunnelId && canInspect && tunnel?.inspect) || pauseBusy) {
      return;
    }
    const id = tunnelId;
    const next = !paused;
    pauseBusy = true;
    actionError = null;
    try {
      await api.setRequestCapturePaused(id, next);
      if (id === tunnelId) {
        pauseGen += 1;
        const g = pauseGen;
        const current = await api.requestCapturePaused(id);
        if (id !== tunnelId) {
          return;
        }
        if (id === tunnelId && g === pauseGen) {
          paused = current;
          pauseKnown = true;
        }
        list?.discardPending();
        if (headerMenu === "options") {
          headerMenu = null;
        }
        if (!paused) {
          await load();
        }
      }
    } catch (e) {
      if (id === tunnelId) {
        actionError = errorText(e);
      }
    } finally {
      pauseBusy = false;
    }
  }

  async function enableInspection() {
    if (!(tunnel && canInspect) || tunnel.inspect || inspectionBusy) {
      return;
    }
    const { id } = tunnel;
    inspectionBusy = true;
    actionError = null;
    try {
      await api.updateTunnel(id, { inspect: true });
      await store.refresh();
    } catch (e) {
      if (id === tunnelId) {
        actionError = errorText(e);
      }
    } finally {
      inspectionBusy = false;
    }
  }

  // The detail load: superseded by every select(), and cancelled on tunnel change, Clear and
  // unmount. A getRequest answer (or queryStatus.failure) that arrives after any of those is dropped (03 R-23),
  // and a superseded select() still resolves.
  const detailLoad = latest();

  /** Make `id` the active request and load its detail, debounced so a held arrow key
   *  doesn't flood IPC. */
  function select(id: string | null, delay = 100): Promise<void> {
    selected = id;
    if (id === null) {
      detailLoad.cancel();
      detail = null;
      detailError = null;
      return Promise.resolve();
    }
    return detailLoad.run(async (current) => {
      try {
        const d = await api.getRequest(id);
        if (current()) {
          detail = d;
          detailError = null;
        }
      } catch (e) {
        if (current()) {
          detailError = errorText(e);
        }
      }
    }, delay);
  }

  // An empty list (a filter with no match, Clear) has no active request, and RequestList, which
  // owns the fallback, isn't mounted to say so (02 §4.2). Clearing the filter doesn't bring the
  // old selection back: nothing is selected until the user picks a row.
  $effect(() => {
    if (rows.length === 0 && selected !== null) {
      untrack(() => select(null));
    }
  });

  let loadedFor: string | null | undefined;
  let loadGen = 0;
  let loadKey: string | undefined;
  const queries = queryTracker(queryStatus);
  let loadEvents = new Map<string, RequestSummary>();
  function load(): Promise<void> {
    if (clearing) {
      return Promise.resolve();
    }
    if (loadedFor !== tunnelId) {
      // Switching tunnels: don't keep showing another tunnel's request.
      select(null);
      loadedFor = tunnelId;
    }
    // Each change of tunnel or filter starts a new generation, so a slow answer for an older one
    // never lands, even after A -> B -> A. A load for the same generation while one is in flight
    // shares it, so the one selectFirst() awaits is never discarded by a concurrent twin.
    const key = JSON.stringify([tunnelId, search, statuses, methods]);
    if (key === loadKey && queries.active) {
      return queries.active;
    }
    if (key !== loadKey) {
      loadGen += 1;
      loadKey = key;
      list?.discardPending();
    }
    const g = loadGen;
    const current = () => g === loadGen;
    const t = tunnelId;
    const q = search;
    const statusFilter = [...statuses];
    const methodFilter = [...methods];
    const events = new Map<string, RequestSummary>();
    loadEvents = events;
    queryStatus.failure = null;
    return queries.run(async () => {
      const r = await api.listRequests(t, q, CAP, statusFilter, methodFilter);
      if (current()) {
        // Live completions arriving during the query supersede its older snapshot.
        rows = [...events.values(), ...r.filter((x) => !events.has(x.id))]
          .filter((x) => matchesRequestFilters(x, statusFilter, methodFilter))
          .sort((a, b) => b.ts - a.ts)
          .slice(0, CAP);
      }
    }, current);
  }

  $effect(() => {
    track(tunnelId);
    track(search);
    track(statuses);
    track(methods);
    const t = setTimeout(load, search ? 180 : 0);
    return () => clearTimeout(t);
  });

  // The detail's tab and body view belong to the pane, not to one request: they persist while
  // the selection moves. RequestDetail falls back (Pretty to Raw, anything to Hex for a binary
  // body) for a request that can't show the chosen view, without changing the choice.
  let side = $state<"req" | "resp">("resp");
  let mode = $state<"pretty" | "raw" | "hex">("pretty");
  /** Used by the e2e check: open the newest request's detail on the given tab. */
  export async function selectFirst(tab: "req" | "resp" = "resp") {
    side = tab;
    await load();
    if (rows[0]) {
      await select(rows[0].id, 0);
    }
  }

  onMount(() => {
    const unPause = listen<{ tunnel_id: string; paused: boolean }>(
      "request-capture-paused",
      (e) => {
        if (e.payload.tunnel_id !== tunnelId) {
          return;
        }
        pauseGen += 1;
        ({ paused } = e.payload);
        pauseKnown = true;
        list?.discardPending();
      }
    );
    const un = listen<RequestSummary>("request", function receiveRequest(e) {
      const r = e.payload;
      if (tunnelId && r.tunnel_id !== tunnelId) {
        return;
      }
      if (paused || clearing || search) {
        return;
      }
      if (queries.active) {
        loadEvents.set(r.id, r);
      }
      if (!matchesRequestFilters(r, statuses, methods)) {
        rows = rows.filter((x) => x.id !== r.id);
        return;
      }
      const i = rows.findIndex((x) => x.id === r.id);
      if (i >= 0) {
        rows[i] = r;
      } else if (list) {
        list.add(r); // batched, keeps the view when scrolled down
      } else {
        rows = [r, ...rows].slice(0, CAP);
      }
      if (selected === r.id) {
        select(r.id);
      }
    });
    return () => {
      un.then((f) => f());
      unPause.then((f) => f());
    };
  });

  onDestroy(() => detailLoad.cancel());

  // The detail pane's width, as a share of the split so it scales with the window. Dragging the
  // divider sets it; it's remembered per window in localStorage, like the hidden columns.
  const SHARE_KEY = "underlane.requests.detail-share";
  const SHARE = 0.44;
  const LIST_MIN = 420;
  const PANE_MIN = 340;
  let share = $state(loadShare());
  let split = $state<HTMLDivElement>();
  let dragging = $state(false);

  function loadShare(): number {
    try {
      const v = Number(localStorage.getItem(SHARE_KEY));
      if (v > 0 && v < 1) {
        return v;
      }
    } catch {
      /* Storage is optional; defaults remain usable. */
    }
    return SHARE;
  }

  function setShare(v: number, save = true) {
    const w = split?.clientWidth ?? 0;
    share =
      w > 0
        ? Math.min(
            Math.max(v, PANE_MIN / w),
            Math.max(PANE_MIN, w - LIST_MIN) / w
          )
        : v;
    if (!save) {
      return;
    }
    try {
      localStorage.setItem(SHARE_KEY, String(share));
    } catch {
      /* Storage is optional; defaults remain usable. */
    }
  }

  function ondividerdown(e: PointerEvent) {
    if (e.button !== 0) {
      return;
    }
    e.preventDefault();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    dragging = true;
  }

  function ondividermove(e: PointerEvent) {
    if (!(dragging && split)) {
      return;
    }
    const r = split.getBoundingClientRect();
    setShare((r.right - e.clientX) / r.width, false);
  }

  function ondividerup() {
    if (!dragging) {
      return;
    }
    dragging = false;
    setShare(share);
  }

  function ondividerkey(e: KeyboardEvent) {
    const step = e.shiftKey ? 0.1 : 0.02;
    if (e.key === "ArrowLeft") {
      setShare(share + step);
    } else if (e.key === "ArrowRight") {
      setShare(share - step);
    } else if (e.key === "Home") {
      setShare(SHARE);
    } else {
      return;
    }
    e.preventDefault();
  }

  function close() {
    select(null);
    list?.focus();
  }

  /** The detail pane opens from the right edge, pushing the list over. The content keeps its
   *  final width while the pane grows or shrinks, so it slides in clipped instead of rewrapping. */
  function reveal(node: HTMLElement) {
    const w = node.offsetWidth;
    const inner = node.firstElementChild as HTMLElement;
    return {
      css: (t: number) =>
        `min-width: 0; width: ${t * w}px; opacity: ${Math.min(1, t * 2)}`,
      duration: motion(200),
      easing: cubicOut,
      tick: (t: number) => {
        inner.style.width = t < 1 ? `${w}px` : "";
      },
    };
  }

  async function clear() {
    if (!clearTarget || clearing) {
      return;
    }
    const target = clearTarget;
    clearing = true;
    actionError = null;
    detailLoad.cancel();
    // A list load still in flight answers from before the Clear: drop it.
    loadGen += 1;
    loadKey = undefined;
    queries.cancel();
    list?.discardPending();
    try {
      await api.clearRequests(target.id);
      if (target.id === tunnelId) {
        rows = [];
        select(null);
      }
      clearOpen = false;
    } catch (e) {
      actionError = errorText(e);
    } finally {
      clearing = false;
      load();
    }
  }

  const inspectionSubtitle = $derived(
    getInspectionSubtitle(canInspect ? tunnel : null, paused)
  );
  const emptyMessage = $derived(
    requestEmptyMessage(tunnel, matchingFiltered, paused)
  );
  const emptyTitle = $derived.by(() => {
    if (matchingFiltered) {
      return "No matching requests";
    }
    if (paused) {
      return "Capture paused";
    }
    return "Waiting for requests";
  });
</script>

{#snippet requestDetails()}
  {#if selected !== null}
    <div
      class="relative max-w-[calc(100%-420px)] min-w-85 flex-none overflow-hidden border-l-[0.5px] border-border"
      style:width="{share * 100}%"
      in:reveal
      out:reveal
    >
      <div class="flex h-full min-w-0 flex-col">
        {#if detailError}
          <StatePanel
            kind="error"
            message={detailError}
            title="Couldn't load this request"
            ><Button onclick={close} variant="outline"
              >Close</Button
            ></StatePanel
          >
        {:else if detail}
          <!-- Remounted per request so its panel starts scrolled to the top; tab and view persist. -->
          {#key detail.id}
            <RequestDetail {detail} onclose={close} bind:mode bind:side />
          {/key}
        {/if}
      </div>
      <!-- The divider: drag, or arrow keys (Shift for bigger steps); double-click or Home resets.
               A focusable separator is a widget (ARIA window splitter), which the lint doesn't know. -->
      <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
      <!-- biome-ignore lint/a11y/useSemanticElements: A focusable ARIA separator implements the draggable window splitter. -->
      <div
        aria-label="Resize request details"
        aria-orientation="vertical"
        aria-valuemax={100}
        aria-valuemin={0}
        aria-valuenow={Math.round(share * 100)}
        class={[
          "absolute inset-y-0 left-0 z-10 w-1.5 cursor-col-resize outline-none",
          "after:absolute after:inset-y-0 after:left-0 after:w-0.5 after:transition-colors after:duration-150 motion-reduce:after:transition-none",
          "hover:after:bg-tint/60 kb-focus:after:bg-tint",
          dragging && "after:bg-tint",
        ]}
        ondblclick={() => setShare(SHARE)}
        onkeydown={ondividerkey}
        onpointercancel={ondividerup}
        onpointerdown={ondividerdown}
        onpointermove={ondividermove}
        onpointerup={ondividerup}
        role="separator"
        tabindex="0"
      ></div>
    </div>
  {/if}
{/snippet}

{#snippet requestRows()}
  <div class="flex min-h-0 min-w-105 flex-1 flex-col">
    <div
      aria-hidden="true"
      class="{columns} h-7 flex-none pr-[calc(--spacing(3)+var(--scrollbar-size))] border-b-[0.5px] border-border text-xs text-muted-foreground"
      style:grid-template-columns={template}
    >
      {#each shown as c (c.id)}
        <span class:text-right={"end" in c}>{c.label}</span>
      {/each}
    </div>
    {#if queryStatus.loading && rows.length === 0}
      <StatePanel kind="loading" title="Loading requests…" />
    {:else if queryStatus.failure}
      <StatePanel
        kind="error"
        message={queryStatus.failure}
        title="Couldn't load requests"
        ><Button onclick={load} variant="outline">Try again</Button></StatePanel
      >
    {:else if rows.length === 0}
      <StatePanel kind="empty" message={emptyMessage} title={emptyTitle} />
    {:else}
      <RequestList
        cap={CAP}
        {now}
        onselect={(id) => select(id)}
        {selected}
        {template}
        bind:this={list}
        bind:rows
      />
    {/if}
  </div>
{/snippet}

{#snippet requestFilterSummary()}
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
          value={tunnel?.name ?? tunnelId}
        />
      {/if}
      <FilterChip
        hidden={!search || searchOpen}
        label="Search"
        onfocus={() => searchAction?.focus()}
        bind:value={search}
      />
      {#if statuses.length}
        <FilterChip
          label="Status"
          onremove={() => {
            statuses = [];
          }}
          removeLabel="Remove status filter"
          values={statuses}
        />
      {/if}
      {#if methods.length}
        <FilterChip
          label="Method"
          onremove={() => {
            methods = [];
          }}
          removeLabel="Remove method filter"
          values={methods}
        />
      {/if}
      <Button
        class="ml-auto text-xs"
        onclick={() => {
          search = "";
          statuses = [];
          methods = [];
          tunnelId = null;
        }}
        variant="ghost"
        >Reset filters</Button
      >
    </div>
  {/if}
{/snippet}

{#snippet requestToolbar()}
  <Toolbar subtitle={inspectionSubtitle} title="Requests">
    <SearchAction
      label="Search requests"
      bind:this={searchAction}
      bind:open={searchOpen}
      bind:value={search}
    />
    <RequestFilters
      boundary={filterBoundary}
      bind:methods
      bind:open={
        () => headerMenu === "filters",
        (value) => setHeaderMenu("filters", value)
      }
      bind:statuses
      bind:tunnelId
    />
    <span aria-hidden="true" class="mx-0.5 h-4 border-l border-border"></span>
    <OptionsMenu bind:menu={headerMenu} bind:trigger={optionsTrigger}>
      <span class="text-xs text-muted-foreground" id="columns-label"
        >Display columns</span
      >
      <ToggleGroupRoot
        aria-labelledby="columns-label"
        class="flex-wrap"
        spacing={1}
        type="multiple"
        bind:value={
          () => shown.map((c) => c.id as string),
          (v: string[]) =>
    setHidden(
      COLUMNS.filter((c) => !("always" in c || v.includes(c.id))).map(
        (c) => c.id as ColumnId
      )
    )
        }
      >
        {#each COLUMNS as c (c.id)}
          <ToggleGroupItem
            class="rounded-full text-muted-foreground data-[state=on]:text-foreground"
            disabled={"always" in c}
            value={c.id}
            >{c.label}</ToggleGroupItem
          >
        {/each}
      </ToggleGroupRoot>
      <div class="-mx-2.5 mt-1 border-t border-border px-0.5 pt-1.5">
        <Button
          class="w-full justify-start"
          disabled={!(canInspect && tunnel?.inspect && pauseKnown) || pauseBusy}
          onclick={toggleCapture}
          variant="ghost"
          >{#if paused}
            <Play />Resume capture
          {:else}
            <Pause />Pause capture
          {/if}</Button
        >
        <Button
          class="w-full justify-start"
          disabled={!store.tunnels.some(inspectable) || clearing}
          onclick={() => {
            clearTarget = {
              id: tunnelId,
              name: tunnel?.name ?? "all projects / tunnels",
            };
            headerMenu = null;
            clearOpen = true;
            actionError = null;
          }}
          variant="ghost"
          ><Trash2 />Clear request history…</Button
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
  {@render requestToolbar()}

  {@render requestFilterSummary()}
  {#if actionError && !clearOpen}
    <div
      class="border-b border-border px-3 py-2 text-xs text-destructive"
      role="alert"
    >
      {actionError}
    </div>
  {/if}

  {#if tunnel && !canInspect}
    <div class="grid flex-1 place-items-center">
      <StatePanel
        kind="empty"
        message="Request inspection is available for config-file and quick tunnels. Token tunnels manage their routes in Cloudflare."
        title="Request inspection unavailable"
      />
    </div>
  {:else if !(store.tunnels.some(inspectable) || store.loading)}
    <div class="grid flex-1 place-items-center">
      <StatePanel
        kind="empty"
        message="Add a config-file or quick tunnel, then turn on request inspection."
        title="No tunnels to inspect"
      />
    </div>
  {:else if tunnel && !tunnel.inspect}
    <div class="grid flex-1 place-items-center">
      <StatePanel
        kind="empty"
        message="Turn it on to capture each request's headers and bodies through Underlane's local proxy. The tunnel restarts to apply it."
        title="Inspection is off"
      >
        <Button disabled={inspectionBusy} onclick={enableInspection}
          >{inspectionBusy
            ? "Turning on inspection…"
            : "Turn on inspection"}</Button
        >
      </StatePanel>
    </div>
  {:else}
    <div
      class="flex min-h-0 flex-1"
      bind:this={split}
      class:cursor-col-resize={dragging}
      class:select-none={dragging}
    >
      {@render requestRows()}
      <!-- Open while a request is selected. While it closes, its content stays as it was. -->
      {@render requestDetails()}
    </div>
  {/if}
</div>

<ClearHistoryDialog
  {clearing}
  failure={actionError}
  kind="requests"
  onclear={clear}
  target={clearTarget?.name}
  trigger={() => optionsTrigger}
  bind:open={clearOpen}
/>
