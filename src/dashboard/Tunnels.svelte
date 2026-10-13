<script lang="ts">
  import { tick } from "svelte";
  import { flip } from "svelte/animate";
  import { fly, fade } from "svelte/transition";
  import Eye from "@lucide/svelte/icons/eye";
  import Play from "@lucide/svelte/icons/play";
  import Plus from "@lucide/svelte/icons/plus";
  import RotateCw from "@lucide/svelte/icons/rotate-cw";
  import Square from "@lucide/svelte/icons/square";
  import { store } from "../lib/store.svelte";
  import { api } from "../lib/api";
  import { motion } from "../lib/theme";
  import { isOn, statusHint, statusLabel } from "../lib/format";
  import type { Tunnel } from "../lib/types";
  import Toolbar from "$lib/components/app/Toolbar.svelte";
  import StatusDot from "$lib/components/app/StatusDot.svelte";
  import StatePanel from "$lib/components/app/StatePanel.svelte";
  import Tip from "$lib/components/app/Tip.svelte";
  import { kbRing } from "$lib/components/ui/mac";
  import { Button } from "$lib/components/ui/button/index.js";
  import { mergeProps } from "bits-ui";
  import { cn } from "$lib/utils";
  import TunnelDetail from "./TunnelDetail.svelte";
  import GroupName from "./GroupName.svelte";

  let { focusId = null, onadd }: { focusId?: string | null; onadd: (e: MouseEvent) => void } = $props();
  let selected = $state<string | null>(null);
  $effect(() => {
    if (focusId) selected = focusId;
  });
  $effect(() => {
    if (!selected && store.tunnels.length) selected = store.tunnels[0].id;
    if (selected && store.overview && !store.tunnels.some((t) => t.id === selected)) selected = store.tunnels[0]?.id ?? null;
  });

  const groups = $derived.by(() => {
    const map = new Map<string | null, { name: string; tunnels: Tunnel[] }>();
    for (const t of store.tunnels) {
      const g = map.get(t.group_id) ?? { name: t.group_name, tunnels: [] };
      g.tunnels.push(t);
      map.set(t.group_id, g);
    }
    return [...map.entries()].map(([id, g]) => ({ id, ...g }));
  });
  const current = $derived(store.tunnels.find((t) => t.id === selected) ?? null);
  const anyOn = $derived(store.tunnels.some(isOn));
  let list = $state<HTMLElement | null>(null);
  let emptyAdd = $state<HTMLElement | null>(null);

  // TunnelDetail's Remove: select the next tunnel in list order (else the previous one) and
  // focus it in the list, since the detail that held focus unmounts (02 2b). Removing the last
  // tunnel focuses the empty state's Add tunnel instead of leaving focus on <body>.
  async function remove(id: string) {
    const order = groups.flatMap((g) => g.tunnels);
    const i = order.findIndex((t) => t.id === id);
    const next = order[i + 1] ?? order[i - 1] ?? null;
    await api.deleteTunnel(id);
    selected = next?.id ?? null;
    await store.refresh();
    await tick();
    if (next) list?.querySelector<HTMLElement>(`[data-tunnel-id="${next.id}"]`)?.focus();
    else emptyAdd?.focus();
  }

  // O3: the list is one Tab stop (the selected tunnel); Up/Down/Home/End move the selection
  // and focus with it, like a source list. Rows fading out are inert, so they're skipped.
  function onListKey(e: KeyboardEvent) {
    const rows = [...(list?.querySelectorAll<HTMLElement>("[data-tunnel-id]:not([inert])") ?? [])];
    const i = rows.indexOf(document.activeElement as HTMLElement);
    const to = ({ ArrowDown: i + 1, ArrowUp: i - 1, Home: 0, End: rows.length - 1 } as Record<string, number>)[e.key];
    if (i < 0 || to === undefined || e.metaKey || e.altKey || e.ctrlKey) return;
    e.preventDefault();
    const row = rows[Math.max(0, Math.min(rows.length - 1, to))];
    selected = row.dataset.tunnelId ?? selected;
    document.documentElement.dataset.kbNav = ""; // arrows count as keyboard navigation (03 R-13)
    row.focus();
  }
</script>

<div class="flex h-full flex-col">
  <Toolbar title="Tunnels">
    {#if store.tunnels.length}
      {#if anyOn}
        <Tip text="Restart every running tunnel">
          {#snippet children({ props })}
            <Button variant="outline" {...mergeProps(props, { onclick: () => api.restartAll() })}><RotateCw class="size-3.5" /> Restart all</Button>
          {/snippet}
        </Tip>
        <Button variant="outline" onclick={() => api.stopAll()}><Square class="size-2.5 fill-current" /> Stop all</Button>
      {:else}
        <Button variant="outline" onclick={() => api.startAll()}><Play class="size-2.5 fill-current" /> Start all</Button>
      {/if}
    {/if}
    <Button data-add-tunnel onclick={onadd}><Plus class="size-3.5" /> Add tunnel</Button>
  </Toolbar>

  {#if store.loading}
    <div class="grid flex-1 place-items-center"><StatePanel kind="loading" title="Loading tunnels…" /></div>
  {:else if store.error}
    <div class="grid flex-1 place-items-center">
      <StatePanel kind="error" title="Couldn't load tunnels" message={store.error}>
        <Button variant="outline" onclick={() => store.refresh()}>Try again</Button>
      </StatePanel>
    </div>
  {:else if store.tunnels.length === 0}
    <div class="grid flex-1 place-items-center">
      <StatePanel kind="empty" title="Set up your first tunnel" message="Underlane runs cloudflared for you. Import the config files you already have, paste a token from the Cloudflare dashboard, or share a local port with a quick tunnel; no account needed for that one.">
        <Button bind:ref={emptyAdd} onclick={onadd}>Add tunnel</Button>
      </StatePanel>
    </div>
  {:else}
    <div class="grid min-h-0 flex-1 grid-cols-[240px_minmax(0,1fr)]">
      <!-- The keydown only moves focus between the buttons inside (O3). -->
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <nav class="flex flex-col gap-2 overflow-y-auto border-e-[0.5px] border-border bg-sidebar/40 p-3" aria-label="Tunnels" bind:this={list} onkeydown={onListKey}>
        {#each groups as g (g.id)}
          <div class="min-w-0" animate:flip={{ duration: motion(200) }}>
            {#if g.id !== null}<GroupName id={g.id} name={g.name} />{/if}
            {#each g.tunnels as t (t.id)}
              <button
                class={cn("flex w-full items-center gap-2.5 rounded-md px-2.5 py-2.5 transition-colors text-left", t.id === selected ? "bg-(--selected)" : "hover:bg-fill", kbRing)}
                data-tunnel-id={t.id}
                tabindex={t.id === selected ? 0 : -1}
                aria-current={t.id === selected ? "true" : undefined}
                aria-label={`${t.name}, ${statusLabel[t.state.status]}`}
                onclick={(e) => {
                  selected = t.id;
                  // WebKit doesn't focus a clicked button: focus it so arrow keys work after a click.
                  e.currentTarget.focus({ preventScroll: true });
                }}
                animate:flip={{ duration: motion(200) }}
                in:fly={{ x: -8, duration: motion(200) }}
                out:fade={{ duration: motion(120) }}
              >
                <!-- The row label includes status; the dot's hint is hover-only (no tab stop). -->
                <Tip text={statusHint[t.state.status]}>
                  {#snippet children({ props })}<span {...{ ...props, tabindex: undefined }} class="flex"><StatusDot status={t.state.status} /></span>{/snippet}
                </Tip>
                <span class="min-w-0 flex-1 truncate font-medium">{t.name}</span>
                {#if t.inspect}
                  <Tip text="Requests are inspected">
                    {#snippet children({ props })}
                      <span {...{ ...props, tabindex: undefined }} class="flex text-muted-foreground" role="img" aria-label="Requests are inspected"><Eye class="size-3" aria-hidden="true" /></span>
                    {/snippet}
                  </Tip>
                {/if}
              </button>
            {/each}
          </div>
        {/each}
      </nav>
      <div class="min-w-0 overflow-y-auto">
        {#if current}
          {#key current.id}
            <div class="mx-auto max-w-[880px] px-8 pt-7 pb-10" in:fade={{ duration: motion(120) }}>
              <TunnelDetail tunnel={current} onremove={remove} />
            </div>
          {/key}
        {/if}
      </div>
    </div>
  {/if}
</div>
