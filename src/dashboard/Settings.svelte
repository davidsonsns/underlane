<script lang="ts">
  import { onMount } from "svelte";
  import { slide } from "svelte/transition";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import { writeText } from "@tauri-apps/plugin-clipboard-manager";
  import { store } from "../lib/store.svelte";
  import { api, errorText } from "../lib/api";
  import { motion } from "../lib/theme";
  import type { LoginItem, Settings } from "../lib/types";
  import Toolbar from "./Toolbar.svelte";
  import Toggle from "../components/Toggle.svelte";
  import StatePanel from "../components/StatePanel.svelte";
  import Icon from "../components/Icon.svelte";

  let draft = $state<Settings | null>(null);
  let login = $state<LoginItem | null>(null);
  let saved = $state(false);
  let error = $state<string | null>(null);
  let detecting = $state(false);
  let redactText = $state("");
  let savedTimer: ReturnType<typeof setTimeout>;

  $effect(() => {
    if (!draft && store.overview) {
      draft = structuredClone($state.snapshot(store.overview.settings));
      redactText = draft.redact_headers.join(", ");
    }
  });

  onMount(async () => {
    login = await api.loginItem();
  });

  async function save(patch: Partial<Settings>) {
    if (!draft) return;
    Object.assign(draft, patch);
    error = null;
    try {
      await api.saveSettings($state.snapshot(draft));
      saved = true;
      clearTimeout(savedTimer);
      savedTimer = setTimeout(() => (saved = false), 1200);
    } catch (e) {
      error = errorText(e);
    }
  }

  async function detect() {
    detecting = true;
    try {
      await save({ cloudflared_path: draft?.cloudflared_path?.trim() || null });
      await api.detectCloudflared();
      await store.refresh();
    } finally {
      detecting = false;
    }
  }

  async function setLogin(v: boolean) {
    login = await api.setLaunchAtLogin(v);
  }

  const num = (e: Event) => Number((e.target as HTMLInputElement).value);
</script>

<div class="page">
  <Toolbar title="Settings" subtitle={saved ? "Saved" : ""} />
  {#if !draft}
    <StatePanel kind="loading" title="Loading settings…" />
  {:else}
    <div class="scroll">
      {#if error}<div class="err" transition:slide={{ duration: motion(160) }} role="alert">{error}</div>{/if}

      <h2>cloudflared</h2>
      <div class="group">
        <div class="row">
          <span class="label">Status</span>
          {#if store.overview?.cloudflared}
            <span class="ok"><Icon name="check" size={13} /> Version {store.overview.cloudflared.version ?? "unknown"}</span>
            <span class="mono faint path" title={store.overview.cloudflared.path}>{store.overview.cloudflared.path}</span>
          {:else}
            <span class="bad">Not found</span>
          {/if}
        </div>
        {#if !store.overview?.cloudflared}
          <div class="row install">
            <span class="label">Install it with Homebrew, then choose Detect.</span>
            <code class="mono">brew install cloudflared</code>
            <button class="icon-btn" aria-label="Copy command" title="Copy command" onclick={() => writeText("brew install cloudflared")}><Icon name="copy" /></button>
          </div>
        {/if}
        <div class="row">
          <span class="label">Path<span class="help">Leave empty to look on PATH and in Homebrew's folders.</span></span>
          <input class="field mono wide" placeholder="/opt/homebrew/bin/cloudflared" bind:value={draft.cloudflared_path} />
          <button class="btn" disabled={detecting} onclick={detect}>{detecting ? "Detecting…" : "Detect"}</button>
        </div>
      </div>

      <h2>General</h2>
      <div class="group">
        <div class="row">
          <span class="label">Launch at login<span class="help">{login?.message ?? "Opens Burrow in the menu bar when you log in. Tunnels marked “Start automatically” come up with it."}</span></span>
          <Toggle checked={login?.enabled ?? false} disabled={!login || login.status === "unsupported"} label="Launch at login" onchange={setLogin} />
        </div>
        <div class="row">
          <span class="label">Appearance</span>
          <div class="seg" role="radiogroup" aria-label="Appearance">
            {#each [["system", "System"], ["light", "Light"], ["dark", "Dark"]] as [v, l]}
              <button role="radio" aria-checked={draft.theme === v} class:active={draft.theme === v} onclick={() => save({ theme: v as Settings["theme"] })}>{l}</button>
            {/each}
          </div>
        </div>
        <div class="row">
          <span class="label">Default log level<span class="help">Used for newly added tunnels.</span></span>
          <select class="field" value={draft.default_loglevel} onchange={(e) => save({ default_loglevel: (e.target as HTMLSelectElement).value })}>
            {#each ["debug", "info", "warn", "error"] as l}<option value={l}>{l}</option>{/each}
          </select>
        </div>
      </div>

      <h2>Storage</h2>
      <div class="group">
        <div class="row">
          <span class="label">Keep logs and requests for<span class="help">Older entries are deleted automatically.</span></span>
          <input class="field num" type="number" min="1" max="365" value={draft.log_ttl_days} onchange={(e) => save({ log_ttl_days: num(e) })} />
          <span class="unit">days</span>
        </div>
        <div class="row">
          <span class="label">Maximum database size<span class="help">The oldest entries go first when this is reached.</span></span>
          <input class="field num" type="number" min="16" step="16" value={draft.max_db_mb} onchange={(e) => save({ max_db_mb: num(e) })} />
          <span class="unit">MB</span>
        </div>
        <div class="row">
          <span class="label">Data folder</span>
          <span class="mono faint path" title={store.overview?.data_dir}>{store.overview?.data_dir}</span>
          <button class="icon-btn" aria-label="Show in Finder" title="Show in Finder" onclick={() => store.overview && revealItemInDir(store.overview.data_dir + "/burrow.db")}><Icon name="folder" /></button>
        </div>
      </div>

      <h2>Request inspection</h2>
      <div class="group">
        <div class="row">
          <span class="label">Body capture limit<span class="help">Bodies larger than this are stored truncated. 0 keeps headers only.</span></span>
          <input class="field num" type="number" min="0" max="16384" value={draft.max_body_kb} onchange={(e) => save({ max_body_kb: num(e) })} />
          <span class="unit">KB</span>
        </div>
        <div class="row column">
          <span class="label">Redact these headers too<span class="help">Authorization, Cookie, Set-Cookie, X-Api-Key and other credential headers are always redacted, as are token-like query parameters. Bodies are stored as sent.</span></span>
          <input class="field mono full" placeholder="X-Internal-Key, X-Session" bind:value={redactText}
            onchange={() => save({ redact_headers: redactText.split(",").map((s) => s.trim()).filter(Boolean) })} />
        </div>
        {#if (store.overview?.dropped_captures ?? 0) > 0}
          <div class="row"><span class="label">Dropped under load</span><span class="muted">{store.overview?.dropped_captures} log lines or requests weren't stored because the writer fell behind.</span></div>
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
  .scroll {
    overflow-y: auto;
    padding: 8px 24px 32px;
    max-width: 760px;
  }
  h2 {
    margin: 20px 0 8px 2px;
    font-size: var(--text);
    font-weight: 600;
  }
  .group {
    background: var(--surface);
    border-radius: var(--radius);
    box-shadow: var(--shadow);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    min-height: 42px;
  }
  .row + .row {
    border-top: 0.5px solid var(--line);
  }
  .row.column {
    flex-direction: column;
    align-items: stretch;
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
    line-height: 1.4;
  }
  .ok {
    display: inline-flex;
    gap: 4px;
    align-items: center;
    color: var(--ok);
  }
  .bad {
    color: var(--bad);
  }
  .path {
    max-width: 280px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .install code {
    padding: 2px 8px;
    border-radius: 5px;
    background: var(--code-bg);
  }
  .wide {
    width: 260px;
  }
  .full {
    width: 100%;
  }
  .num {
    width: 80px;
    text-align: right;
  }
  .unit {
    width: 34px;
    color: var(--text-2);
  }
  .seg {
    display: inline-flex;
    padding: 2px;
    border-radius: 7px;
    background: var(--hover);
  }
  .seg button {
    height: 22px;
    padding: 0 12px;
    border: 0;
    border-radius: 5px;
    background: transparent;
    color: var(--text-2);
  }
  .seg button.active {
    background: var(--surface);
    color: var(--text-1);
    box-shadow: var(--shadow);
  }
  .err {
    margin-top: 12px;
    padding: 8px 12px;
    border-radius: var(--radius);
    background: color-mix(in srgb, var(--bad) 12%, transparent);
    color: var(--bad);
  }
</style>
