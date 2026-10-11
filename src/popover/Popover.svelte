<script lang="ts">
  import { onMount } from "svelte";
  import { flip } from "svelte/animate";
  import { fly, fade } from "svelte/transition";
  import { writeText } from "@tauri-apps/plugin-clipboard-manager";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { listen } from "@tauri-apps/api/event";
  import { store } from "../lib/store.svelte";
  import { api, errorText } from "../lib/api";
  import { motion } from "../lib/theme";
  import { isOn, publicUrl, statusLabel } from "../lib/format";
  import type { Tunnel } from "../lib/types";
  import StatusDot from "$lib/components/app/StatusDot.svelte";
  import Toggle from "$lib/components/app/Toggle.svelte";
  import RouteLine from "$lib/components/app/RouteLine.svelte";
  import Icon from "$lib/components/app/Icon.svelte";
  import StatePanel from "$lib/components/app/StatePanel.svelte";

  let busy = $state<Record<string, boolean>>({});
  let toast = $state<string | null>(null);
  let toastTimer: ReturnType<typeof setTimeout>;

  onMount(() => {
    store.init();
    // Refresh when shown, in case tunnels were added in the dashboard.
    const un = listen("popover-shown", () => store.refresh());
    return () => {
      un.then((f) => f());
    };
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


  function flash(msg: string) {
    toast = msg;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = null), 1600);
  }

  async function toggle(t: Tunnel, on: boolean) {
    busy[t.id] = true;
    try {
      if (on) await api.start(t.id);
      else await api.stop(t.id);
    } catch (e) {
      flash(errorText(e));
    } finally {
      busy[t.id] = false;
    }
  }

  async function copy(t: Tunnel) {
    const url = publicUrl(t);
    if (!url) return;
    await writeText(url);
    flash("Copied " + url.replace("https://", ""));
  }

  const anyOn = $derived(store.tunnels.some(isOn));
  const summary = $derived.by(() => {
    const n = store.activeCount;
    const starting = store.tunnels.filter((t) => t.state.status === "starting").length;
    if (n === 0 && starting === 0) return "No tunnels running";
    const parts = [];
    if (n) parts.push(`${n} running`);
    if (starting) parts.push(`${starting} starting`);
    return parts.join(", ");
  });
</script>

<main class="popover">
  <header>
    <div class="title">
      <strong>Burrow</strong>
      <span class="summary">{summary}</span>
    </div>
    {#if store.tunnels.length}
      {#if anyOn}
        <button class="btn ghost small" onclick={() => api.stopAll()}>Stop all</button>
      {:else}
        <button class="btn ghost small" onclick={() => api.startAll()}>Start all</button>
      {/if}
    {/if}
  </header>

  {#if store.overview && !store.overview.cloudflared}
    <div class="banner" role="alert">
      <Icon name="warn" size={13} />
      <span>cloudflared isn't installed. Run <code>brew install cloudflared</code>, then open Settings.</span>
    </div>
  {/if}

  <section class="list">
    {#if store.loading}
      <StatePanel kind="loading" title="Loading tunnels…" />
    {:else if store.error}
      <StatePanel kind="error" title="Couldn't load tunnels" message={store.error}>
        <button class="btn" onclick={() => store.refresh()}>Try again</button>
      </StatePanel>
    {:else if store.tunnels.length === 0}
      <StatePanel kind="empty" title="No tunnels yet" message="Import a config.yml, paste a tunnel token, or share a local port with a quick tunnel.">
        <button class="btn primary" onclick={() => api.openDashboard("add")}>Add a tunnel</button>
      </StatePanel>
    {:else}
      {#each groups as g (g.id)}
        <div class="group" animate:flip={{ duration: motion(220) }}>
          <div class="group-name">{g.name}</div>
          {#each g.tunnels as t (t.id)}
            {@const m = t.state.mappings.find((m) => m.hostname) ?? t.state.mappings[0]}
            {@const url = publicUrl(t)}
            <div
              class="row"
              animate:flip={{ duration: motion(220) }}
              in:fly={{ y: -6, duration: motion(200) }}
              out:fade={{ duration: motion(120) }}
            >
              <StatusDot status={t.state.status} />
              <button class="main" onclick={() => api.openDashboard(`tunnels:${t.id}`)} title="Show in dashboard">
                <span class="name">{t.name}</span>
                {#if t.state.status === "crashed" || t.state.status === "degraded"}
                  <span class="detail" class:bad={t.state.status === "crashed"}>{t.state.detail ?? statusLabel[t.state.status]}</span>
                {:else if m}
                  <RouteLine hostname={t.kind === "quick" ? (t.state.public_url?.replace("https://", "") ?? null) : m.hostname} service={m.service} inspected={m.inspected} pulse={store.pulses[t.id] ?? 0} compact />
                {:else if t.kind === "token"}
                  <span class="detail">Routes come from the Cloudflare dashboard</span>
                {:else if t.kind === "quick"}
                  <span class="detail mono">{t.origin_url}</span>
                {/if}
              </button>
              <div class="actions">
                {#if url && t.state.status === "connected"}
                  <button class="icon-btn" title="Copy public URL" aria-label="Copy public URL" onclick={() => copy(t)}><Icon name="copy" /></button>
                  <button class="icon-btn" title="Open in browser" aria-label="Open in browser" onclick={() => openUrl(url)}><Icon name="open" /></button>
                {/if}
              </div>
              <Toggle checked={isOn(t)} busy={busy[t.id]} label={isOn(t) ? `Stop ${t.name}` : `Start ${t.name}`} onchange={(v) => toggle(t, v)} />
            </div>
          {/each}
        </div>
      {/each}
    {/if}
  </section>

  <footer>
    <button class="btn ghost" onclick={() => api.openDashboard()}><Icon name="dashboard" size={14} /> Open Dashboard</button>
    <button class="btn ghost" onclick={() => api.quit()} title="Quit Burrow and stop all tunnels">Quit</button>
  </footer>

  {#if toast}
    <div class="toast" transition:fly={{ y: 8, duration: motion(180) }}>{toast}</div>
  {/if}
</main>

<style>
  :global(.popover-body) {
    background: transparent;
  }
  .popover {
    display: flex;
    flex-direction: column;
    height: 100vh;
    background: var(--popover-tint);
    border-radius: 12px;
    overflow: hidden;
    position: relative;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 12px 8px 14px;
  }
  .title {
    display: flex;
    flex-direction: column;
  }
  .title strong {
    font-size: var(--text-sm);
    font-weight: 600;
  }
  .summary {
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .small {
    height: 22px;
    font-size: var(--text-callout);
  }
  .banner {
    display: flex;
    gap: 6px;
    align-items: flex-start;
    margin: 0 10px 6px;
    padding: 8px 10px;
    border-radius: var(--radius);
    background: color-mix(in srgb, var(--degraded) 14%, transparent);
    font-size: var(--text-callout);
    line-height: 1.35;
  }
  .banner code {
    font-family: var(--font-mono);
    font-size: 11px;
  }
  .list {
    flex: 1;
    overflow-y: auto;
    padding: 0 6px 6px;
  }
  .group + .group {
    margin-top: 6px;
  }
  .group-name {
    padding: 6px 8px 3px;
    font-size: var(--text-xs);
    font-weight: 600;
    color: var(--text-2);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 6px 8px;
    border-radius: var(--radius);
    transition: background-color 120ms ease;
  }
  .row:hover {
    background: var(--hover);
  }
  .main {
    display: flex;
    flex-direction: column;
    gap: 1px;
    flex: 1;
    min-width: 0;
    padding: 0;
    border: 0;
    background: transparent;
    text-align: left;
  }
  .name {
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .detail {
    font-size: 11.5px;
    color: var(--text-2);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .detail.bad {
    color: var(--bad);
  }
  .actions {
    display: flex;
    opacity: 0;
    transition: opacity 120ms ease;
  }
  .row:hover .actions,
  .row:focus-within .actions {
    opacity: 1;
  }
  footer {
    display: flex;
    justify-content: space-between;
    padding: 6px 8px 8px;
    border-top: 0.5px solid var(--line);
  }
  .toast {
    position: absolute;
    left: 50%;
    bottom: 48px;
    transform: translateX(-50%);
    max-width: 90%;
    padding: 6px 12px;
    border-radius: 999px;
    background: var(--text-1);
    color: var(--bg);
    font-size: var(--text-callout);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
