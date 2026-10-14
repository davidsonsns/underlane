<script lang="ts">
  import CircleMinus from "@lucide/svelte/icons/circle-minus";
  import Plus from "@lucide/svelte/icons/plus";
  import X from "@lucide/svelte/icons/x";
  import { tick, untrack } from "svelte";
  import { api, errorText } from "$lib/api";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Checkbox } from "$lib/components/ui/checkbox/index.js";
  import {
    Content as DialogContent,
    Description as DialogDescription,
    Footer as DialogFooter,
    Header as DialogHeader,
    Root as DialogRoot,
    Title as DialogTitle,
  } from "$lib/components/ui/dialog/index.js";
  import { Label } from "$lib/components/ui/label/index.js";
  import { returnFocusTo } from "$lib/focus";
  import { hostnameChanged, hostnamePayload } from "$lib/hostname-draft";
  import type { HostnameConfig, HostnameRoute } from "$lib/types";

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
  let failureMessage = $state<string | null>(null);
  let removed = $state<{ row: DraftRoute; position: number } | null>(null);
  let table = $state<HTMLDivElement | null>(null);
  let addButton = $state<HTMLButtonElement | null>(null);
  let nextKey = 0;
  let request = 0;
  const uid = $props.id();
  const plainText = {
    autocapitalize: "off",
    autocorrect: "off",
    spellcheck: false,
    writingsuggestions: false,
  } as const;
  const fieldClass =
    "h-8 w-full min-w-0 rounded-xs border-b border-transparent bg-transparent px-1.5 text-sm outline-none transition-colors hover:bg-fill focus:bg-fill disabled:opacity-45 placeholder:text-placeholder";
  const payload = $derived(hostnamePayload(routes, return404, customFallback));
  const changed = $derived(config !== null && hostnameChanged(config, payload));
  // Open at the editor's final height instead of flashing a short loading panel.
  const visible = $derived(open && (!loading || config !== null));

  async function load() {
    request += 1;
    const current = request;
    const { id } = target;
    loading = true;
    failureMessage = null;
    try {
      const loaded = await api.loadHostnames(id);
      if (current !== request) {
        return;
      }
      config = loaded;
      routes = loaded.routes.map((row) => {
        const key = nextKey;
        nextKey += 1;
        return { ...row, key };
      });
      return404 = loaded.fallback_service === "http_status:404";
      customFallback = return404 ? "" : loaded.fallback_service;
      removed = null;
    } catch (e) {
      if (current === request) {
        failureMessage = errorText(e);
      }
    } finally {
      if (current === request) {
        loading = false;
      }
    }
  }

  $effect(() => {
    if (open) {
      untrack(() => {
        config = null;
        routes = [];
        removed = null;
        load();
      });
    } else {
      request += 1;
    }
  });

  async function add() {
    nextKey += 1;
    const key = nextKey;
    routes = [
      ...routes,
      { hostname: "", index: null, key, path: null, service: "" },
    ];
    await tick();
    table
      ?.querySelector<HTMLInputElement>(`[data-hostname-key="${key}"]`)
      ?.focus();
  }

  async function remove(key: number) {
    const position = routes.findIndex((row) => row.key === key);
    if (position < 0) {
      return;
    }
    removed = { position, row: { ...routes[position] } };
    routes = routes.filter((row) => row.key !== key);
    await tick();
    const next = routes[Math.min(position, routes.length - 1)];
    if (next) {
      table
        ?.querySelector<HTMLInputElement>(`[data-hostname-key="${next.key}"]`)
        ?.focus();
    } else {
      addButton?.focus();
    }
  }

  function undo() {
    if (!removed) {
      return;
    }
    const restored = [...routes];
    restored.splice(removed.position, 0, removed.row);
    routes = restored;
    removed = null;
  }

  async function save() {
    if (!config || saving || loading || !changed) {
      return;
    }
    saving = true;
    failureMessage = null;
    try {
      const result = await api.saveHostnames(
        target.id,
        config.revision,
        payload.routes,
        payload.fallbackService
      );
      // Once saved, refresh/restart errors must never invite a second write of this draft.
      open = false;
      await onsaved(target.id, result.restart_error);
    } catch (e) {
      failureMessage = errorText(e);
    } finally {
      saving = false;
    }
  }
</script>
{#snippet hostnameTable()}
  <div
    class="max-h-[min(340px,calc(100vh-280px))] overflow-auto px-5"
    bind:this={table}
  >
    <div
      aria-hidden="true"
      class="grid grid-cols-[minmax(0,1.3fr)_minmax(0,1fr)_28px] gap-x-3 border-b border-border pb-2 text-callout text-muted-foreground"
    >
      <span class="px-1.5">Hostname</span><span class="px-1.5">Service</span
      ><span></span>
    </div>
    {#each routes as row, i (row.key)}
      {@render hostnameRow(row, i)}
    {/each}
    {#if !routes.length}
      <p class="py-5 text-callout text-muted-foreground">
        No hostnames. Add one to route traffic to a service.
      </p>
    {/if}
  </div>
{/snippet}

{#snippet hostnameFooter()}
  <div class="border-t border-border px-5 py-4">
    {#if config && !return404}
      <div class="mb-3 flex items-center gap-3">
        <Label
          class="shrink-0 text-callout text-muted-foreground"
          for="{uid}-fallback"
          >Other hostnames:</Label
        >
        <input
          id="{uid}-fallback"
          {...plainText}
          aria-label="Service for other hostnames"
          class="{fieldClass} font-mono text-callout"
          disabled={saving || loading}
          placeholder="http_status:404 or a service URL"
          required
          bind:value={customFallback}
        >
      </div>
    {/if}
    <DialogFooter class="items-center pt-0">
      <Label class="mr-auto gap-2 text-callout" for="{uid}-404">
        <Checkbox
          disabled={!config || loading || saving}
          id="{uid}-404"
          bind:checked={return404}
        />
        Return 404 for other hostnames
      </Label>
      <Button
        disabled={saving}
        onclick={() => {
          open = false;
        }}
        variant="ghost"
        >Cancel</Button
      >
      <Button disabled={!config || loading || saving || !changed} type="submit"
        >{saving ? "Saving…" : "Save changes"}</Button
      >
    </DialogFooter>
  </div>
{/snippet}

{#snippet hostnameRow(
  row: (typeof routes)[number],
  i: number
)}
  <div
    class="grid grid-cols-[minmax(0,1.3fr)_minmax(0,1fr)_28px] items-center gap-x-3 border-b border-border py-2"
  >
    <div class="min-w-0">
      <input
        {...plainText}
        aria-label="Hostname {i + 1}"
        class={fieldClass}
        data-hostname-key={row.key}
        disabled={saving || loading}
        placeholder="app.example.com"
        required
        bind:value={row.hostname}
      >
      {#if row.path}
        <span
          class="block truncate px-1.5 pt-0.5 font-mono text-xs text-muted-foreground"
          title={row.path}
          >Path: {row.path}</span
        >
      {/if}
    </div>
    <input
      {...plainText}
      aria-label="Service for hostname {i + 1}"
      class="{fieldClass} font-mono text-callout"
      disabled={saving || loading}
      placeholder="localhost:3000"
      required
      bind:value={row.service}
    >
    <Button
      aria-label="Remove {row.hostname || `hostname ${i + 1}`}"
      disabled={saving || loading}
      onclick={() => remove(row.key)}
      size="icon"
      variant="ghost"
      ><CircleMinus /></Button
    >
  </div>
{/snippet}

<DialogRoot
  bind:open={
    () => visible,
    (value) => {
    if (!saving) {
      open = value;
    }
  }
  }
>
  <DialogContent
    class="flex w-[min(700px,calc(100vw-48px))] flex-col gap-0 overflow-hidden p-0"
    onCloseAutoFocus={returnFocusTo(returnFocus)}
    onEscapeKeydown={(e) => {
      if (saving) {
        e.preventDefault();
      }
    }}
    onInteractOutside={(e) => {
      if (saving) {
        e.preventDefault();
      }
    }}
  >
    <DialogHeader class="px-5 pt-5 pb-3">
      <div class="flex items-center justify-between gap-3">
        <DialogTitle>Manage hostnames</DialogTitle>
        <Button
          aria-label="Close"
          disabled={saving}
          onclick={() => {
            open = false;
          }}
          size="icon-sm"
          variant="ghost"
          ><X /></Button
        >
      </div>
      <DialogDescription
        >{target.name}
        · Updates your config with a backup; YAML comments and formatting may
        change.{target.running
          ? " Restarts the tunnel when running."
          : ""}</DialogDescription
      >
    </DialogHeader>

    <form
      aria-busy={loading || saving}
      class="flex min-h-0 flex-col"
      onsubmit={(e) => {
        e.preventDefault();
        save();
      }}
    >
      {#if config}
        {@render hostnameTable()}
        <div class="flex items-center justify-between gap-3 px-5 py-3">
          <Button
            disabled={saving || loading}
            onclick={add}
            variant="link"
            bind:ref={addButton}
            ><Plus />Add hostname</Button
          >
          {#if removed}
            <span
              class="flex items-center gap-2 text-xs text-muted-foreground"
              role="status"
              >Hostname removed
              <Button
                disabled={saving || loading}
                onclick={undo}
                size="xs"
                variant="link"
                >Undo</Button
              ></span
            >
          {/if}
        </div>
        {#if routes.some((row) => row.index === null)}
          <p class="px-5 pb-3 text-xs text-muted-foreground">
            New hostnames also need DNS pointing to this tunnel.
          </p>
        {/if}
      {/if}

      {#if failureMessage}
        <div
          class="mx-5 mb-3 flex items-start justify-between gap-3"
          role="alert"
        >
          <p
            class="min-w-0 select-text text-callout whitespace-pre-wrap text-bad-text"
          >
            {failureMessage}
          </p>
          {#if config}
            <Button
              disabled={saving || loading}
              onclick={load}
              size="sm"
              variant="outline"
              >Reload and discard edits</Button
            >
          {:else if !config}
            <Button
              disabled={loading}
              onclick={load}
              size="sm"
              variant="outline"
              >Try again</Button
            >
          {/if}
        </div>
      {/if}

      {@render hostnameFooter()}
    </form>
  </DialogContent>
</DialogRoot>
