<script lang="ts" module>
  export type ProjectTab = "overview" | "requests" | "logs";
</script>

<script lang="ts">
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import Play from "@lucide/svelte/icons/play";
  import Plus from "@lucide/svelte/icons/plus";
  import RotateCw from "@lucide/svelte/icons/rotate-cw";
  import Square from "@lucide/svelte/icons/square";
  import { DropdownMenu } from "bits-ui";
  import { api, errorText } from "$lib/api";
  import StatePanel from "$lib/components/app/StatePanel.svelte";
  import { Button } from "$lib/components/ui/button/index.js";
  import { menuContent, menuItem } from "$lib/components/ui/mac";
  import {
    Content as TabsContent,
    List as TabsList,
    Root as TabsRoot,
    Trigger as TabsTrigger,
  } from "$lib/components/ui/tabs/index.js";
  import { isOn } from "$lib/format";
  import { store } from "$lib/store.svelte";
  import { cn } from "$lib/utils";
  import Logs from "./Logs.svelte";
  import Requests from "./Requests.svelte";
  import TunnelDetail from "./TunnelDetail.svelte";

  let {
    selectedId,
    tab = $bindable("overview"),
    requestsRef = $bindable(null),
    onadd,
    onremove,
  }: {
    selectedId: string | null;
    tab?: ProjectTab;
    requestsRef?: {
      selectFirst: (side?: "req" | "resp") => Promise<void>;
    } | null;
    onadd: (event: MouseEvent) => void;
    onremove: (id: string, deleteConfig: boolean) => Promise<void>;
  } = $props();
  const current = $derived(
    store.tunnels.find((tunnel) => tunnel.id === selectedId) ?? null
  );
  const anyOn = $derived(store.tunnels.some(isOn));
  const projectViews = [
    { label: "Overview", value: "overview" },
    { label: "Requests", value: "requests" },
    { label: "Logs", value: "logs" },
  ] as const;
  let bulkBusy = $state(false);
  let bulkError = $state<string | null>(null);

  async function bulk(action: () => Promise<void>) {
    if (bulkBusy) {
      return;
    }
    bulkBusy = true;
    bulkError = null;
    try {
      await action();
    } catch (error) {
      bulkError = errorText(error);
    } finally {
      bulkBusy = false;
    }
  }
</script>

{#snippet projectTabs()}
  <TabsList
    aria-label="Project views"
    class="mb-4 h-10 w-full justify-start gap-5 border-b-[0.5px] border-border"
    variant="line"
  >
    {#each projectViews as item}
      <TabsTrigger
        class="flex-none rounded-none px-0 text-callout after:bottom-[-1px] after:bg-primary data-[state=inactive]:text-muted-foreground"
        value={item.value}
      >
        {item.label}
      </TabsTrigger>
    {/each}
  </TabsList>
{/snippet}

{#snippet projectActivity()}
  <TabsContent class="mt-0 min-h-0 flex-1" value="requests">
    {#if current && tab === "requests"}
      <Requests embedded scopeId={current.id} bind:this={requestsRef} />
    {/if}
  </TabsContent>
  <TabsContent class="mt-0 min-h-0 flex-1" value="logs">
    {#if current && tab === "logs"}
      <Logs embedded scopeId={current.id} />
    {/if}
  </TabsContent>
{/snippet}

<div class="flex h-full min-h-0 flex-col">
  <header
    class="flex h-(--toolbar-h) shrink-0 items-center justify-between gap-3 border-b-[0.5px] border-border px-5"
    data-tauri-drag-region
  >
    <h1
      class="flex min-w-0 items-center gap-2 text-callout"
      data-tauri-drag-region
    >
      <span class="truncate text-muted-foreground" data-tauri-drag-region
        >{current?.group_name ||
          (store.overview?.groups.length ? "Ungrouped" : "Projects")}</span
      >
      {#if current}
        <span class="text-faint" data-tauri-drag-region>/</span
        ><span class="truncate" data-tauri-drag-region>{current.name}</span>
      {/if}
    </h1>
    <DropdownMenu.Root>
      <DropdownMenu.Trigger>
        {#snippet child({
          props,
        })}
          <Button
            {...props}
            class="text-xs text-muted-foreground"
            variant="ghost"
            >Project actions<ChevronDown class="size-3" /></Button
          >
        {/snippet}
      </DropdownMenu.Trigger>
      <DropdownMenu.Portal>
        <DropdownMenu.Content align="end" class={menuContent} sideOffset={3}>
          {#if store.tunnels.length}
            {#if anyOn}
              <DropdownMenu.Item
                class={cn(menuItem, "ps-2")}
                disabled={bulkBusy}
                onSelect={() => bulk(api.restartAll)}
                ><RotateCw />Restart all running</DropdownMenu.Item
              >
              <DropdownMenu.Item
                class={cn(menuItem, "ps-2")}
                disabled={bulkBusy}
                onSelect={() => bulk(api.stopAll)}
                ><Square />Stop all</DropdownMenu.Item
              >
            {:else}
              <DropdownMenu.Item
                class={cn(menuItem, "ps-2")}
                disabled={bulkBusy}
                onSelect={() => bulk(api.startAll)}
                ><Play />Start all</DropdownMenu.Item
              >
            {/if}
          {/if}
          <DropdownMenu.Item class={cn(menuItem, "ps-2")} onclick={onadd}
            ><Plus />New project</DropdownMenu.Item
          >
        </DropdownMenu.Content>
      </DropdownMenu.Portal>
    </DropdownMenu.Root>
  </header>
  {#if bulkError}
    <p
      class="border-b border-border px-5 py-2 text-xs text-destructive"
      role="alert"
    >
      {bulkError}
    </p>
  {/if}
  {#if store.loading}
    <div class="grid flex-1 place-items-center">
      <StatePanel kind="loading" title="Loading projects…" />
    </div>
  {:else if store.error}
    <div class="grid flex-1 place-items-center">
      <StatePanel
        kind="error"
        message={store.error}
        title="Couldn't load projects"
        ><Button onclick={() => store.refresh()} variant="outline"
          >Try again</Button
        ></StatePanel
      >
    </div>
  {:else if !current}
    <div class="grid flex-1 place-items-center">
      <StatePanel
        kind="empty"
        message="Start with a new config or load one you already have. For a temporary public URL, create a quick tunnel; no Cloudflare account needed."
        title="Create your first project"
        ><Button onclick={onadd}>New</Button></StatePanel
      >
    </div>
  {:else}
    {#key current.id}
      <TabsRoot class="min-h-0 flex-1 gap-0" bind:value={tab}>
        <TunnelDetail
          activity={projectActivity}
          navigation={projectTabs}
          {onremove}
          tunnel={current}
        />
      </TabsRoot>
    {/key}
  {/if}
</div>
