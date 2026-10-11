<script lang="ts">
  import { onDestroy, onMount, untrack } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { store } from "../lib/store.svelte";
  import { api, errorText } from "../lib/api";
  import { latest } from "../lib/latest";
  import type { RequestSummary, RequestDetail as Detail } from "../lib/types";
  import SearchIcon from "@lucide/svelte/icons/search";
  import { Button } from "$lib/components/ui/button/index.js";
  import * as InputGroup from "$lib/components/ui/input-group/index.js";
  import Toolbar from "$lib/components/app/Toolbar.svelte";
  import StatePanel from "$lib/components/app/StatePanel.svelte";
  import TunnelPicker from "./TunnelPicker.svelte";
  import RequestList, { columns } from "./RequestList.svelte";
  import RequestDetail from "./RequestDetail.svelte";

  const CAP = 500;
  let { focusId = null }: { focusId?: string | null } = $props();
  let tunnelId = $state<string | null>(null);
  let rows = $state<RequestSummary[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let search = $state("");
  /** The active request, by id (02 §4.2): the list derives its index from `rows`. */
  let selected = $state<string | null>(null);
  let detail = $state<Detail | null>(null);
  let detailError = $state<string | null>(null);
  let list = $state<RequestList>();

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
  async function load() {
    if (loadedFor !== tunnelId) {
      // Switching tunnels: don't keep showing another tunnel's request.
      select(null);
      loadedFor = tunnelId;
    }
    // A load answers for the tunnel and filter it asked about: if either changed meanwhile, a
    // newer load is on its way and this result is dropped. Identical loads all apply, so the
    // one selectFirst() awaits is never discarded by a concurrent twin.
    const t = tunnelId;
    const q = search;
    const current = () => t === tunnelId && q === search;
    loading = true;
    error = null;
    try {
      const r = await api.listRequests(t, q);
      if (current()) rows = r;
    } catch (e) {
      if (current()) error = errorText(e);
    } finally {
      if (current()) loading = false;
    }
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

  async function clear() {
    detailLoad.cancel();
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
    <Button variant="outline" onclick={clear} disabled={!rows.length}>Clear</Button>
  </Toolbar>

  {#if !tunnel && !store.loading}
    <div class="grid flex-1 place-items-center">
      <StatePanel kind="empty" title="No tunnels to inspect" message="Add a config-file or quick tunnel, then turn on request inspection." />
    </div>
  {:else if tunnel && !tunnel.inspect}
    <div class="grid flex-1 place-items-center">
      <StatePanel kind="empty" title="Inspection is off" message="Turn it on to capture each request's headers and bodies through Burrow's local proxy. The tunnel restarts to apply it.">
        <Button onclick={async () => { await api.updateTunnel(tunnel.id, { inspect: true }); await store.refresh(); }}>Turn on inspection</Button>
      </StatePanel>
    </div>
  {:else}
    <div class="grid min-h-0 flex-1 grid-cols-[minmax(420px,1.25fr)_minmax(340px,1fr)]">
      <div class="flex min-h-0 flex-col border-r-[0.5px] border-border">
        <div class="{columns} h-7 flex-none border-b-[0.5px] border-border text-xs text-muted-foreground" aria-hidden="true">
          <span>Method</span><span>Path</span><span class="text-right">Status</span><span class="text-right">Time</span><span class="text-right">Size</span><span class="text-right">At</span>
        </div>
        {#if loading && rows.length === 0}
          <StatePanel kind="loading" title="Loading requests…" />
        {:else if error}
          <StatePanel kind="error" title="Couldn't load requests" message={error}><Button variant="outline" onclick={load}>Try again</Button></StatePanel>
        {:else if rows.length === 0}
          <StatePanel kind="empty" title={search ? "No matching requests" : "Waiting for requests"} message={search ? "Try a different filter." : tunnel?.state.status === "connected" ? `Requests to ${tunnel.state.public_url ?? "this tunnel"} will appear here as they happen.` : "Start the tunnel, then send it some traffic."} />
        {:else}
          <RequestList bind:this={list} bind:rows {selected} cap={CAP} onselect={(id) => select(id)} />
        {/if}
      </div>
      <div class="flex min-w-0 flex-col overflow-hidden">
        {#if detailError}
          <StatePanel kind="error" title="Couldn't load this request" message={detailError} />
        {:else if detail}
          <!-- Remounted per request so its panel starts scrolled to the top; tab and view persist. -->
          {#key detail.id}<RequestDetail {detail} bind:side bind:mode />{/key}
        {:else}
          <StatePanel kind="empty" title="Select a request" message="Headers and bodies show here." />
        {/if}
      </div>
    </div>
  {/if}
</div>
