<script lang="ts">
  import { onMount, tick } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { store } from "../lib/store.svelte";
  import { api, errorText } from "../lib/api";
  import { formatClock } from "../lib/format";
  import type { LogLine } from "../lib/types";
  import Toolbar from "./Toolbar.svelte";
  import TunnelPicker from "./TunnelPicker.svelte";
  import StatePanel from "../components/StatePanel.svelte";
  import Icon from "../components/Icon.svelte";

  let { focusId = null }: { focusId?: string | null } = $props();
  let tunnelId = $state<string | null>(null);
  $effect(() => {
    if (focusId) tunnelId = focusId;
  });
  const LEVELS = ["debug", "info", "warn", "error"] as const;
  let levels = $state<Record<string, boolean>>({ debug: true, info: true, warn: true, error: true });
  let search = $state("");
  let lines = $state<LogLine[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let follow = $state(true);
  let expanded = $state<number | null>(null);
  let scroller: HTMLDivElement | undefined = $state();
  const MAX = 3000;

  const activeLevels = $derived(LEVELS.filter((l) => levels[l]).flatMap((l) => (l === "error" ? ["error", "fatal"] : [l])));
  const names = $derived(Object.fromEntries(store.tunnels.map((t) => [t.id, t.name])));

  async function load() {
    loading = true;
    error = null;
    try {
      lines = await api.queryLogs({ tunnelId, levels: activeLevels.length === 5 ? [] : activeLevels, search, limit: 1000 });
      await scrollToEnd();
    } catch (e) {
      error = errorText(e);
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    void tunnelId;
    void search;
    void activeLevels.join();
    const t = setTimeout(load, search ? 180 : 0);
    return () => clearTimeout(t);
  });

  function matches(l: LogLine) {
    if (tunnelId && l.tunnel_id !== tunnelId) return false;
    if (!activeLevels.includes(l.level)) return false;
    if (search && !(l.message + l.fields).toLowerCase().includes(search.toLowerCase())) return false;
    return true;
  }

  async function scrollToEnd() {
    if (!follow) return;
    await tick();
    scroller?.scrollTo({ top: scroller.scrollHeight });
  }

  function onScroll() {
    if (!scroller) return;
    follow = scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 40;
  }

  onMount(() => {
    const un = listen<LogLine[]>("logs", (e) => {
      const add = e.payload.filter(matches);
      if (!add.length) return;
      lines = [...lines, ...add].slice(-MAX);
      scrollToEnd();
    });
    return () => un.then((f) => f());
  });

  function fieldsOf(l: LogLine): [string, string][] {
    try {
      const o = JSON.parse(l.fields) as Record<string, unknown>;
      return Object.entries(o).map(([k, v]) => [k, typeof v === "string" ? v : JSON.stringify(v)]);
    } catch {
      return [];
    }
  }

  async function clear() {
    await api.clearLogs(tunnelId);
    lines = [];
  }
</script>

<div class="page">
  <Toolbar title="Logs">
    <TunnelPicker bind:value={tunnelId} allowAll />
    <div class="chips" role="group" aria-label="Levels">
      {#each LEVELS as l}
        <button class="chip {l}" class:on={levels[l]} aria-pressed={levels[l]} onclick={() => (levels[l] = !levels[l])}>{l}</button>
      {/each}
    </div>
    <div class="search">
      <Icon name="search" size={13} />
      <input class="field" placeholder="Search" bind:value={search} aria-label="Search logs" />
    </div>
    <button class="btn" onclick={clear} disabled={!lines.length}>Clear</button>
  </Toolbar>

  <div class="logwrap">
    {#if loading && lines.length === 0}
      <StatePanel kind="loading" title="Loading logs…" />
    {:else if error}
      <StatePanel kind="error" title="Couldn't load logs" message={error}><button class="btn" onclick={load}>Try again</button></StatePanel>
    {:else if lines.length === 0}
      <StatePanel kind="empty" title={search || activeLevels.length < 5 ? "No matching log lines" : "No logs yet"} message={search ? "Try a different search or level filter." : "Start a tunnel and cloudflared's output streams here live."} />
    {:else}
      <div class="log" bind:this={scroller} onscroll={onScroll} role="log" aria-live="polite">
        {#each lines as l (l.id)}
          <button class="line {l.level}" class:open={expanded === l.id} onclick={() => (expanded = expanded === l.id ? null : l.id)}>
            <span class="ts">{formatClock(l.ts)}</span>
            <span class="lvl">{l.level === "fatal" ? "FTL" : l.level.slice(0, 3).toUpperCase()}</span>
            {#if !tunnelId}<span class="tn">{names[l.tunnel_id] ?? "removed"}</span>{/if}
            <span class="msg">{l.message}</span>
          </button>
          {#if expanded === l.id && l.fields !== "{}"}
            <div class="fields selectable">
              {#each fieldsOf(l) as [k, v]}<div><span class="k">{k}</span> {v}</div>{/each}
            </div>
          {/if}
        {/each}
      </div>
      {#if !follow}
        <button class="jump btn" onclick={() => { follow = true; scrollToEnd(); }}>Jump to latest</button>
      {/if}
    {/if}
  </div>
</div>

<style>
  .page {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  .chips {
    display: flex;
    gap: 3px;
  }
  .chip {
    height: 22px;
    padding: 0 8px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--text-3);
    font-size: var(--text-xs);
    box-shadow: inset 0 0 0 0.5px var(--line-strong);
  }
  .chip.on {
    color: var(--text-1);
    background: var(--surface);
    box-shadow: var(--shadow);
  }
  .chip.on.warn { color: var(--degraded); }
  .chip.on.error { color: var(--bad); }
  .search {
    display: flex;
    align-items: center;
    gap: 4px;
    color: var(--text-3);
  }
  .search .field {
    width: 160px;
  }
  .logwrap {
    position: relative;
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .log {
    flex: 1;
    overflow: auto;
    padding: 6px 0 12px;
    font-family: var(--mono);
    font-size: 11.5px;
  }
  .line {
    display: flex;
    gap: 10px;
    width: 100%;
    padding: 1px 16px;
    border: 0;
    background: transparent;
    text-align: left;
    font: inherit;
    line-height: 1.55;
  }
  .line:hover {
    background: var(--hover);
  }
  .ts {
    color: var(--text-3);
    flex: none;
    font-variant-numeric: tabular-nums;
  }
  .lvl {
    flex: none;
    width: 28px;
    color: var(--text-3);
  }
  .info .lvl { color: var(--text-2); }
  .warn .lvl, .warn .msg { color: var(--degraded); }
  .error .lvl, .error .msg, .fatal .lvl, .fatal .msg { color: var(--bad); }
  .tn {
    flex: none;
    max-width: 110px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-2);
  }
  .msg {
    white-space: pre-wrap;
    word-break: break-word;
  }
  .fields {
    margin: 2px 16px 6px 112px;
    padding: 6px 10px;
    border-radius: 6px;
    background: var(--code-bg);
    line-height: 1.5;
    word-break: break-all;
  }
  .k {
    color: var(--text-2);
  }
  .jump {
    position: absolute;
    right: 18px;
    bottom: 14px;
  }
</style>
