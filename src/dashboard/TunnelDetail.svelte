<script lang="ts">
  import { slide } from "svelte/transition";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import { store } from "../lib/store.svelte";
  import { api, errorText } from "../lib/api";
  import { motion } from "../lib/theme";
  import { formatUptime, isOn, statusLabel } from "../lib/format";
  import type { Tunnel, Validation } from "../lib/types";
  import StatusDot from "../components/StatusDot.svelte";
  import Toggle from "../components/Toggle.svelte";
  import Routes from "./Routes.svelte";
  import Icon from "../components/Icon.svelte";

  let { tunnel }: { tunnel: Tunnel } = $props();
  let busy = $state(false);
  let error = $state<string | null>(null);
  let validation = $state<Validation | null>(null);
  let confirmDelete = $state(false);
  let editingName = $state(false);
  let nameDraft = $state("");
  let originDraft = $state("");
  let tokenDraft = $state("");
  let projectDraft = $state("");
  let now = $state(Date.now());

  $effect(() => {
    const i = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(i);
  });

  const s = $derived(tunnel.state);
  const on = $derived(isOn(tunnel));
  const kindText = { config: "Named tunnel from a config file", token: "Remotely managed tunnel (token)", quick: "Quick tunnel on trycloudflare.com" };

  async function run(fn: () => Promise<unknown>) {
    busy = true;
    error = null;
    try {
      await fn();
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = false;
    }
  }

  // Settings edits skip `busy`: disabling every control for each field change made the whole panel flash.
  async function patch(p: Parameters<typeof api.updateTunnel>[1]) {
    error = null;
    try {
      await api.updateTunnel(tunnel.id, p);
      await store.refresh();
    } catch (e) {
      error = errorText(e);
    }
  }

  async function validate() {
    await run(async () => {
      validation = await api.validate(tunnel.id);
    });
  }

  async function remove() {
    await run(async () => {
      await api.deleteTunnel(tunnel.id);
      await store.refresh();
    });
  }
</script>

<header class="head">
  <div class="title-row">
    <StatusDot status={s.status} size={10} />
    {#if editingName}
      <!-- svelte-ignore a11y_autofocus -->
      <input
        class="field name-input"
        bind:value={nameDraft}
        autofocus
        onkeydown={(e) => {
          if (e.key === "Enter") { patch({ name: nameDraft }); editingName = false; }
          if (e.key === "Escape") editingName = false;
        }}
        onblur={() => { if (nameDraft.trim() && nameDraft !== tunnel.name) patch({ name: nameDraft }); editingName = false; }}
      />
    {:else}
      <button class="name" title="Rename" onclick={() => { nameDraft = tunnel.name; editingName = true; }}>{tunnel.name}</button>
    {/if}
  </div>
  <div class="sub">
    <span class="status-text {s.status}">{statusLabel[s.status]}</span>
    {#if s.status === "connected" && s.started_at}<span class="muted">for {formatUptime(s.started_at, now)}</span>{/if}
    {#if s.connections > 0}<span class="muted">{s.connections} edge connection{s.connections === 1 ? "" : "s"}</span>{/if}
    {#if s.restarts > 0}<span class="muted">{s.restarts} restart{s.restarts === 1 ? "" : "s"}</span>{/if}
  </div>
  <div class="actions">
    {#if on}
      <button class="btn" disabled={busy} onclick={() => run(() => api.stop(tunnel.id))}><Icon name="stop" size={10} /> Stop</button>
      <button class="btn" disabled={busy} onclick={() => run(() => api.restart(tunnel.id))}><Icon name="restart" size={13} /> {s.status === "crashed" ? "Try again now" : "Restart"}</button>
    {:else}
      <button class="btn primary" disabled={busy || !store.overview?.cloudflared} onclick={() => run(() => api.start(tunnel.id))}>
        <Icon name="play" size={10} /> {s.status === "crashed" ? "Try again" : "Start"}
      </button>
    {/if}
    {#if tunnel.kind === "config"}
      <button class="btn" disabled={busy} onclick={validate}><Icon name="check" size={13} /> Validate</button>
    {/if}
  </div>
</header>

{#if s.detail || s.hint}
  <div class="notice {s.status}" transition:slide={{ duration: motion(180) }} role="status">
    {#if s.detail}<div class="selectable">{s.detail}</div>{/if}
    {#if s.hint}<div class="hint selectable">{s.hint}</div>{/if}
  </div>
{/if}
{#if error}
  <div class="notice crashed" transition:slide={{ duration: motion(180) }} role="alert"><div class="selectable">{error}</div></div>
{/if}
{#if validation}
  <div class="notice {validation.ok ? 'connected' : 'crashed'}" transition:slide={{ duration: motion(180) }}>
    {#if validation.ok}
      <div>The config is valid.</div>
    {:else}
      <div class="selectable">{validation.raw}</div>
      {#if validation.hint}<div class="hint">{validation.hint}</div>{/if}
    {/if}
  </div>
{/if}

<Routes {tunnel} />

<section>
  <h2>Behaviour</h2>
  <div class="rows">
    <div class="row">
      <span class="label">
        Inspect requests
        <span class="help">
          {#if tunnel.kind === "token"}Not available for token tunnels: their routes live in the Cloudflare dashboard.{:else}Routes traffic through Burrow's local proxy and stores request and response bodies (up to {store.overview?.settings.max_body_kb ?? 1024} KB each). Bodies may contain secrets.{/if}
        </span>
      </span>
      <Toggle checked={tunnel.inspect} disabled={tunnel.kind === "token" || busy} label="Inspect requests" onchange={(v) => patch({ inspect: v })} />
    </div>
    <div class="row">
      <span class="label">Start automatically<span class="help">Start this tunnel whenever Burrow launches.</span></span>
      <Toggle checked={tunnel.autostart} disabled={busy} label="Start automatically" onchange={(v) => patch({ autostart: v })} />
    </div>
    <div class="row">
      <span class="label">Log level<span class="help">Debug logs every request's headers (redacted before storage).</span></span>
      <select class="field" value={tunnel.loglevel} onchange={(e) => patch({ loglevel: (e.target as HTMLSelectElement).value })}>
        {#each ["debug", "info", "warn", "error"] as l}<option value={l}>{l}</option>{/each}
      </select>
    </div>
    <div class="row">
      <span class="label">Project</span>
      <input
        class="field"
        list="projects"
        placeholder={tunnel.project_name}
        bind:value={projectDraft}
        onkeydown={(e) => { if (e.key === "Enter" && projectDraft.trim()) { patch({ project: projectDraft }); projectDraft = ""; } }}
      />
      <datalist id="projects">{#each store.overview?.projects ?? [] as p}<option value={p.name}></option>{/each}</datalist>
    </div>
  </div>
</section>

<section>
  <h2>Source</h2>
  <div class="rows">
    <div class="row"><span class="label">Type</span><span class="muted">{kindText[tunnel.kind]}</span></div>
    {#if tunnel.kind === "config" && tunnel.config_path}
      <div class="row">
        <span class="label">Config file</span>
        <span class="mono path-text selectable" title={tunnel.config_path}>{tunnel.config_path}</span>
        <button class="icon-btn" title="Show in Finder" aria-label="Show in Finder" onclick={() => revealItemInDir(tunnel.config_path!)}><Icon name="folder" /></button>
      </div>
    {/if}
    {#if tunnel.kind === "quick"}
      <div class="row">
        <span class="label">Local address</span>
        <input class="field mono" placeholder={tunnel.origin_url ?? ""} bind:value={originDraft}
          onkeydown={(e) => { if (e.key === "Enter" && originDraft.trim()) { patch({ originUrl: originDraft }); originDraft = ""; } }} />
      </div>
    {/if}
    {#if tunnel.kind === "token"}
      <div class="row">
        <span class="label">Token</span>
        <span class="muted">{tunnel.has_token ? "Stored in the keychain" : "Missing"}</span>
        <input class="field mono" type="password" placeholder="Paste a new token" bind:value={tokenDraft}
          onkeydown={(e) => { if (e.key === "Enter" && tokenDraft.trim()) { patch({ token: tokenDraft }); tokenDraft = ""; } }} />
      </div>
    {/if}
  </div>
</section>

{#if tunnel.kind === "config"}
  <p class="muted small">Burrow reads this file and never changes it. Edits are picked up the next time the tunnel starts.</p>
{:else if tunnel.kind === "quick"}
  <p class="muted small">Quick tunnels get a new random hostname every time they start, and don't support server-sent events.</p>
{/if}

<section class="links">
  {#if tunnel.inspect}<button class="btn ghost" onclick={() => api.openDashboard(`requests:${tunnel.id}`)}><Icon name="list" size={13} /> View requests</button>{/if}
  <button class="btn ghost" onclick={() => api.openDashboard(`logs:${tunnel.id}`)}><Icon name="logs" size={13} /> View logs</button>
  <span class="spacer"></span>
  {#if confirmDelete}
    <span class="muted small">Remove “{tunnel.name}” from Burrow? {tunnel.kind === "config" ? "The config file stays on disk." : ""}</span>
    <button class="btn" onclick={() => (confirmDelete = false)}>Cancel</button>
    <button class="btn danger" onclick={remove}>Remove</button>
  {:else}
    <button class="btn ghost danger" onclick={() => (confirmDelete = true)}><Icon name="trash" size={13} /> Remove</button>
  {/if}
</section>

<style>
  .head {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-bottom: 14px;
  }
  .title-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .name {
    padding: 0;
    border: 0;
    background: none;
    font-size: var(--text-xl);
    font-weight: 600;
    letter-spacing: -0.01em;
    text-align: left;
    cursor: text;
  }
  .name-input {
    font-size: var(--text-lg);
    height: 30px;
    width: 280px;
  }
  .sub {
    display: flex;
    gap: 12px;
    font-size: var(--text-sm);
    padding-left: 20px;
  }
  .status-text.connected { color: var(--ok); }
  .status-text.starting { color: var(--starting); }
  .status-text.degraded { color: var(--degraded); }
  .status-text.crashed { color: var(--bad); }
  .status-text.stopped { color: var(--text-2); }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 8px;
  }
  .notice {
    margin: 0 0 14px;
    padding: 10px 12px;
    border-radius: var(--radius);
    background: var(--surface);
    box-shadow: var(--shadow);
    border-left: 3px solid var(--off);
    font-size: var(--text-sm);
    line-height: 1.45;
    white-space: pre-wrap;
  }
  .notice.crashed { border-left-color: var(--bad); }
  .notice.degraded { border-left-color: var(--degraded); }
  .notice.starting { border-left-color: var(--starting); }
  .notice.connected { border-left-color: var(--ok); }
  .hint {
    margin-top: 6px;
    color: var(--text-2);
  }
  section {
    margin-top: 22px;
  }
  h2 {
    margin: 0 0 8px;
    font-size: var(--text);
    font-weight: 600;
  }
  .rows {
    background: var(--surface);
    border-radius: var(--radius);
    box-shadow: var(--shadow);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 9px 12px;
    min-height: 40px;
  }
  .row + .row {
    border-top: 0.5px solid var(--line);
  }
  .label {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
  }
  .help {
    font-size: var(--text-xs);
    color: var(--text-2);
    margin-top: 1px;
    line-height: 1.35;
  }
  .path-text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 55%;
    color: var(--text-2);
  }
  .small {
    font-size: var(--text-xs);
    margin: 6px 2px 0;
  }
  .links {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .spacer {
    flex: 1;
  }
  select.field {
    width: 110px;
  }
  input.field {
    width: 220px;
  }
</style>
