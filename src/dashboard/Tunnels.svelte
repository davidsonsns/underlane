<script lang="ts">
  import { flip } from "svelte/animate";
  import { fly, fade } from "svelte/transition";
  import { store } from "../lib/store.svelte";
  import { api } from "../lib/api";
  import { motion } from "../lib/theme";
  import { statusLabel } from "../lib/format";
  import type { Tunnel } from "../lib/types";
  import Toolbar from "./Toolbar.svelte";
  import StatusDot from "../components/StatusDot.svelte";
  import Icon from "../components/Icon.svelte";
  import StatePanel from "../components/StatePanel.svelte";
  import TunnelDetail from "./TunnelDetail.svelte";

  let { focusId = null, onadd }: { focusId?: string | null; onadd: () => void } = $props();
  let selected = $state<string | null>(null);
  $effect(() => {
    if (focusId) selected = focusId;
  });
  $effect(() => {
    if (!selected && store.tunnels.length) selected = store.tunnels[0].id;
    if (selected && store.overview && !store.tunnels.some((t) => t.id === selected)) selected = store.tunnels[0]?.id ?? null;
  });

  const groups = $derived.by(() => {
    const map = new Map<string, { name: string; tunnels: Tunnel[] }>();
    for (const t of store.tunnels) {
      const g = map.get(t.project_id) ?? { name: t.project_name, tunnels: [] };
      g.tunnels.push(t);
      map.set(t.project_id, g);
    }
    return [...map.entries()].map(([id, g]) => ({ id, ...g }));
  });
  const current = $derived(store.tunnels.find((t) => t.id === selected) ?? null);
  const anyOn = $derived(store.tunnels.some((t) => ["starting", "connected", "degraded"].includes(t.state.status)));
</script>

<div class="page">
  <Toolbar title="Tunnels" subtitle={store.tunnels.length ? `${store.activeCount} of ${store.tunnels.length} running` : ""}>
    {#if store.tunnels.length}
      {#if anyOn}
        <button class="btn" onclick={() => api.stopAll()}><Icon name="stop" size={11} /> Stop all</button>
      {:else}
        <button class="btn" onclick={() => api.startAll()}><Icon name="play" size={11} /> Start all</button>
      {/if}
    {/if}
    <button class="btn primary" onclick={onadd}><Icon name="plus" size={12} /> Add tunnel</button>
  </Toolbar>

  {#if store.loading}
    <StatePanel kind="loading" title="Loading tunnels…" />
  {:else if store.error}
    <StatePanel kind="error" title="Couldn't load tunnels" message={store.error}>
      <button class="btn" onclick={() => store.refresh()}>Try again</button>
    </StatePanel>
  {:else if store.tunnels.length === 0}
    <div class="empty">
      <StatePanel kind="empty" title="Set up your first tunnel" message="Burrow runs cloudflared for you. Import the config files you already have, paste a token from the Cloudflare dashboard, or share a local port with a quick tunnel; no account needed for that one.">
        <button class="btn primary" onclick={onadd}>Add tunnel</button>
      </StatePanel>
    </div>
  {:else}
    <div class="split">
      <div class="list" role="listbox" aria-label="Tunnels">
        {#each groups as g (g.id)}
          <div class="group" animate:flip={{ duration: motion(200) }}>
            <div class="group-name">{g.name}</div>
            {#each g.tunnels as t (t.id)}
              <button
                class="item"
                class:sel={t.id === selected}
                role="option"
                aria-selected={t.id === selected}
                onclick={() => (selected = t.id)}
                animate:flip={{ duration: motion(200) }}
                in:fly={{ x: -8, duration: motion(200) }}
                out:fade={{ duration: motion(120) }}
              >
                <StatusDot status={t.state.status} />
                <span class="item-text">
                  <span class="item-name">{t.name}</span>
                  <span class="item-sub" class:bad={t.state.status === "crashed"}>{statusLabel[t.state.status]}</span>
                </span>
                {#if t.inspect}<span class="tag" title="Requests are inspected"><Icon name="eye" size={12} /></span>{/if}
              </button>
            {/each}
          </div>
        {/each}
      </div>
      <div class="detail">
        {#if current}
          {#key current.id}
            <div class="detail-inner" in:fade={{ duration: motion(120) }}>
              <TunnelDetail tunnel={current} />
            </div>
          {/key}
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
  .empty {
    flex: 1;
    display: grid;
    place-items: center;
  }
  .split {
    flex: 1;
    display: grid;
    grid-template-columns: 250px 1fr;
    min-height: 0;
  }
  .list {
    overflow-y: auto;
    padding: 8px;
    border-right: 0.5px solid var(--line);
  }
  .group + .group {
    margin-top: 8px;
  }
  .group-name {
    padding: 4px 8px;
    font-size: var(--text-xs);
    font-weight: 600;
    color: var(--text-2);
  }
  .item {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 7px 8px;
    border: 0;
    border-radius: 7px;
    background: transparent;
    text-align: left;
  }
  .item:hover {
    background: var(--hover);
  }
  .item.sel {
    background: var(--selected);
  }
  .item-text {
    display: flex;
    flex-direction: column;
    min-width: 0;
    flex: 1;
  }
  .item-name {
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .item-sub {
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .item-sub.bad {
    color: var(--bad);
  }
  .tag {
    color: var(--text-3);
    display: flex;
  }
  .detail {
    overflow-y: auto;
    min-width: 0;
  }
  .detail-inner {
    padding: 20px 24px 32px;
  }
</style>
