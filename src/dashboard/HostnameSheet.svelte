<script lang="ts">
  import { tick, untrack } from "svelte";
  import CircleMinus from "@lucide/svelte/icons/circle-minus";
  import Plus from "@lucide/svelte/icons/plus";
  import X from "@lucide/svelte/icons/x";
  import { api, errorText } from "$lib/api";
  import { hostnameChanged, hostnamePayload } from "$lib/hostname-draft";
  import { returnFocusTo } from "$lib/focus";
  import type { HostnameConfig, HostnameRoute } from "$lib/types";
  import * as Dialog from "$lib/components/ui/dialog/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Checkbox } from "$lib/components/ui/checkbox/index.js";
  import { Label } from "$lib/components/ui/label/index.js";

  let {
    open = $bindable(false),
    target,
    returnFocus,
    onsaved,
  }: {
    open?: boolean;
    target: { id: string; name: string; running: boolean };
    returnFocus: () => HTMLElement | null | undefined;
    onsaved: (id: string, restartError: string | null) => Promise<void>;
  } = $props();

  type DraftRoute = HostnameRoute & { key: number };
  let config = $state<HostnameConfig | null>(null);
  let routes = $state<DraftRoute[]>([]);
  let return404 = $state(true);
  let customFallback = $state("");
  let loading = $state(true);
  let saving = $state(false);
  let error = $state<string | null>(null);
  let removed = $state<{ row: DraftRoute; position: number } | null>(null);
  let table = $state<HTMLDivElement | null>(null);
  let addButton = $state<HTMLButtonElement | null>(null);
  let nextKey = 0;
  let request = 0;
  const uid = $props.id();
  const plainText = { autocorrect: "off", autocapitalize: "off", spellcheck: false, writingsuggestions: false } as const;
  const fieldClass = "h-8 w-full min-w-0 rounded-xs border-b border-transparent bg-transparent px-1.5 text-sm outline-none transition-colors hover:bg-fill focus:bg-fill disabled:opacity-45 placeholder:text-placeholder";
  const payload = $derived(hostnamePayload(routes, return404, customFallback));
  const changed = $derived(config !== null && hostnameChanged(config, payload));
  // Open at the editor's final height instead of flashing a short loading panel.
  const visible = $derived(open && (!loading || config !== null));

  async function load() {
    const current = ++request;
    const id = target.id;
    loading = true;
    error = null;
    try {
      const loaded = await api.loadHostnames(id);
      if (current !== request) return;
      config = loaded;
      routes = loaded.routes.map((row) => ({ ...row, key: nextKey++ }));
      return404 = loaded.fallback_service === "http_status:404";
      customFallback = return404 ? "" : loaded.fallback_service;
      removed = null;
    } catch (e) {
      if (current === request) error = errorText(e);
    } finally {
      if (current === request) loading = false;
    }
  }

  $effect(() => {
    if (open) untrack(() => {
      config = null;
      routes = [];
      removed = null;
      load();
    });
    else request++;
  });

  async function add() {
    const key = nextKey++;
    routes = [...routes, { key, index: null, hostname: "", service: "", path: null }];
    await tick();
    table?.querySelector<HTMLInputElement>(`[data-hostname-key="${key}"]`)?.focus();
  }

  async function remove(key: number) {
    const position = routes.findIndex((row) => row.key === key);
    if (position < 0) return;
    removed = { row: { ...routes[position] }, position };
    routes = routes.filter((row) => row.key !== key);
    await tick();
    const next = routes[Math.min(position, routes.length - 1)];
    if (next) table?.querySelector<HTMLInputElement>(`[data-hostname-key="${next.key}"]`)?.focus();
    else addButton?.focus();
  }

  function undo() {
    if (!removed) return;
    const restored = [...routes];
    restored.splice(removed.position, 0, removed.row);
    routes = restored;
    removed = null;
  }

  async function save() {
    if (!config || saving || loading || !changed) return;
    saving = true;
    error = null;
    try {
      const result = await api.saveHostnames(target.id, config.revision, payload.routes, payload.fallbackService);
      // Once saved, refresh/restart errors must never invite a second write of this draft.
      open = false;
      await onsaved(target.id, result.restart_error);
    } catch (e) {
      error = errorText(e);
    } finally {
      saving = false;
    }
  }
</script>

<Dialog.Root bind:open={() => visible, (value) => { if (!saving) open = value; }}>
  <Dialog.Content
    class="flex w-[min(700px,calc(100vw-48px))] flex-col gap-0 overflow-hidden p-0"
    onCloseAutoFocus={returnFocusTo(returnFocus)}
    onEscapeKeydown={(e) => { if (saving) e.preventDefault(); }}
    onInteractOutside={(e) => { if (saving) e.preventDefault(); }}
  >
    <Dialog.Header class="px-5 pt-5 pb-3">
      <div class="flex items-center justify-between gap-3">
        <Dialog.Title>Manage hostnames</Dialog.Title>
        <Button variant="ghost" size="icon-sm" aria-label="Close" disabled={saving} onclick={() => open = false}><X /></Button>
      </div>
      <Dialog.Description>{target.name} · Updates your config with a backup; YAML comments and formatting may change.{target.running ? " Restarts the tunnel when running." : ""}</Dialog.Description>
    </Dialog.Header>

    <form class="flex min-h-0 flex-col" aria-busy={loading || saving} onsubmit={(e) => { e.preventDefault(); save(); }}>
      {#if config}
        <div bind:this={table} class="max-h-[min(340px,calc(100vh-280px))] overflow-auto px-5">
          <div class="grid grid-cols-[minmax(0,1.3fr)_minmax(0,1fr)_28px] gap-x-3 border-b border-border pb-2 text-callout text-muted-foreground" aria-hidden="true">
            <span class="px-1.5">Hostname</span><span class="px-1.5">Service</span><span></span>
          </div>
          {#each routes as row, i (row.key)}
            <div class="grid grid-cols-[minmax(0,1.3fr)_minmax(0,1fr)_28px] items-center gap-x-3 border-b border-border py-2">
              <div class="min-w-0">
                <input {...plainText} class={fieldClass} data-hostname-key={row.key} aria-label="Hostname {i + 1}" placeholder="app.example.com" bind:value={row.hostname} disabled={saving || loading} required />
                {#if row.path}<span class="block truncate px-1.5 pt-0.5 font-mono text-xs text-muted-foreground" title={row.path}>Path: {row.path}</span>{/if}
              </div>
              <input {...plainText} class="{fieldClass} font-mono text-callout" aria-label="Service for hostname {i + 1}" placeholder="localhost:3000" bind:value={row.service} disabled={saving || loading} required />
              <Button variant="ghost" size="icon" aria-label="Remove {row.hostname || `hostname ${i + 1}`}" disabled={saving || loading} onclick={() => remove(row.key)}><CircleMinus /></Button>
            </div>
          {/each}
          {#if !routes.length}<p class="py-5 text-callout text-muted-foreground">No hostnames. Add one to route traffic to a service.</p>{/if}
        </div>
        <div class="flex items-center justify-between gap-3 px-5 py-3">
          <Button bind:ref={addButton} variant="link" disabled={saving || loading} onclick={add}><Plus />Add hostname</Button>
          {#if removed}
            <span class="flex items-center gap-2 text-xs text-muted-foreground" role="status">Hostname removed <Button variant="link" size="xs" disabled={saving || loading} onclick={undo}>Undo</Button></span>
          {/if}
        </div>
        {#if routes.some((row) => row.index === null)}
          <p class="px-5 pb-3 text-xs text-muted-foreground">New hostnames also need DNS pointing to this tunnel.</p>
        {/if}
      {/if}

      {#if error}
        <div class="mx-5 mb-3 flex items-start justify-between gap-3" role="alert">
          <p class="min-w-0 select-text text-callout whitespace-pre-wrap text-bad-text">{error}</p>
          {#if config}<Button variant="outline" size="sm" disabled={saving || loading} onclick={load}>Reload and discard edits</Button>
          {:else if !config}<Button variant="outline" size="sm" disabled={loading} onclick={load}>Try again</Button>{/if}
        </div>
      {/if}

      <div class="border-t border-border px-5 py-4">
        {#if config && !return404}
          <div class="mb-3 flex items-center gap-3">
            <Label for="{uid}-fallback" class="shrink-0 text-callout text-muted-foreground">Other hostnames:</Label>
            <input id="{uid}-fallback" {...plainText} class="{fieldClass} font-mono text-callout" placeholder="http_status:404 or a service URL" aria-label="Service for other hostnames" bind:value={customFallback} disabled={saving || loading} required />
          </div>
        {/if}
        <Dialog.Footer class="items-center pt-0">
          <Label for="{uid}-404" class="mr-auto gap-2 text-callout">
            <Checkbox id="{uid}-404" bind:checked={return404} disabled={!config || loading || saving} />
            Return 404 for other hostnames
          </Label>
          <Button variant="ghost" disabled={saving} onclick={() => open = false}>Cancel</Button>
          <Button type="submit" disabled={!config || loading || saving || !changed}>{saving ? "Saving…" : "Save changes"}</Button>
        </Dialog.Footer>
      </div>
    </form>
  </Dialog.Content>
</Dialog.Root>
