<script lang="ts">
  import StatePanel from "$lib/components/app/StatePanel.svelte";
  import Toolbar from "$lib/components/app/Toolbar.svelte";
  import { Button } from "$lib/components/ui/button/index.js";
  import { store } from "$lib/store.svelte";
  import TunnelDetail from "./TunnelDetail.svelte";

  let {
    selectedId,
    onadd,
    onremove,
  }: {
    selectedId: string | null;
    onadd: (event: MouseEvent) => void;
    onremove: (id: string, deleteConfig: boolean) => Promise<void>;
  } = $props();
  const current = $derived(
    store.tunnels.find((tunnel) => tunnel.id === selectedId) ?? null
  );
</script>

<div class="flex h-full min-h-0 flex-col">
  {#if store.loading || store.error || !current}
    <Toolbar title="Projects" />
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
      <TunnelDetail {onremove} tunnel={current} />
    {/key}
  {/if}
</div>
