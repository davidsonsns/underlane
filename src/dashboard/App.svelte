<script lang="ts">
  import { onMount, tick } from "svelte";
  import { fade } from "svelte/transition";
  import { listen } from "@tauri-apps/api/event";
  import { store } from "../lib/store.svelte";
  import { api } from "../lib/api";
  import { motion } from "../lib/theme";
  import Icon from "$lib/components/app/Icon.svelte";
  import Tunnels from "./Tunnels.svelte";
  import Requests from "./Requests.svelte";
  import Logs from "./Logs.svelte";
  import Settings from "./Settings.svelte";
  import AddSheet from "./AddSheet.svelte";
  import * as Tooltip from "$lib/components/ui/tooltip/index.js";
  import { Toaster } from "$lib/components/ui/sonner/index.js";
  import logo from "../../src-tauri/icons/64x64.png";

  type View = "tunnels" | "requests" | "logs" | "settings";
  let view = $state<View>("tunnels");
  let focusId = $state<string | null>(null);
  let adding = $state(false);
  let requestsRef = $state<{ selectFirst: (side?: "req" | "resp") => Promise<void> } | null>(null);

  function navigate(target: string) {
    const [v, id] = target.replace(/^#/, "").split(":");
    if (v === "add") {
      view = "tunnels";
      adding = true;
      return;
    }
    adding = false;
    if (["tunnels", "requests", "logs", "settings"].includes(v)) view = v as View;
    focusId = id || null;
  }

  onMount(() => {
    store.init();
    if (location.hash) navigate(location.hash);
    const subs = [
      listen<string>("navigate", (e) => navigate(e.payload)),
      // The e2e harness asks for the rendered text of a view.
      listen<{ nonce: string; view: string; select?: string; tab?: string }>("e2e-snapshot", async (e) => {
        navigate(e.payload.view);
        await tick();
        if (e.payload.select === "first" && requestsRef) await requestsRef.selectFirst(e.payload.tab === "req" ? "req" : "resp");
        await new Promise((r) => setTimeout(r, 400));
        await api.e2eReport(e.payload.nonce, document.body.innerText);
      }),
    ];
    return () => subs.forEach((s) => s.then((f) => f()));
  });

  const nav: { id: View; label: string; icon: string }[] = [
    { id: "tunnels", label: "Tunnels", icon: "tunnel" },
    { id: "requests", label: "Requests", icon: "list" },
    { id: "logs", label: "Logs", icon: "logs" },
    { id: "settings", label: "Settings", icon: "gear" },
  ];
</script>

<!-- One tooltip provider and one toaster per window (02 §1.4, §2). -->
<Tooltip.Provider delayDuration={250}>
<div class="shell">
  <aside class="sidebar" data-tauri-drag-region>
    <div class="traffic-space" data-tauri-drag-region></div>
    <div class="brand" data-tauri-drag-region>
      <img src={logo} alt="" width="22" height="22" draggable="false" />
      <span>Burrow</span>
    </div>
    <nav aria-label="Sections">
      {#each nav as n}
        <button class="nav" class:active={view === n.id} aria-current={view === n.id ? "page" : undefined} onclick={() => { view = n.id; focusId = null; }}>
          <Icon name={n.icon} size={15} />
          <span>{n.label}</span>
          {#if n.id === "tunnels" && store.activeCount > 0}
            <span class="count" transition:fade={{ duration: motion(150) }}>{store.activeCount}</span>
          {/if}
        </button>
      {/each}
    </nav>
    <div class="side-foot">
      {#if store.overview?.cloudflared}
        <span class="faint">cloudflared {store.overview.cloudflared.version ?? ""}</span>
      {:else if store.overview}
        <button class="missing" onclick={() => (view = "settings")}><Icon name="warn" size={12} /> cloudflared not found</button>
      {/if}
    </div>
  </aside>

  <main class="content">
    {#key view}
      <div class="view" in:fade={{ duration: motion(140) }}>
        {#if view === "tunnels"}
          <Tunnels {focusId} onadd={() => (adding = true)} />
        {:else if view === "requests"}
          <Requests bind:this={requestsRef} {focusId} />
        {:else if view === "logs"}
          <Logs {focusId} />
        {:else}
          <Settings />
        {/if}
      </div>
    {/key}
  </main>
</div>

{#if adding}
  <AddSheet onclose={() => (adding = false)} onadded={(id) => { adding = false; focusId = id; view = "tunnels"; store.refresh(); }} />
{/if}

<Toaster theme={store.overview?.settings.theme ?? "system"} position="bottom-right" visibleToasts={3} />
</Tooltip.Provider>

<style>
  :global(.dashboard-body) {
    background: transparent;
  }
  .shell {
    display: grid;
    grid-template-columns: 200px 1fr;
    height: 100vh;
  }
  .sidebar {
    display: flex;
    flex-direction: column;
    background: var(--sidebar);
    border-right: 0.5px solid var(--line);
    padding: 0 10px 10px;
  }
  .traffic-space {
    height: 46px;
    flex: none;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 0 6px 10px;
    font-size: var(--text-lg);
    font-weight: 600;
    color: var(--text-1);
    user-select: none;
  }
  .brand img {
    pointer-events: none;
  }
  nav {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .nav {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 28px;
    padding: 0 8px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--text-1);
    text-align: left;
  }
  .nav :global(svg) {
    color: var(--tint);
  }
  .nav:hover {
    background: var(--hover);
  }
  .nav.active {
    background: var(--selected);
  }
  .nav span:first-of-type {
    flex: 1;
  }
  .count {
    min-width: 18px;
    padding: 0 5px;
    border-radius: 9px;
    background: color-mix(in srgb, var(--ok) 18%, transparent);
    color: color-mix(in srgb, var(--ok) 70%, var(--text-1));
    font-size: var(--text-xs);
    font-weight: 600;
    text-align: center;
  }
  .side-foot {
    margin-top: auto;
    padding: 0 8px;
    font-size: var(--text-xs);
  }
  .missing {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--degraded);
    font-size: var(--text-xs);
  }
  .content {
    background: var(--bg);
    min-width: 0;
    overflow: hidden;
  }
  .view {
    height: 100%;
  }
</style>
