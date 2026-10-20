<script lang="ts">
  import ExternalLink from "@lucide/svelte/icons/external-link";
  import PanelLeft from "@lucide/svelte/icons/panel-left";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import { listen } from "@tauri-apps/api/event";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { mergeProps } from "bits-ui";
  import { onMount, tick } from "svelte";
  import { flip } from "svelte/animate";
  import { fade, fly } from "svelte/transition";
  import CopyButton from "$lib/components/app/CopyButton.svelte";
  import IconAction from "$lib/components/app/IconAction.svelte";
  import RouteLine from "$lib/components/app/RouteLine.svelte";
  import StatePanel from "$lib/components/app/StatePanel.svelte";
  import Tip, { preloadTipContent } from "$lib/components/app/Tip.svelte";
  import TunnelStatus from "$lib/components/app/TunnelStatus.svelte";
  import {
    Description as AlertDescription,
    Root as AlertRoot,
    Title as AlertTitle,
  } from "$lib/components/ui/alert/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { kbRing } from "$lib/components/ui/mac.js";
  import { Switch } from "$lib/components/ui/switch/index.js";
  // The provider alone, not the ui/tooltip barrel (Tip brings the rest).
  import TooltipProvider from "$lib/components/ui/tooltip/tooltip-provider.svelte";
  import { connectionWarning, resolvedOriginFailure } from "$lib/connection";
  import {
    popoverKeyAction,
    popoverRowIndex,
    tunnelSummary,
  } from "$lib/popover-navigation";
  import { notify } from "$lib/toast";
  import { activeTunnelOwner } from "$lib/tunnel-conflict";
  import { groupTunnels } from "$lib/tunnel-groups";
  import { cn } from "$lib/utils";
  import { api } from "../lib/api";
  import { isOn, publicUrl, statusLabel } from "../lib/format";
  import { store } from "../lib/store.svelte";
  import { motion } from "../lib/theme";
  import type { Tunnel } from "../lib/types";

  let busy = $state<Record<string, boolean>>({});
  let root = $state<HTMLElement | null>(null);

  // svelte-sonner and the Toaster load on the first toast (02 O10), so the popover's first open
  // doesn't pay for them. The toast list is global, so the toast that loaded it still shows.
  let toaster = $state(false);
  notify.onLoad(() => {
    toaster = true;
  });

  /** Focus the first tunnel row (or "New" when there are none), without a ring:
   *  AppKit shows none until the first key (02 §4.4). */
  function focusFirst() {
    delete document.documentElement.dataset.kbNav;
    root
      ?.querySelector<HTMLElement>("[data-row-main], [data-empty-add]")
      ?.focus();
  }

  onMount(() => {
    // The first open creates this webview, and tray.rs emits popover-shown right after show(),
    // which can be before the listener below exists. So focus after the first load too (R-27).
    store
      .init()
      .then(tick)
      .then(focusFirst)
      .then(() => {
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

  // Escape hides the popover; Up/Down/Home/End move between the rows (F12). Tab order inside a
  // row stays native: main, copy, open, switch.
  function navigationTarget(e: KeyboardEvent): number | null {
    if (!root) {
      return null;
    }
    const rows = [...root.querySelectorAll<HTMLElement>("[data-row-main]")];
    const current = document.activeElement
      ?.closest("[data-row]")
      ?.querySelector<HTMLElement>("[data-row-main]");
    return popoverRowIndex(
      e.key,
      current ? rows.indexOf(current) : -1,
      rows.length
    );
  }

  function navigate(e: KeyboardEvent) {
    const index = navigationTarget(e);
    if (index === null || index < 0) {
      return;
    }
    e.preventDefault();
    document.documentElement.dataset.kbNav = "";
    root?.querySelectorAll<HTMLElement>("[data-row-main]")[index]?.focus();
  }

  function onkeydown(e: KeyboardEvent) {
    const el = e.target as HTMLElement | null;
    if (el?.closest("input, textarea, select, [contenteditable]")) {
      return;
    }
    const action = popoverKeyAction(e);
    if (action === "hide") {
      api.hidePopover();
    }
    if (action === "navigate") {
      navigate(e);
    }
  }

  const groups = $derived(groupTunnels(store.tunnels));

  async function toggle(t: Tunnel, on: boolean) {
    busy[t.id] = true;
    try {
      if (on) {
        await api.start(t.id);
      } else {
        await api.stop(t.id);
      }
    } catch (e) {
      notify.error(
        e,
        on ? `Couldn't start ${t.name}` : `Couldn't stop ${t.name}`
      );
    } finally {
      busy[t.id] = false;
    }
  }

  const anyOn = $derived(store.tunnels.some(isOn));
  const summary = $derived(
    tunnelSummary(
      store.activeCount,
      store.tunnels.filter((t) => t.state.status === "starting").length
    )
  );

  const detail = "truncate text-code text-muted-foreground";
  // The popover's text buttons keep the label colour (the old .btn.ghost); icon buttons stay muted.
  const ghost = "text-callout text-foreground";
</script>

{#snippet tunnelGroups()}
  {#each groups as g (g.id)}
    <div class="not-first:mt-1.5" animate:flip={{ duration: motion(220) }}>
      {#if g.id !== null}
        <Tip text={g.name}>
          {#snippet children({
            props,
          })}
            <div
              {...{ ...props, tabindex: undefined }}
              class="truncate px-2 pt-1.5 pb-0.75 text-xs font-semibold text-muted-foreground"
            >
              {g.name}
            </div>
          {/snippet}
        </Tip>
      {/if}
      {#each g.tunnels as t (t.id)}
        <div
          class="group/row flex items-center gap-2.25 rounded-lg px-2 py-1.5 transition-colors duration-120 hover:bg-fill"
          data-row
          in:fly={{ duration: motion(200), y: -6 }}
          out:fade={{ duration: motion(120) }}
          animate:flip={{ duration: motion(220) }}
        >
          {@render tunnelRow(t)}
        </div>
      {/each}
    </div>
  {/each}
{/snippet}

{#snippet tunnelDescription(
  t: Tunnel,
  m: Tunnel["state"]["mappings"][number] | undefined
)}
  {#if t.separating_tunnel}
    <span class={detail}>Creating separate tunnel…</span>
  {:else if t.separate_tunnel_pending}
    <span class={detail}>Separate tunnel setup is unfinished</span>
  {:else if !isOn(t) && activeTunnelOwner(t, store.tunnels)}
    <span class={detail}
      >Not started:
      {activeTunnelOwner(t, store.tunnels)?.name}
      uses this tunnel</span
    >
  {:else if connectionWarning(t)}
    <span class={cn(detail, "text-degraded-text")}>{connectionWarning(t)}</span>
  {:else if (t.state.failure && !resolvedOriginFailure(t)) ||
    t.state.status === "crashed" ||
    t.state.status === "degraded"}
    <span class={cn(detail, t.state.status === "crashed" && "text-bad-text")}
      >{t.state.detail ?? statusLabel[t.state.status]}</span
    >
  {:else if m}
    <RouteLine
      compact
      hostname={t.kind === "quick"
        ? (t.state.public_url?.replace("https://", "") ?? null)
        : m.hostname}
      inspected={m.inspected}
      pulse={store.pulses[t.id] ?? 0}
      service={m.service}
    />
  {:else if t.kind === "token"}
    <span class={detail}>Hostnames come from the Cloudflare dashboard</span>
  {:else if t.kind === "quick"}
    <span class={cn(detail, "font-mono")}>{t.origin_url}</span>
  {/if}
{/snippet}

{#snippet tunnelRow(
  t: Tunnel
)}
  {@const m =
    t.state.mappings.find((m) => m.hostname && m.enabled !== false) ??
    t.state.mappings.find((m) => m.enabled !== false)}
  {@const url = publicUrl(t)}
  <!-- The status shows in words in the row when it matters, so the dot's hint is hover-only (no tab stop). -->
  <TunnelStatus status={t.state.status} />
  <button
    class={cn(
      "-mx-1 -my-0.5 flex min-w-0 flex-1 flex-col gap-px rounded-md px-1 py-0.5 text-left",
      kbRing
    )}
    data-row-main
    onclick={() => api.openDashboard(`tunnels:${t.id}`)}
    type="button"
  >
    <span class="truncate font-medium">{t.name}</span>
    {@render tunnelDescription(t, m)}
  </button>
  <!-- Shown on hover, and while a control inside has keyboard focus. -->
  <div
    class="flex opacity-0 transition-opacity duration-120 group-hover/row:opacity-100 focus-within:opacity-100"
  >
    {#if url && t.state.status === "connected"}
      <CopyButton
        tip="Copy public URL"
        value={url}
        what={url.replace("https://", "")}
      />
      <IconAction label="Open in browser" onclick={() => openUrl(url)}
        ><ExternalLink /></IconAction
      >
    {/if}
  </div>
  <!-- Busy while a start/stop is in flight: disabled, announced, and the thumb pulses. -->
  <Switch
    aria-busy={busy[t.id] || undefined}
    aria-label={isOn(t) ? `Stop ${t.name}` : `Start ${t.name}`}
    class={cn(busy[t.id] && "*:animate-pulse")}
    disabled={busy[t.id] ||
      t.separating_tunnel ||
      (!isOn(t) &&
        (t.setup_needed ||
          t.separate_tunnel_pending ||
          !!activeTunnelOwner(t, store.tunnels)))}
    bind:checked={() => isOn(t), (v) => toggle(t, v)}
  />
{/snippet}

<!-- The popover hides on blur (tray.rs); drop toasts so a stale one doesn't reappear. -->
<svelte:window onblur={() => notify.dismiss()} {onkeydown} />

<TooltipProvider delayDuration={250} ignoreNonKeyboardFocus>
  <!-- The radius must equal `radius: Some(12.0)` in tray.rs (the popover window's vibrancy corner).
     The tint sits on this root; html and body stay transparent (app.css). -->
  <main
    class="relative flex h-screen flex-col overflow-hidden rounded-[12px] bg-(--popover-tint)"
    bind:this={root}
  >
    <header class="flex items-center justify-between pt-3 pr-3 pb-2 pl-3.5">
      <div class="flex flex-col">
        <strong class="text-sm font-semibold">Underlane</strong>
        <span class="text-xs text-muted-foreground">{summary}</span>
      </div>
      {#if store.tunnels.length}
        {#if anyOn}
          <Button class={ghost} onclick={() => api.stopAll()} variant="ghost"
            >Stop all</Button
          >
        {:else}
          <Button class={ghost} onclick={() => api.startAll()} variant="ghost"
            >Start all</Button
          >
        {/if}
      {/if}
    </header>

    {#if store.overview && !store.overview.cloudflared}
      <AlertRoot class="mx-2.5 mb-1.5 w-auto" variant="warning">
        {#snippet icon()}
          <TriangleAlert />
        {/snippet}
        <AlertTitle>cloudflared isn't installed</AlertTitle>
        <AlertDescription
          >Run <code class="font-mono text-xs">brew install cloudflared</code>,
          then open Settings.</AlertDescription
        >
      </AlertRoot>
    {/if}

    <section class="flex-1 overflow-y-auto px-1.5 pb-1.5">
      {#if store.loading}
        <StatePanel kind="loading" title="Loading tunnels…" />
      {:else if store.error}
        <StatePanel
          kind="error"
          message={store.error}
          title="Couldn't load tunnels"
        >
          <Button onclick={() => store.refresh()} variant="outline"
            >Try again</Button
          >
        </StatePanel>
      {:else if store.tunnels.length === 0}
        <StatePanel
          kind="empty"
          message="Import a config.yml, paste a tunnel token, or share a local port with a quick tunnel."
          title="No tunnels yet"
        >
          <Button data-empty-add onclick={() => api.openDashboard("add")}
            >New</Button
          >
        </StatePanel>
      {:else}
        {@render tunnelGroups()}
      {/if}
    </section>

    <footer
      class="flex justify-between border-t-[0.5px] border-border px-2 pt-1.5 pb-2"
    >
      <Button
        class="text-foreground"
        onclick={() => api.openDashboard()}
        variant="ghost"
        ><PanelLeft class="size-3.5" />
        Open Dashboard</Button
      >
      <Tip side="top" text="Quit Underlane and stop all tunnels">
        {#snippet children({
          props,
        })}
          <Button
            {...mergeProps(props, { onclick: () => api.quit() })}
            class="text-foreground"
            variant="ghost"
            >Quit</Button
          >
        {/snippet}
      </Tip>
    </footer>
  </main>

  {#if toaster}
    {#await import("$lib/lazy") then { Toaster }}
      <!-- Above the footer; the window is 360px wide, so sonner uses mobileOffset (<= 600px). -->
      <Toaster
        mobileOffset={{ bottom: 52, left: 20, right: 20 }}
        offset={{ bottom: 52 }}
        position="bottom-center"
        theme={store.overview?.settings.theme ?? "system"}
        visibleToasts={1}
      />
    {/await}
  {/if}
</TooltipProvider>
