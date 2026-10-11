<script lang="ts">
  import { onMount } from "svelte";
  import { fade, fly } from "svelte/transition";
  import { open } from "@tauri-apps/plugin-dialog";
  import { api, errorText } from "../lib/api";
  import { motion } from "../lib/theme";
  import type { Candidate, ImportReport } from "../lib/types";
  import Icon from "../components/Icon.svelte";
  import StatePanel from "../components/StatePanel.svelte";
  import Toggle from "../components/Toggle.svelte";

  let { onclose, onadded }: { onclose: () => void; onadded: (id: string | null) => void } = $props();

  type Tab = "import" | "quick" | "token";
  let tab = $state<Tab>("import");
  let candidates = $state<Candidate[] | null>(null);
  let scanError = $state<string | null>(null);
  let picked = $state<Record<string, boolean>>({});
  let report = $state<ImportReport | null>(null);
  let error = $state<string | null>(null);
  let busy = $state(false);

  let quick = $state({ origin: "", name: "", project: "Quick tunnels", inspect: false });
  let token = $state({ token: "", name: "", project: "Remote" });

  onMount(async () => {
    try {
      candidates = await api.scanDefault();
      for (const c of candidates) if (!c.imported) picked[c.path] = true;
    } catch (e) {
      scanError = errorText(e);
    }
  });

  async function importPaths(paths: string[]) {
    if (!paths.length) return;
    busy = true;
    error = null;
    try {
      report = await api.importPaths(paths);
      if (report.added.length && !report.failed.length) onadded(null);
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = false;
    }
  }

  async function chooseFiles(directory: boolean) {
    const res = await open({ multiple: true, directory, filters: directory ? undefined : [{ name: "cloudflared config", extensions: ["yml", "yaml"] }] });
    if (!res) return;
    await importPaths(Array.isArray(res) ? res : [res]);
  }

  async function addQuick() {
    busy = true;
    error = null;
    try {
      const id = await api.addQuick(quick.name, quick.origin, quick.project, quick.inspect);
      await api.start(id);
      onadded(id);
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = false;
    }
  }

  async function addToken() {
    busy = true;
    error = null;
    try {
      const id = await api.addToken(token.name, token.token, token.project);
      onadded(id);
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = false;
    }
  }

  const selectedPaths = $derived(Object.entries(picked).filter(([, v]) => v).map(([k]) => k));
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onclose()} />

<div class="scrim" transition:fade={{ duration: motion(150) }} onclick={onclose} role="presentation"></div>
<div class="sheet" role="dialog" aria-modal="true" aria-label="Add tunnel" transition:fly={{ y: -16, duration: motion(220) }}>
  <div class="tabs" role="tablist">
    <button role="tab" aria-selected={tab === "import"} class:active={tab === "import"} onclick={() => (tab = "import")}><Icon name="file" size={13} /> Import config</button>
    <button role="tab" aria-selected={tab === "quick"} class:active={tab === "quick"} onclick={() => (tab = "quick")}><Icon name="bolt" size={12} /> Quick tunnel</button>
    <button role="tab" aria-selected={tab === "token"} class:active={tab === "token"} onclick={() => (tab = "token")}><Icon name="key" size={13} /> Token</button>
  </div>

  <div class="body">
    {#if tab === "import"}
      <p class="lead">Burrow reads existing cloudflared config files and never modifies them.</p>
      <div class="box">
        <div class="box-head">
          <span>In <span class="mono">~/.cloudflared</span></span>
        </div>
        {#if candidates === null && !scanError}
          <StatePanel kind="loading" title="Looking for config files…" />
        {:else if scanError}
          <StatePanel kind="error" title="Couldn't read ~/.cloudflared" message={scanError} />
        {:else if candidates && candidates.length === 0}
          <StatePanel kind="empty" title="No config files found here" message="Choose files or a project folder below instead." />
        {:else if candidates}
          <ul class="cands">
            {#each candidates as c (c.path)}
              <li>
                <label class:disabled={c.imported}>
                  <input type="checkbox" disabled={c.imported} bind:checked={picked[c.path]} />
                  <span class="cand-text">
                    <span class="cand-name">{c.name} <span class="faint">{c.path.split("/").pop()}</span></span>
                    <span class="cand-hosts mono">{c.hostnames.slice(0, 3).join(", ") || "no hostnames"}{c.hostnames.length > 3 ? ` +${c.hostnames.length - 3}` : ""}</span>
                  </span>
                  {#if c.imported}<span class="faint small">Imported</span>{:else}<span class="faint small">{c.project}</span>{/if}
                </label>
              </li>
            {/each}
          </ul>
        {/if}
      </div>
      <div class="row-actions">
        <button class="btn" disabled={busy} onclick={() => chooseFiles(false)}><Icon name="file" size={13} /> Choose files…</button>
        <button class="btn" disabled={busy} onclick={() => chooseFiles(true)}><Icon name="folder" size={13} /> Choose folder…</button>
        <span class="spacer"></span>
        <button class="btn primary" disabled={busy || selectedPaths.length === 0} onclick={() => importPaths(selectedPaths)}>
          Import {selectedPaths.length || ""}
        </button>
      </div>
      {#if report}
        <div class="report" transition:fade={{ duration: motion(150) }}>
          {#if report.added.length}<div>Imported {report.added.length} tunnel{report.added.length === 1 ? "" : "s"}.</div>{/if}
          {#if report.existing.length}<div class="muted">{report.existing.length} already imported.</div>{/if}
          {#each report.failed as [path, why]}<div class="fail"><span class="mono">{path.split("/").pop()}</span>: {why}</div>{/each}
          {#if report.added.length === 0 && report.failed.length === 0 && report.existing.length === 0}<div class="muted">No cloudflared configs found there.</div>{/if}
        </div>
      {/if}
    {:else if tab === "quick"}
      <p class="lead">Share a local server at a random <span class="mono">trycloudflare.com</span> address. No Cloudflare account needed.</p>
      <form class="form" onsubmit={(e) => { e.preventDefault(); addQuick(); }}>
        <label>Local address<input class="field mono" placeholder="localhost:3000" bind:value={quick.origin} required /></label>
        <label>Name<input class="field" placeholder="Optional" bind:value={quick.name} /></label>
        <label>Project<input class="field" bind:value={quick.project} /></label>
        <div class="toggle-row">
          <span>Inspect requests<span class="help">Capture request and response bodies through Burrow's local proxy.</span></span>
          <Toggle checked={quick.inspect} label="Inspect requests" onchange={(v) => (quick.inspect = v)} />
        </div>
        <div class="row-actions">
          <span class="spacer"></span>
          <button class="btn" type="button" onclick={onclose}>Cancel</button>
          <button class="btn primary" type="submit" disabled={busy || !quick.origin.trim()}>Create and start</button>
        </div>
      </form>
    {:else}
      <p class="lead">For tunnels created in the Cloudflare dashboard. Copy the token from the tunnel's install command; Burrow keeps it in your keychain.</p>
      <form class="form" onsubmit={(e) => { e.preventDefault(); addToken(); }}>
        <label>Token<textarea class="field mono" rows="3" placeholder="eyJhIjoi…" bind:value={token.token} required></textarea></label>
        <label>Name<input class="field" placeholder="Remote tunnel" bind:value={token.name} /></label>
        <label>Project<input class="field" bind:value={token.project} /></label>
        <div class="row-actions">
          <span class="spacer"></span>
          <button class="btn" type="button" onclick={onclose}>Cancel</button>
          <button class="btn primary" type="submit" disabled={busy || !token.token.trim()}>Add tunnel</button>
        </div>
      </form>
    {/if}
    {#if error}<div class="fail" role="alert">{error}</div>{/if}
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.18);
    z-index: 10;
  }
  .sheet {
    position: fixed;
    top: 40px;
    left: 50%;
    transform: translateX(-50%);
    width: min(560px, calc(100vw - 48px));
    max-height: calc(100vh - 80px);
    overflow: auto;
    z-index: 11;
    background: var(--bg);
    border-radius: var(--radius-xl);
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.28), 0 0 0 0.5px var(--line-strong);
  }
  .tabs {
    display: flex;
    gap: 4px;
    padding: 10px 10px 0;
  }
  .tabs button {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    padding: 0 12px;
    border: 0;
    border-radius: 7px;
    background: transparent;
    color: var(--text-2);
  }
  .tabs button.active {
    background: var(--selected);
    color: var(--text-1);
  }
  .body {
    padding: 14px 18px 18px;
  }
  .lead {
    margin: 0 0 12px;
    color: var(--text-2);
    font-size: var(--text-callout);
    line-height: 1.45;
  }
  .box {
    background: var(--surface);
    border-radius: var(--radius);
    box-shadow: var(--shadow);
  }
  .box-head {
    padding: 8px 12px;
    font-size: var(--text-xs);
    color: var(--text-2);
    border-bottom: 0.5px solid var(--line);
  }
  .cands {
    list-style: none;
    margin: 0;
    padding: 0;
    max-height: 240px;
    overflow: auto;
  }
  .cands li + li {
    border-top: 0.5px solid var(--line);
  }
  .cands label {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 12px;
  }
  .cands label.disabled {
    opacity: 0.55;
  }
  .cand-text {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
  }
  .cand-name {
    font-weight: 500;
  }
  .cand-hosts {
    font-size: 11px;
    color: var(--text-2);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .small {
    font-size: var(--text-xs);
  }
  .row-actions {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-top: 12px;
  }
  .spacer {
    flex: 1;
  }
  .report {
    margin-top: 12px;
    font-size: var(--text-callout);
    line-height: 1.5;
  }
  .fail {
    margin-top: 8px;
    color: var(--bad);
    font-size: var(--text-callout);
    -webkit-user-select: text;
    user-select: text;
  }
  .form {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .form label {
    display: grid;
    grid-template-columns: 120px 1fr;
    align-items: center;
    gap: 10px;
    font-size: var(--text-sm);
  }
  .form label:has(textarea) {
    align-items: start;
  }
  .toggle-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding-top: 4px;
  }
  .toggle-row > span {
    display: flex;
    flex-direction: column;
  }
  .help {
    font-size: var(--text-xs);
    color: var(--text-2);
  }
</style>
