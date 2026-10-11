<script lang="ts">
  import { onMount } from "svelte";
  import { fly } from "svelte/transition";
  import { listen } from "@tauri-apps/api/event";
  import { store } from "../lib/store.svelte";
  import { api, errorText } from "../lib/api";
  import { motion } from "../lib/theme";
  import { formatBytes, formatClock, formatDuration, statusClass } from "../lib/format";
  import type { RequestSummary, RequestDetail as Detail } from "../lib/types";
  import Toolbar from "$lib/components/app/Toolbar.svelte";
  import TunnelPicker from "./TunnelPicker.svelte";
  import StatePanel from "$lib/components/app/StatePanel.svelte";
  import Icon from "$lib/components/app/Icon.svelte";
  import RequestDetail from "./RequestDetail.svelte";

  let { focusId = null }: { focusId?: string | null } = $props();
  let tunnelId = $state<string | null>(null);
  let rows = $state<RequestSummary[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let search = $state("");
  let selected = $state<string | null>(null);
  let detail = $state<Detail | null>(null);
  let detailError = $state<string | null>(null);
  let fresh = $state<Record<string, boolean>>({});

  const inspectable = (t: { kind: string }) => t.kind !== "token";
  $effect(() => {
    if (focusId) tunnelId = focusId;
    else if (!tunnelId) tunnelId = store.tunnels.find((t) => t.inspect)?.id ?? store.tunnels.find(inspectable)?.id ?? null;
  });
  const tunnel = $derived(store.tunnels.find((t) => t.id === tunnelId) ?? null);

  let loadedFor: string | null | undefined = undefined;
  async function load() {
    if (loadedFor !== tunnelId) {
      // Switching tunnels: don't keep showing another tunnel's request.
      selected = null;
      detail = null;
      detailError = null;
      loadedFor = tunnelId;
    }
    loading = true;
    error = null;
    try {
      rows = await api.listRequests(tunnelId, search);
    } catch (e) {
      error = errorText(e);
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    void tunnelId;
    void search;
    const t = setTimeout(load, search ? 180 : 0);
    return () => clearTimeout(t);
  });

  async function select(id: string) {
    selected = id;
    detailError = null;
    try {
      detail = await api.getRequest(id);
    } catch (e) {
      detailError = errorText(e);
    }
  }

  let initialSide = $state<"req" | "resp">("resp");
  /** Used by the e2e check: open the newest request's detail on the given tab. */
  export async function selectFirst(side: "req" | "resp" = "resp") {
    initialSide = side;
    await load();
    if (rows[0]) await select(rows[0].id);
  }

  onMount(() => {
    const un = listen<RequestSummary>("request", (e) => {
      const r = e.payload;
      if (tunnelId && r.tunnel_id !== tunnelId) return;
      if (search) return;
      const i = rows.findIndex((x) => x.id === r.id);
      if (i >= 0) rows[i] = r;
      else {
        rows = [r, ...rows].slice(0, 500);
        fresh[r.id] = true;
        setTimeout(() => delete fresh[r.id], 1200);
      }
      if (selected === r.id) select(r.id);
    });
    return () => un.then((f) => f());
  });

  async function clear() {
    await api.clearRequests(tunnelId);
    rows = [];
    detail = null;
    selected = null;
  }
</script>

<div class="page">
  <Toolbar title="Requests" subtitle={tunnel ? (tunnel.inspect ? (tunnel.state.inspecting ? "Inspecting" : "Inspection on, starts with the tunnel") : "Inspection is off for this tunnel") : ""}>
    <TunnelPicker bind:value={tunnelId} filter={inspectable} />
    <div class="search">
      <Icon name="search" size={13} />
      <input class="field" placeholder="Filter path, method, status" bind:value={search} aria-label="Filter requests" />
    </div>
    <button class="btn" onclick={clear} disabled={!rows.length}>Clear</button>
  </Toolbar>

  {#if !tunnel && !store.loading}
    <div class="center">
      <StatePanel kind="empty" title="No tunnels to inspect" message="Add a config-file or quick tunnel, then turn on request inspection." />
    </div>
  {:else if tunnel && !tunnel.inspect}
    <div class="center">
      <StatePanel kind="empty" title="Inspection is off" message="Turn it on to capture each request's headers and bodies through Burrow's local proxy. The tunnel restarts to apply it.">
        <button class="btn primary" onclick={async () => { await api.updateTunnel(tunnel.id, { inspect: true }); await store.refresh(); }}>Turn on inspection</button>
      </StatePanel>
    </div>
  {:else}
    <div class="split">
      <div class="table-wrap">
        <div class="thead" aria-hidden="true">
          <span>Method</span><span>Path</span><span class="num">Status</span><span class="num">Time</span><span class="num">Size</span><span class="num">At</span>
        </div>
        {#if loading && rows.length === 0}
          <StatePanel kind="loading" title="Loading requests…" />
        {:else if error}
          <StatePanel kind="error" title="Couldn't load requests" message={error}><button class="btn" onclick={load}>Try again</button></StatePanel>
        {:else if rows.length === 0}
          <StatePanel kind="empty" title={search ? "No matching requests" : "Waiting for requests"} message={search ? "Try a different filter." : tunnel?.state.status === "connected" ? `Requests to ${tunnel.state.public_url ?? "this tunnel"} will appear here as they happen.` : "Start the tunnel, then send it some traffic."} />
        {:else}
          <div class="rows" aria-label="Requests">
            {#each rows as r (r.id)}
              <button
                class="tr"
                class:sel={r.id === selected}
                class:fresh={fresh[r.id]}
                aria-current={r.id === selected ? "true" : undefined}
                aria-label="{r.method} {r.path}, status {r.status ?? 'none'}"
                onclick={() => select(r.id)}
                in:fly={{ y: -4, duration: motion(160) }}
              >
                <span class="method m-{r.method.toLowerCase()}">{r.kind === "websocket" ? "WS" : r.method}</span>
                <span class="path mono" title={r.path}>{r.path}</span>
                <span class="num status s-{statusClass(r.status)}">{r.status ?? "—"}</span>
                <span class="num">{formatDuration(r.duration_ms)}</span>
                <span class="num">{formatBytes(r.resp_size)}</span>
                <span class="num faint">{formatClock(r.ts).slice(0, 8)}</span>
              </button>
            {/each}
          </div>
        {/if}
      </div>
      <div class="detail">
        {#if detailError}
          <StatePanel kind="error" title="Couldn't load this request" message={detailError} />
        {:else if detail}
          {#key `${detail.id}:${initialSide}`}<RequestDetail {detail} side={initialSide} />{/key}
        {:else}
          <StatePanel kind="empty" title="Select a request" message="Headers and bodies show here." />
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .page {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  .center {
    flex: 1;
    display: grid;
    place-items: center;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 4px;
    color: var(--text-3);
  }
  .search .field {
    width: 200px;
  }
  .split {
    flex: 1;
    display: grid;
    grid-template-columns: minmax(420px, 1.25fr) minmax(340px, 1fr);
    min-height: 0;
  }
  .table-wrap {
    display: flex;
    flex-direction: column;
    min-height: 0;
    border-right: 0.5px solid var(--line);
  }
  .thead,
  .tr {
    display: grid;
    grid-template-columns: 46px minmax(90px, 1fr) 44px 56px 56px 54px;
    gap: 6px;
    align-items: center;
    padding: 0 12px;
  }
  .thead {
    height: 28px;
    font-size: var(--text-xs);
    color: var(--text-2);
    border-bottom: 0.5px solid var(--line);
    flex: none;
  }
  .rows {
    overflow-y: auto;
    flex: 1;
  }
  .tr {
    width: 100%;
    height: 28px;
    border: 0;
    background: transparent;
    text-align: left;
    font-size: var(--text-callout);
    font-variant-numeric: tabular-nums;
  }
  .tr:nth-child(even) {
    background: color-mix(in srgb, var(--hover) 50%, transparent);
  }
  .tr:hover {
    background: var(--hover);
  }
  .tr.sel {
    background: var(--selected);
  }
  .tr.fresh {
    animation: arrive 1.2s ease-out;
  }
  @keyframes arrive {
    from { box-shadow: inset 3px 0 0 var(--tint); }
    to { box-shadow: inset 3px 0 0 transparent; }
  }
  .num {
    text-align: right;
  }
  .path {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 11.5px;
  }
  .method {
    font-weight: 600;
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .m-post, .m-put, .m-patch { color: color-mix(in srgb, var(--tint) 80%, var(--text-1)); }
  .m-delete { color: var(--bad); }
  .status { font-weight: 600; }
  .s-ok { color: var(--ok); }
  .s-redirect { color: var(--text-2); }
  .s-client { color: var(--degraded); }
  .s-server { color: var(--bad); }
  .detail {
    min-width: 0;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }
</style>
