<script lang="ts" module>
  export type AddTab = "import" | "quick" | "token";
</script>

<script lang="ts">
  // The Add tunnel sheet's contents. AddSheet renders this inside Dialog.Content, which Bits
  // unmounts while the dialog is closed, so every open starts fresh: first tab, a new scan of
  // ~/.cloudflared, no picks, no errors, and no token draft left in memory after Cancel (02 2c).
  import { onMount } from "svelte";
  import { fade } from "svelte/transition";
  import { open as openPicker } from "@tauri-apps/plugin-dialog";
  import FileText from "@lucide/svelte/icons/file-text";
  import Folder from "@lucide/svelte/icons/folder";
  import KeyRound from "@lucide/svelte/icons/key-round";
  import Zap from "@lucide/svelte/icons/zap";
  import { api, errorText } from "../lib/api";
  import { motion } from "../lib/theme";
  import type { Candidate, ImportReport } from "../lib/types";
  import StatePanel from "$lib/components/app/StatePanel.svelte";
  import * as Dialog from "$lib/components/ui/dialog/index.js";
  import * as Tabs from "$lib/components/ui/tabs/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Checkbox } from "$lib/components/ui/checkbox/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import { Label } from "$lib/components/ui/label/index.js";
  import { Switch } from "$lib/components/ui/switch/index.js";
  import { Textarea } from "$lib/components/ui/textarea/index.js";

  let {
    initialTab = "import",
    onclose,
    onadded,
  }: { initialTab?: AddTab; onclose: () => void; onadded: (id: string | null) => void } = $props();

  // Only the starting tab: the body remounts on every open.
  // svelte-ignore state_referenced_locally
  let tab = $state<AddTab>(initialTab);
  let candidates = $state<Candidate[] | null>(null);
  let scanError = $state<string | null>(null);
  let picked = $state<Record<string, boolean>>({});
  let report = $state<ImportReport | null>(null);
  let error = $state<string | null>(null);
  let busy = $state(false);

  let quick = $state({ origin: "", name: "", project: "Quick tunnels", inspect: false });
  let token = $state({ token: "", name: "", project: "Remote" });

  // Addresses, names and tokens aren't prose (same as TunnelDetail): no autocorrect or
  // capitalisation, and no spelling bubble, whose dismissal would eat the first Escape.
  const plainText = { autocorrect: "off", autocapitalize: "off", spellcheck: false, writingsuggestions: false } as const;
  const uid = $props.id();
  const id = (name: string) => `${uid}-${name}`;

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
    const res = await openPicker({ multiple: true, directory, filters: directory ? undefined : [{ name: "cloudflared config", extensions: ["yml", "yaml"] }] });
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
  const hosts = (c: Candidate) => (c.hostnames.slice(0, 3).join(", ") || "no hostnames") + (c.hostnames.length > 3 ? ` +${c.hostnames.length - 3}` : "");
</script>

<!-- Errors stay inline, next to the form (02 §1.4), above the footer of the tab that made them. -->
{#snippet failure()}
  {#if error}<p class="text-callout whitespace-pre-wrap text-bad-text select-text" role="alert">{error}</p>{/if}
{/snippet}

<Dialog.Header>
  <Dialog.Title>Add tunnel</Dialog.Title>
</Dialog.Header>

<!-- min-w-0: the sheet is a grid, and the untruncated hostnames would otherwise widen its column. -->
<Tabs.Root class="min-w-0" bind:value={() => tab, (v) => (tab = v as AddTab)}>
  <Tabs.List class="self-center">
    <Tabs.Trigger value="import"><FileText />Import config</Tabs.Trigger>
    <Tabs.Trigger value="quick"><Zap />Quick tunnel</Tabs.Trigger>
    <Tabs.Trigger value="token"><KeyRound />Token</Tabs.Trigger>
  </Tabs.List>

  <!-- tabindex -1: every panel starts with controls, so Tab goes from the tabs straight to them. -->
  <Tabs.Content value="import" tabindex={-1}>
    <!-- A form for the submit button; it has no text field, so Return doesn't import (as before). -->
    <form class="flex flex-col gap-3" onsubmit={(e) => { e.preventDefault(); importPaths(selectedPaths); }}>
      <p class="text-callout text-muted-foreground">Burrow reads existing cloudflared config files and never modifies them.</p>
      <div class="rounded-lg bg-card shadow-(--shadow)">
        <div class="border-b-[0.5px] border-border px-3 py-2 text-xs text-muted-foreground">In <span class="font-mono">~/.cloudflared</span></div>
        {#if candidates === null && !scanError}
          <StatePanel kind="loading" title="Looking for config files…" />
        {:else if scanError}
          <StatePanel kind="error" title="Couldn't read ~/.cloudflared" message={scanError} />
        {:else if candidates && candidates.length === 0}
          <StatePanel kind="empty" title="No config files found here" message="Choose files or a project folder below instead." />
        {:else if candidates}
          <ul class="max-h-60 overflow-auto">
            {#each candidates as c, i (c.path)}
              <li class="not-first:border-t-[0.5px] not-first:border-border">
                <!-- The whole row toggles the checkbox it contains. -->
                <Label for={id(`cand-${i}`)} class="gap-2.5 px-3 py-2 {c.imported ? 'text-muted-foreground' : ''}">
                  <Checkbox id={id(`cand-${i}`)} disabled={c.imported} bind:checked={() => picked[c.path] ?? false, (v) => (picked[c.path] = v)} />
                  <span class="flex min-w-0 flex-1 flex-col">
                    <span class="font-medium">{c.name} <span class="font-normal text-muted-foreground">{c.path.split("/").pop()}</span></span>
                    <span class="truncate font-mono text-code text-muted-foreground" title={c.hostnames.join(", ")}>{hosts(c)}</span>
                  </span>
                  <span class="text-xs text-muted-foreground">{c.imported ? "Imported" : c.project}</span>
                </Label>
              </li>
            {/each}
          </ul>
        {/if}
      </div>
      {#if report}
        <div class="text-callout leading-normal select-text" role="status" transition:fade={{ duration: motion(150) }}>
          {#if report.added.length}<p>Imported {report.added.length} tunnel{report.added.length === 1 ? "" : "s"}.</p>{/if}
          {#if report.existing.length}<p class="text-muted-foreground">{report.existing.length} already imported.</p>{/if}
          {#each report.failed as [path, why] (path)}<p class="text-bad-text"><span class="font-mono">{path.split("/").pop()}</span>: {why}</p>{/each}
          {#if report.added.length === 0 && report.failed.length === 0 && report.existing.length === 0}<p class="text-muted-foreground">No cloudflared configs found there.</p>{/if}
        </div>
      {/if}
      {@render failure()}
      <Dialog.Footer>
        <Button variant="outline" disabled={busy} onclick={() => chooseFiles(false)}><FileText class="size-3.5" /> Choose files…</Button>
        <Button variant="outline" disabled={busy} onclick={() => chooseFiles(true)}><Folder class="size-3.5" /> Choose folder…</Button>
        <span class="flex-1"></span>
        <Button variant="outline" onclick={onclose}>Cancel</Button>
        <Button type="submit" disabled={busy || selectedPaths.length === 0}>Import{selectedPaths.length ? ` ${selectedPaths.length}` : ""}</Button>
      </Dialog.Footer>
    </form>
  </Tabs.Content>

  <Tabs.Content value="quick" tabindex={-1}>
    <form class="flex flex-col gap-3" onsubmit={(e) => { e.preventDefault(); addQuick(); }}>
      <p class="text-callout text-muted-foreground">Share a local server at a random <span class="font-mono">trycloudflare.com</span> address. No Cloudflare account needed.</p>
      <!-- 120px: the right-aligned label column of an AppKit form ("Local address:" fits). -->
      <div class="grid grid-cols-[120px_1fr] items-center gap-x-3 gap-y-2.5">
        <Label for={id("q-origin")} class="justify-self-end">Local address:</Label>
        <Input id={id("q-origin")} {...plainText} class="font-mono" placeholder="localhost:3000" bind:value={quick.origin} required />
        <Label for={id("q-name")} class="justify-self-end">Name:</Label>
        <Input id={id("q-name")} {...plainText} placeholder="Optional" bind:value={quick.name} />
        <Label for={id("q-project")} class="justify-self-end">Project:</Label>
        <Input id={id("q-project")} {...plainText} bind:value={quick.project} />
        <span></span>
        <div class="flex items-start gap-2 pt-1">
          <!-- Bits' Switch is type="button", so it never submits this form (decisions §15, 02 F15). -->
          <Switch id={id("q-inspect")} aria-describedby={id("q-inspect-help")} disabled={busy} bind:checked={quick.inspect} />
          <div class="flex flex-col">
            <Label for={id("q-inspect")}>Inspect requests</Label>
            <span id={id("q-inspect-help")} class="text-xs text-muted-foreground">Capture request and response bodies through Burrow's local proxy.</span>
          </div>
        </div>
      </div>
      {@render failure()}
      <Dialog.Footer>
        <Button variant="outline" onclick={onclose}>Cancel</Button>
        <Button type="submit" disabled={busy || !quick.origin.trim()}>Create and start</Button>
      </Dialog.Footer>
    </form>
  </Tabs.Content>

  <Tabs.Content value="token" tabindex={-1}>
    <form class="flex flex-col gap-3" onsubmit={(e) => { e.preventDefault(); addToken(); }}>
      <p class="text-callout text-muted-foreground">For tunnels created in the Cloudflare dashboard. Copy the token from the tunnel's install command; Burrow keeps it in your keychain.</p>
      <!-- 120px: the right-aligned label column of an AppKit form ("Local address:" fits). -->
      <div class="grid grid-cols-[120px_1fr] items-center gap-x-3 gap-y-2.5">
        <Label for={id("t-token")} class="self-start justify-self-end pt-1">Token:</Label>
        <!-- break-all: a token is one long word. -->
        <Textarea id={id("t-token")} {...plainText} class="max-h-28 resize-none font-mono break-all" rows={3} placeholder="eyJhIjoi…" bind:value={token.token} required />
        <Label for={id("t-name")} class="justify-self-end">Name:</Label>
        <Input id={id("t-name")} {...plainText} placeholder="Remote tunnel" bind:value={token.name} />
        <Label for={id("t-project")} class="justify-self-end">Project:</Label>
        <Input id={id("t-project")} {...plainText} bind:value={token.project} />
      </div>
      {@render failure()}
      <Dialog.Footer>
        <Button variant="outline" onclick={onclose}>Cancel</Button>
        <Button type="submit" disabled={busy || !token.token.trim()}>Add tunnel</Button>
      </Dialog.Footer>
    </form>
  </Tabs.Content>
</Tabs.Root>
