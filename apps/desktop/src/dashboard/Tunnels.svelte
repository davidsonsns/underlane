<script lang="ts">
  import Eye from "@lucide/svelte/icons/eye";
  import Play from "@lucide/svelte/icons/play";
  import Plus from "@lucide/svelte/icons/plus";
  import RotateCw from "@lucide/svelte/icons/rotate-cw";
  import Square from "@lucide/svelte/icons/square";
  import { mergeProps } from "bits-ui";
  import { tick } from "svelte";
  import { flip } from "svelte/animate";
  import { fade, fly } from "svelte/transition";
  import StatePanel from "$lib/components/app/StatePanel.svelte";

  import Tip from "$lib/components/app/Tip.svelte";
  import Toolbar from "$lib/components/app/Toolbar.svelte";
  import TunnelStatus from "$lib/components/app/TunnelStatus.svelte";
  import { Button } from "$lib/components/ui/button/index.js";
  import { kbRing } from "$lib/components/ui/mac";
  import { groupTunnels } from "$lib/tunnel-groups";
  import { cn } from "$lib/utils";
  import { api } from "../lib/api";
  import { isOn, statusLabel } from "../lib/format";
  import { store } from "../lib/store.svelte";
  import { motion } from "../lib/theme";

  import GroupName from "./GroupName.svelte";
  import TunnelDetail from "./TunnelDetail.svelte";

  let {
    focusId = null,
    onadd,
  }: { focusId?: string | null; onadd: (e: MouseEvent) => void } = $props();
  let selected = $state<string | null>(null);
  $effect(() => {
    if (focusId) {
      selected = focusId;
    }
  });
  $effect(function reconcileSelection() {
    if (!selected && store.tunnels.length) {
      selected = store.tunnels[0].id;
    }
    if (
      selected &&
      store.overview &&
      !store.tunnels.some((t) => t.id === selected)
    ) {
      selected = store.tunnels[0]?.id ?? null;
    }
  });

  const groups = $derived(groupTunnels(store.tunnels));
  const current = $derived(
    store.tunnels.find((t) => t.id === selected) ?? null
  );
  const anyOn = $derived(store.tunnels.some(isOn));
  let list = $state<HTMLElement | null>(null);
  let emptyAdd = $state<HTMLElement | null>(null);

  // TunnelDetail's Remove: select the next tunnel in list order (else the previous one) and
  // focus it in the list, since the detail that held focus unmounts (02 2b). Removing the last
  // tunnel focuses the empty state's New button instead of leaving focus on <body>.
  async function remove(id: string, deleteConfig: boolean) {
    const order = groups.flatMap((g) => g.tunnels);
    const i = order.findIndex((t) => t.id === id);
    const next = order[i + 1] ?? order[i - 1] ?? null;
    await api.deleteTunnel(id, deleteConfig);
    selected = next?.id ?? null;
    await store.refresh();
    await tick();
    if (next) {
      list
        ?.querySelector<HTMLElement>(`[data-tunnel-id="${next.id}"]`)
        ?.focus();
    } else {
      emptyAdd?.focus();
    }
  }

  // O3: the list is one Tab stop (the selected tunnel); Up/Down/Home/End move the selection
  // and focus with it, like a source list. Rows fading out are inert, so they're skipped.
  function onListKey(e: KeyboardEvent) {
    const rows = [
      ...(list?.querySelectorAll<HTMLElement>(
        "[data-tunnel-id]:not([inert])"
      ) ?? []),
    ];
    const i = rows.indexOf(document.activeElement as HTMLElement);
    const to = (
      {
        ArrowDown: i + 1,
        ArrowUp: i - 1,
        End: rows.length - 1,
        Home: 0,
      } as Record<string, number>
    )[e.key];
    if (i < 0 || to === undefined || e.metaKey || e.altKey || e.ctrlKey) {
      return;
    }
    e.preventDefault();
    const row = rows[Math.max(0, Math.min(rows.length - 1, to))];
    selected = row.dataset.tunnelId ?? selected;
    document.documentElement.dataset.kbNav = ""; // arrows count as keyboard navigation (03 R-13)
    row.focus();
  }
</script>
{#snippet tunnelToolbar()}
  <Toolbar title="Projects">
    {#if store.tunnels.length}
      {#if anyOn}
        <Tip text="Restart every running tunnel">
          {#snippet children({
            props,
          })}
            <Button
              variant="outline"
              {...mergeProps(props, { onclick: () => api.restartAll() })}
              ><RotateCw class="size-3.5" />
              Restart all</Button
            >
          {/snippet}
        </Tip>
        <Button onclick={() => api.stopAll()} variant="outline"
          ><Square class="size-2.5 fill-current" />
          Stop all</Button
        >
      {:else}
        <Button onclick={() => api.startAll()} variant="outline"
          ><Play class="size-2.5 fill-current" />
          Start all</Button
        >
      {/if}
    {/if}
    <Button data-add-tunnel onclick={onadd}
      ><Plus class="size-3.5" />
      New</Button
    >
  </Toolbar>
{/snippet}

{#snippet tunnelRows(
  g: (typeof groups)[number]
)}
  {#each g.tunnels as t (t.id)}
    <button
      aria-current={t.id === selected ? "true" : undefined}
      aria-label={`${t.name}, ${statusLabel[t.state.status]}`}
      class={cn(
        "flex h-8 w-full items-center gap-2.5 rounded-md px-2.5 text-left transition-colors",
        t.id === selected ? "bg-(--selected)" : "hover:bg-fill",
        kbRing
      )}
      data-tunnel-id={t.id}
      onclick={(e) => {
        selected = t.id;
        // WebKit doesn't focus a clicked button: focus it so arrow keys work after a click.
        e.currentTarget.focus({ preventScroll: true });
      }}
      tabindex={t.id === selected ? 0 : -1}
      type="button"
      in:fly={{ duration: motion(200), x: -8 }}
      out:fade={{ duration: motion(120) }}
      animate:flip={{ duration: motion(200) }}
    >
      <!-- The row label includes status; the dot's hint is hover-only (no tab stop). -->
      <TunnelStatus status={t.state.status} />
      <span class="min-w-0 flex-1 truncate font-medium">{t.name}</span>
      {#if t.inspect}
        <Tip text="Requests are inspected">
          {#snippet children({
            props,
          })}
            <span
              {...{ ...props, tabindex: undefined }}
              aria-label="Requests are inspected"
              class="flex text-muted-foreground"
              role="img"
              ><Eye aria-hidden="true" class="size-3" /></span
            >
          {/snippet}
        </Tip>
      {/if}
    </button>
  {/each}
{/snippet}

<div class="flex h-full flex-col">
  {@render tunnelToolbar()}

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
      >
        <Button onclick={() => store.refresh()} variant="outline"
          >Try again</Button
        >
      </StatePanel>
    </div>
  {:else if store.tunnels.length === 0}
    <div class="grid flex-1 place-items-center">
      <StatePanel
        kind="empty"
        message="Start with a new config or load one you already have. For a temporary public URL, create a quick tunnel; no Cloudflare account needed."
        title="Create your first project"
      >
        <Button onclick={onadd} bind:ref={emptyAdd}>New</Button>
      </StatePanel>
    </div>
  {:else}
    <div class="grid min-h-0 flex-1 grid-cols-[240px_minmax(0,1fr)]">
      <!-- The keydown only moves focus between the buttons inside (O3). -->
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <!-- biome-ignore lint/a11y/noNoninteractiveElementInteractions: The navigation handles arrow keys for its focusable tunnel buttons. -->
      <nav
        aria-label="Projects"
        class="flex flex-col gap-2 overflow-y-auto border-e-[0.5px] border-border bg-sidebar/40 p-3"
        onkeydown={onListKey}
        bind:this={list}
      >
        {#each groups as g (g.id)}
          <div
            class="flex min-w-0 flex-col gap-1"
            animate:flip={{ duration: motion(200) }}
          >
            {#if g.id !== null}
              <GroupName id={g.id} name={g.name} />
            {/if}
            {@render tunnelRows(g)}
          </div>
        {/each}
      </nav>
      <div class="min-w-0 overflow-y-auto">
        {#if current}
          {#key current.id}
            <div
              class="mx-auto max-w-[880px] px-8 pt-7 pb-10"
              in:fade={{ duration: motion(120) }}
            >
              <TunnelDetail onremove={remove} tunnel={current} />
            </div>
          {/key}
        {/if}
      </div>
    </div>
  {/if}
</div>
