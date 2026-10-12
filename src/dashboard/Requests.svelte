<script lang="ts">
  import { onDestroy, onMount, untrack } from "svelte";
  import { cubicOut } from "svelte/easing";
  import { listen } from "@tauri-apps/api/event";
  import { store } from "../lib/store.svelte";
  import { api, errorText } from "../lib/api";
  import { latest } from "../lib/latest";
  import type { RequestSummary, RequestDetail as Detail } from "../lib/types";
  import SearchIcon from "@lucide/svelte/icons/search";
  import Columns3 from "@lucide/svelte/icons/columns-3";
  import { Button } from "$lib/components/ui/button/index.js";
  import * as InputGroup from "$lib/components/ui/input-group/index.js";
  import * as Popover from "$lib/components/ui/popover/index.js";
  import * as ToggleGroup from "$lib/components/ui/toggle-group/index.js";
  import IconAction from "$lib/components/app/IconAction.svelte";
  import Toolbar from "$lib/components/app/Toolbar.svelte";
  import StatePanel from "$lib/components/app/StatePanel.svelte";
  import TunnelPicker from "./TunnelPicker.svelte";
  import RequestList, { columns } from "./RequestList.svelte";
  import RequestDetail from "./RequestDetail.svelte";
  import { COLUMNS, columnPrefs, gridTemplate, setHidden, type ColumnId } from "$lib/columns.svelte";
  import { formatAt } from "$lib/format";
  import { motion } from "$lib/theme";

  const CAP = 500;
  let { focusId = null }: { focusId?: string | null } = $props();
  let tunnelId = $state<string | null>(null);
  let rows = $state<RequestSummary[]>([]);
  /** Only the first load shows the loading panel; later reloads keep the current view until they land. */
  let loading = $state(true);
  let error = $state<string | null>(null);
  let search = $state("");
  /** The active request, by id (02 §4.2): the list derives its index from `rows`. */
  let selected = $state<string | null>(null);
  let detail = $state<Detail | null>(null);
  let detailError = $state<string | null>(null);
  let list = $state<RequestList>();

  // At labels show the date for requests from another day: re-check each minute, so the
  // labels gain their date once the day turns.
  let now = $state(Date.now());
  onMount(() => {
    const t = setInterval(() => (now = Date.now()), 60_000);
    return () => clearInterval(t);
  });
  const shown = $derived(COLUMNS.filter((c) => !columnPrefs.hidden.includes(c.id)));
  const template = $derived(gridTemplate(columnPrefs.hidden, rows.reduce((n, r) => Math.max(n, formatAt(r.ts, now).length), 2)));

  const inspectable = (t: { kind: string }) => t.kind !== "token";
  $effect(() => {
    if (focusId) tunnelId = focusId;
    else if (!tunnelId) tunnelId = store.tunnels.find((t) => t.inspect)?.id ?? store.tunnels.find(inspectable)?.id ?? null;
  });
  const tunnel = $derived(store.tunnels.find((t) => t.id === tunnelId) ?? null);

  // The detail load: superseded by every select(), and cancelled on tunnel change, Clear and
  // unmount. A getRequest answer (or error) that arrives after any of those is dropped (03 R-23),
  // and a superseded select() still resolves.
  const detailLoad = latest();

  /** Make `id` the active request and load its detail, debounced so a held arrow key
   *  doesn't flood IPC. */
  function select(id: string | null, delay = 100): Promise<void> {
    selected = id;
    if (id == null) {
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
        if (current()) detailError = errorText(e);
      }
    }, delay);
  }

  // An empty list (a filter with no match, Clear) has no active request, and RequestList, which
  // owns the fallback, isn't mounted to say so (02 §4.2). Clearing the filter doesn't bring the
  // old selection back: nothing is selected until the user picks a row.
  $effect(() => {
    if (rows.length === 0 && selected != null) untrack(() => select(null));
  });

  let loadedFor: string | null | undefined = undefined;
  let loadGen = 0;
  let loadKey: string | undefined;
  let inflight: Promise<void> | null = null;
  function load(): Promise<void> {
    if (loadedFor !== tunnelId) {
      // Switching tunnels: don't keep showing another tunnel's request.
      select(null);
      loadedFor = tunnelId;
    }
    // Each change of tunnel or filter starts a new generation, so a slow answer for an older one
    // never lands, even after A -> B -> A. A load for the same generation while one is in flight
    // shares it, so the one selectFirst() awaits is never discarded by a concurrent twin.
    const key = `${tunnelId}\u0000${search}`;
    if (key === loadKey && inflight) return inflight;
    if (key !== loadKey) {
      loadGen++;
      loadKey = key;
    }
    const g = loadGen;
    const current = () => g === loadGen;
    const t = tunnelId;
    const q = search;
    error = null;
    const run = (async () => {
      try {
        const r = await api.listRequests(t, q);
        if (current()) rows = r;
      } catch (e) {
        if (current()) error = errorText(e);
      } finally {
        if (current()) loading = false;
      }
    })();
    inflight = run;
    run.finally(() => {
      if (inflight === run) inflight = null;
    });
    return run;
  }

  $effect(() => {
    void tunnelId;
    void search;
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
    if (rows[0]) await select(rows[0].id, 0);
  }

  onMount(() => {
    const un = listen<RequestSummary>("request", (e) => {
      const r = e.payload;
      if (tunnelId && r.tunnel_id !== tunnelId) return;
      if (search) return;
      const i = rows.findIndex((x) => x.id === r.id);
      if (i >= 0) rows[i] = r;
      else if (list) list.add(r); // batched, keeps the view when scrolled down
      else rows = [r, ...rows].slice(0, CAP);
      if (selected === r.id) select(r.id);
    });
    return () => un.then((f) => f());
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
      if (v > 0 && v < 1) return v;
    } catch {}
    return SHARE;
  }

  function setShare(v: number, save = true) {
    const w = split?.clientWidth ?? 0;
    share = w > 0 ? Math.min(Math.max(v, PANE_MIN / w), Math.max(PANE_MIN, w - LIST_MIN) / w) : v;
    if (!save) return;
    try {
      localStorage.setItem(SHARE_KEY, String(share));
    } catch {}
  }

  function ondividerdown(e: PointerEvent) {
    if (e.button !== 0) return;
    e.preventDefault();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    dragging = true;
  }

  function ondividermove(e: PointerEvent) {
    if (!dragging || !split) return;
    const r = split.getBoundingClientRect();
    setShare((r.right - e.clientX) / r.width, false);
  }

  function ondividerup() {
    if (!dragging) return;
    dragging = false;
    setShare(share);
  }

  function ondividerkey(e: KeyboardEvent) {
    const step = e.shiftKey ? 0.1 : 0.02;
    if (e.key === "ArrowLeft") setShare(share + step);
    else if (e.key === "ArrowRight") setShare(share - step);
    else if (e.key === "Home") setShare(SHARE);
    else return;
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
      duration: motion(200),
      easing: cubicOut,
      css: (t: number) => `min-width: 0; width: ${t * w}px; opacity: ${Math.min(1, t * 2)}`,
      tick: (t: number) => (inner.style.width = t < 1 ? `${w}px` : ""),
    };
  }

  async function clear() {
    detailLoad.cancel();
    // A list load still in flight answers from before the Clear: drop it.
    loadGen++;
    loadKey = undefined;
    inflight = null;
    await api.clearRequests(tunnelId);
    rows = [];
    select(null);
  }
</script>

<div class="flex h-full flex-col">
  <Toolbar title="Requests" subtitle={tunnel ? (tunnel.inspect ? (tunnel.state.inspecting ? "Inspecting" : "Inspection on, starts with the tunnel") : "Inspection is off for this tunnel") : ""}>
    <TunnelPicker bind:value={tunnelId} filter={inspectable} />
    <InputGroup.Root class="w-50">
      <InputGroup.Addon><SearchIcon /></InputGroup.Addon>
      <InputGroup.Input placeholder="Filter path, method, status" bind:value={search} aria-label="Filter requests" autocorrect="off" autocapitalize="off" spellcheck="false" />
    </InputGroup.Root>
    <Popover.Root>
      <Popover.Trigger>
        {#snippet child({ props })}
          <IconAction {...props} label="Columns"><Columns3 /></IconAction>
        {/snippet}
      </Popover.Trigger>
      <Popover.Content align="end" class="w-auto max-w-72 gap-1.5 p-2.5">
        <span id="columns-label" class="text-xs text-muted-foreground">Columns</span>
        <ToggleGroup.Root
          type="multiple"
          spacing={1}
          aria-labelledby="columns-label"
          class="flex-wrap"
          bind:value={
            () => shown.map((c) => c.id as string),
            (v: string[]) => setHidden(COLUMNS.filter((c) => !("always" in c) && !v.includes(c.id)).map((c) => c.id as ColumnId))
          }
        >
          {#each COLUMNS as c (c.id)}
            {#if !("always" in c)}
              <ToggleGroup.Item value={c.id} class="text-muted-foreground data-[state=on]:text-foreground">{c.label}</ToggleGroup.Item>
            {/if}
          {/each}
        </ToggleGroup.Root>
      </Popover.Content>
    </Popover.Root>
    <Button variant="outline" onclick={clear} disabled={!rows.length}>Clear</Button>
  </Toolbar>

  {#if !tunnel && !store.loading}
    <div class="grid flex-1 place-items-center">
      <StatePanel kind="empty" title="No tunnels to inspect" message="Add a config-file or quick tunnel, then turn on request inspection." />
    </div>
  {:else if tunnel && !tunnel.inspect}
    <div class="grid flex-1 place-items-center">
      <StatePanel kind="empty" title="Inspection is off" message="Turn it on to capture each request's headers and bodies through Underlane's local proxy. The tunnel restarts to apply it.">
        <Button onclick={async () => { await api.updateTunnel(tunnel.id, { inspect: true }); await store.refresh(); }}>Turn on inspection</Button>
      </StatePanel>
    </div>
  {:else}
    <div class="flex min-h-0 flex-1" class:cursor-col-resize={dragging} class:select-none={dragging} bind:this={split}>
      <div class="flex min-h-0 min-w-105 flex-1 flex-col">
        <div class="{columns} h-7 flex-none pr-[calc(--spacing(3)+var(--scrollbar-size))] border-b-[0.5px] border-border text-xs text-muted-foreground" style:grid-template-columns={template} aria-hidden="true">
          {#each shown as c (c.id)}<span class:text-right={"end" in c}>{c.label}</span>{/each}
        </div>
        {#if loading && rows.length === 0}
          <StatePanel kind="loading" title="Loading requests…" />
        {:else if error}
          <StatePanel kind="error" title="Couldn't load requests" message={error}><Button variant="outline" onclick={load}>Try again</Button></StatePanel>
        {:else if rows.length === 0}
          <StatePanel kind="empty" title={search ? "No matching requests" : "Waiting for requests"} message={search ? "Try a different filter." : tunnel?.state.status === "connected" ? `Requests to ${tunnel.state.public_url ?? "this tunnel"} will appear here as they happen.` : "Start the tunnel, then send it some traffic."} />
        {:else}
          <RequestList bind:this={list} bind:rows {selected} {template} {now} cap={CAP} onselect={(id) => select(id)} />
        {/if}
      </div>
      <!-- Open while a request is selected. While it closes, its content stays as it was. -->
      {#if selected != null}
        <div
          class="relative max-w-[calc(100%-420px)] min-w-85 flex-none overflow-hidden border-l-[0.5px] border-border"
          style:width="{share * 100}%"
          in:reveal
          out:reveal
        >
          <div class="flex h-full min-w-0 flex-col">
            {#if detailError}
              <StatePanel kind="error" title="Couldn't load this request" message={detailError}><Button variant="outline" onclick={close}>Close</Button></StatePanel>
            {:else if detail}
              <!-- Remounted per request so its panel starts scrolled to the top; tab and view persist. -->
              {#key detail.id}<RequestDetail {detail} bind:side bind:mode onclose={close} />{/key}
            {/if}
          </div>
          <!-- The divider: drag, or arrow keys (Shift for bigger steps); double-click or Home resets.
               A focusable separator is a widget (ARIA window splitter), which the lint doesn't know. -->
          <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
          <div
            role="separator"
            aria-orientation="vertical"
            aria-label="Resize request details"
            aria-valuenow={Math.round(share * 100)}
            aria-valuemin={0}
            aria-valuemax={100}
            tabindex="0"
            class={[
              "absolute inset-y-0 left-0 z-10 w-1.5 cursor-col-resize outline-none",
              "after:absolute after:inset-y-0 after:left-0 after:w-0.5 after:transition-colors after:duration-150 motion-reduce:after:transition-none",
              "hover:after:bg-tint/60 kb-focus:after:bg-tint",
              dragging && "after:bg-tint",
            ]}
            onpointerdown={ondividerdown}
            onpointermove={ondividermove}
            onpointerup={ondividerup}
            onpointercancel={ondividerup}
            ondblclick={() => setShare(SHARE)}
            onkeydown={ondividerkey}
          ></div>
        </div>
      {/if}
    </div>
  {/if}
</div>
