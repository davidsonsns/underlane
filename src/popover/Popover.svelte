<script lang="ts">
  import { onMount, tick } from "svelte";
  import { flip } from "svelte/animate";
  import { fly, fade } from "svelte/transition";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { listen } from "@tauri-apps/api/event";
  import ExternalLink from "@lucide/svelte/icons/external-link";
  import PanelLeft from "@lucide/svelte/icons/panel-left";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import { store } from "../lib/store.svelte";
  import { api } from "../lib/api";
  import { motion } from "../lib/theme";
  import { isOn, publicUrl, statusHint, statusLabel } from "../lib/format";
  import { next } from "../lib/listnav";
  import type { Tunnel } from "../lib/types";
  import { mergeProps } from "bits-ui";
  import { cn } from "$lib/utils";
  import { kbRing } from "$lib/components/ui/mac.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Switch } from "$lib/components/ui/switch/index.js";
  import * as Alert from "$lib/components/ui/alert/index.js";
  // The provider alone, not the ui/tooltip barrel (Tip brings the rest).
  import TooltipProvider from "$lib/components/ui/tooltip/tooltip-provider.svelte";
  import StatusDot from "$lib/components/app/StatusDot.svelte";
  import RouteLine from "$lib/components/app/RouteLine.svelte";
  import StatePanel from "$lib/components/app/StatePanel.svelte";
  import CopyButton from "$lib/components/app/CopyButton.svelte";
  import IconAction from "$lib/components/app/IconAction.svelte";
  import Tip, { preloadTipContent } from "$lib/components/app/Tip.svelte";
  import { notify } from "$lib/toast";

  let busy = $state<Record<string, boolean>>({});
  let root = $state<HTMLElement | null>(null);

  // svelte-sonner and the Toaster load on the first toast (02 O10), so the popover's first open
  // doesn't pay for them. The toast list is global, so the toast that loaded it still shows.
  let toaster = $state(false);
  notify.onLoad(() => (toaster = true));

  /** Focus the first tunnel row (or "Add a tunnel" when there are none), without a ring:
   *  AppKit shows none until the first key (02 §4.4). */
  function focusFirst() {
    delete document.documentElement.dataset.kbNav;
    root?.querySelector<HTMLElement>("[data-row-main], [data-empty-add]")?.focus();
  }

  onMount(() => {
    // The first open creates this webview, and tray.rs emits popover-shown right after show(),
    // which can be before the listener below exists. So focus after the first load too (R-27).
    store.init().then(tick).then(focusFirst).then(() => {
      // The tooltip content is lazy (02 O10); fetch it once the first open has settled, so it's
      // there before the first hover instead of mounting mid-hover.
      setTimeout(preloadTipContent, 300);
    });
    // Refresh when shown, in case tunnels were added in the dashboard.
    const un = listen("popover-shown", async () => {
      notify.dismiss(); // also covered by the blur handler; this doesn't depend on blur firing
      await store.refresh();
      await tick();
      focusFirst();
    });
    return () => {
      un.then((f) => f());
    };
  });

  const NAV_KEYS = new Set(["ArrowDown", "ArrowUp", "Home", "End"]);

  // Escape hides the popover; Up/Down/Home/End move between the rows (F12). Tab order inside a
  // row stays native: main, copy, open, switch.
  function onkeydown(e: KeyboardEvent) {
    const el = e.target as HTMLElement | null;
    if (el?.closest("input, textarea, select, [contenteditable]")) return;
    if (e.key === "Escape") {
      api.hidePopover();
      return;
    }
    if (!NAV_KEYS.has(e.key) || e.metaKey || e.ctrlKey || e.altKey || !root) return;
    const rows = [...root.querySelectorAll<HTMLElement>("[data-row-main]")];
    const current = document.activeElement?.closest("[data-row]")?.querySelector<HTMLElement>("[data-row-main]");
    // From outside the rows (header, footer), Up goes to the last row and Down to the first.
    const i = !current && e.key === "ArrowUp" ? rows.length - 1 : next(e.key, current ? rows.indexOf(current) : -1, rows.length);
    if (i === null || i < 0) return;
    e.preventDefault();
    // installFocusModality only reacts to Tab; the arrows are keyboard navigation too (R-13).
    document.documentElement.dataset.kbNav = "";
    rows[i].focus();
  }

  const groups = $derived.by(() => {
    const map = new Map<string, { name: string; tunnels: Tunnel[] }>();
    for (const t of store.tunnels) {
      const g = map.get(t.project_id) ?? { name: t.project_name, tunnels: [] };
      g.tunnels.push(t);
      map.set(t.project_id, g);
    }
    return [...map.entries()].map(([id, g]) => ({ id, ...g }));
  });

  async function toggle(t: Tunnel, on: boolean) {
    busy[t.id] = true;
    try {
      if (on) await api.start(t.id);
      else await api.stop(t.id);
    } catch (e) {
      notify.error(e, on ? `Couldn't start ${t.name}` : `Couldn't stop ${t.name}`);
    } finally {
      busy[t.id] = false;
    }
  }

  const anyOn = $derived(store.tunnels.some(isOn));
  const summary = $derived.by(() => {
    const n = store.activeCount;
    const starting = store.tunnels.filter((t) => t.state.status === "starting").length;
    if (n === 0 && starting === 0) return "No tunnels running";
    const parts = [];
    if (n) parts.push(`${n} running`);
    if (starting) parts.push(`${starting} starting`);
    return parts.join(", ");
  });

  const detail = "truncate text-code text-muted-foreground";
  // The popover's text buttons keep the label colour (the old .btn.ghost); icon buttons stay muted.
  const ghost = "text-callout text-foreground";
</script>

<!-- The popover hides on blur (tray.rs); drop toasts so a stale one doesn't reappear. -->
<svelte:window onblur={() => notify.dismiss()} {onkeydown} />

<TooltipProvider delayDuration={250} ignoreNonKeyboardFocus>
<!-- The radius must equal `radius: Some(12.0)` in tray.rs (the popover window's vibrancy corner).
     The tint sits on this root; html and body stay transparent (app.css). -->
<main bind:this={root} class="relative flex h-screen flex-col overflow-hidden rounded-[12px] bg-(--popover-tint)">
  <header class="flex items-center justify-between pt-3 pr-3 pb-2 pl-3.5">
    <div class="flex flex-col">
      <strong class="text-sm font-semibold">Burrow</strong>
      <span class="text-xs text-muted-foreground">{summary}</span>
    </div>
    {#if store.tunnels.length}
      {#if anyOn}
        <Button variant="ghost" class={ghost} onclick={() => api.stopAll()}>Stop all</Button>
      {:else}
        <Button variant="ghost" class={ghost} onclick={() => api.startAll()}>Start all</Button>
      {/if}
    {/if}
  </header>

  {#if store.overview && !store.overview.cloudflared}
    <Alert.Root variant="warning" class="mx-2.5 mb-1.5 w-auto">
      <TriangleAlert />
      <Alert.Description>cloudflared isn't installed. Run <code class="font-mono text-xs">brew install cloudflared</code>, then open Settings.</Alert.Description>
    </Alert.Root>
  {/if}

  <section class="flex-1 overflow-y-auto px-1.5 pb-1.5">
    {#if store.loading}
      <StatePanel kind="loading" title="Loading tunnels…" />
    {:else if store.error}
      <StatePanel kind="error" title="Couldn't load tunnels" message={store.error}>
        <Button variant="outline" onclick={() => store.refresh()}>Try again</Button>
      </StatePanel>
    {:else if store.tunnels.length === 0}
      <StatePanel kind="empty" title="No tunnels yet" message="Import a config.yml, paste a tunnel token, or share a local port with a quick tunnel.">
        <Button data-empty-add onclick={() => api.openDashboard("add")}>Add a tunnel</Button>
      </StatePanel>
    {:else}
      {#each groups as g (g.id)}
        <div class="not-first:mt-1.5" animate:flip={{ duration: motion(220) }}>
          <div class="px-2 pt-1.5 pb-0.75 text-xs font-semibold text-muted-foreground">{g.name}</div>
          {#each g.tunnels as t (t.id)}
            {@const m = t.state.mappings.find((m) => m.hostname) ?? t.state.mappings[0]}
            {@const url = publicUrl(t)}
            <div
              class="group/row flex items-center gap-2.25 rounded-lg px-2 py-1.5 transition-colors duration-120 hover:bg-fill"
              data-row
              animate:flip={{ duration: motion(220) }}
              in:fly={{ y: -6, duration: motion(200) }}
              out:fade={{ duration: motion(120) }}
            >
              <!-- The status shows in words in the row when it matters, so the dot's hint is hover-only (no tab stop). -->
              <Tip text={statusHint[t.state.status]}>
                {#snippet children({ props })}<span {...{ ...props, tabindex: undefined }} class="flex"><StatusDot status={t.state.status} /></span>{/snippet}
              </Tip>
              <button
                class={cn("-mx-1 -my-0.5 flex min-w-0 flex-1 flex-col gap-px rounded-md px-1 py-0.5 text-left", kbRing)}
                data-row-main
                onclick={() => api.openDashboard(`tunnels:${t.id}`)}
              >
                <span class="truncate font-medium">{t.name}</span>
                {#if t.state.status === "crashed" || t.state.status === "degraded"}
                  <span class={cn(detail, t.state.status === "crashed" && "text-bad-text")}>{t.state.detail ?? statusLabel[t.state.status]}</span>
                {:else if m}
                  <RouteLine hostname={t.kind === "quick" ? (t.state.public_url?.replace("https://", "") ?? null) : m.hostname} service={m.service} inspected={m.inspected} pulse={store.pulses[t.id] ?? 0} compact />
                {:else if t.kind === "token"}
                  <span class={detail}>Routes come from the Cloudflare dashboard</span>
                {:else if t.kind === "quick"}
                  <span class={cn(detail, "font-mono")}>{t.origin_url}</span>
                {/if}
              </button>
              <!-- Shown on hover, and while a control inside has keyboard focus. -->
              <div class="flex opacity-0 transition-opacity duration-120 group-hover/row:opacity-100 focus-within:opacity-100">
                {#if url && t.state.status === "connected"}
                  <CopyButton value={url} what={url.replace("https://", "")} tip="Copy public URL" />
                  <IconAction label="Open in browser" onclick={() => openUrl(url)}><ExternalLink /></IconAction>
                {/if}
              </div>
              <!-- Busy while a start/stop is in flight: disabled, announced, and the thumb pulses. -->
              <Switch
                bind:checked={() => isOn(t), (v) => toggle(t, v)}
                disabled={busy[t.id]}
                aria-busy={busy[t.id] || undefined}
                aria-label={isOn(t) ? `Stop ${t.name}` : `Start ${t.name}`}
                class={cn(busy[t.id] && "*:animate-pulse")}
              />
            </div>
          {/each}
        </div>
      {/each}
    {/if}
  </section>

  <footer class="flex justify-between border-t-[0.5px] border-border px-2 pt-1.5 pb-2">
    <Button variant="ghost" class="text-foreground" onclick={() => api.openDashboard()}><PanelLeft class="size-3.5" /> Open Dashboard</Button>
    <Tip text="Quit Burrow and stop all tunnels" side="top">
      {#snippet children({ props })}<Button {...mergeProps(props, { onclick: () => api.quit() })} variant="ghost" class="text-foreground">Quit</Button>{/snippet}
    </Tip>
  </footer>
</main>

{#if toaster}
  {#await import("$lib/lazy") then { Toaster }}
    <!-- Above the footer; the window is 360px wide, so sonner uses mobileOffset (<= 600px). -->
    <Toaster
      theme={store.overview?.settings.theme ?? "system"}
      position="bottom-center"
      visibleToasts={1}
      offset={{ bottom: 52 }}
      mobileOffset={{ bottom: 52, left: 20, right: 20 }}
    />
  {/await}
{/if}
</TooltipProvider>
