<script lang="ts">
  import List from "@lucide/svelte/icons/list";
  import ScrollText from "@lucide/svelte/icons/scroll-text";
  import SettingsIcon from "@lucide/svelte/icons/settings";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import Waypoints from "@lucide/svelte/icons/waypoints";
  import { listen } from "@tauri-apps/api/event";
  import { type Component, onMount, tick, untrack } from "svelte";
  import { fade } from "svelte/transition";
  import LaneGlyph from "$lib/components/app/LaneGlyph.svelte";
  import { Badge } from "$lib/components/ui/badge/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { kbRing } from "$lib/components/ui/mac";
  import { Toaster } from "$lib/components/ui/sonner/index.js";
  import { Provider as TooltipProvider } from "$lib/components/ui/tooltip/index.js";
  import { cn } from "$lib/utils";
  import { api, errorText } from "../lib/api";
  import { snapshot } from "../lib/snapshot.svelte";
  import { store } from "../lib/store.svelte";
  import { motion } from "../lib/theme";
  import AddSheet from "./AddSheet.svelte";
  import type { AddTab } from "./AddSheetBody.svelte";
  import Logs from "./Logs.svelte";
  import Onboarding from "./Onboarding.svelte";
  import Requests from "./Requests.svelte";
  import { track } from "./reactivity";
  import Settings from "./Settings.svelte";
  import Tunnels from "./Tunnels.svelte";

  type View = "tunnels" | "requests" | "logs" | "settings";
  let view = $state<View>("tunnels");
  let onboarding = $state<boolean | null>(null);
  let onboardingError = $state<string | null>(null);
  let existingAuthorization = $state(false);
  let onboardingRequest = 0;
  let alive = true;
  let focusId = $state<string | null>(null);
  let adding = $state(false);
  let addTab = $state<AddTab>("project");
  // The button that opened the sheet, for return focus (02 R7). Null when the popover opened it.
  // Bumped each time the Add sheet opens, so a slow add can't close a sheet reopened meanwhile.
  let sheetSession = $state(0);
  $effect.pre(() => {
    track(addTab);
    if (adding) {
      untrack(() => {
        sheetSession += 1;
      });
    }
  });
  let addTrigger = $state<HTMLElement | null>(null);
  // The project or quick tunnel the sheet just created, for return focus.
  let addedId: string | null = null;
  let requestsRef = $state<{
    selectFirst: (side?: "req" | "resp") => Promise<void>;
  } | null>(null);

  function navigate(target: string) {
    if (onboarding !== false) {
      return;
    }
    const [v, id] = (target.startsWith("#") ? target.slice(1) : target).split(
      ":"
    );
    if (v === "add") {
      // add:<tab> opens a given tab (O6), e.g. for screenshots.
      addTab = id === "quick" ? "quick" : "project";
      addTrigger = null;
      addedId = null;
      view = "tunnels";
      adding = true;
      return;
    }
    adding = false;
    const next = ["tunnels", "requests", "logs", "settings"].includes(v)
      ? (v as View)
      : view;
    show(next, () => {
      view = next;
      focusId = id || null;
    });
  }

  // A view switch crossfades the content pane (app.css): WebKit snapshots the old view, so the
  // two are never mounted together. The update runs a frame later, so e2e snapshots, which
  // read the view right after navigating, switch at once.
  function show(next: View, update: () => void) {
    if (next === view || snapshot.full || !document.startViewTransition) {
      return update();
    }
    document.startViewTransition(async () => {
      update();
      await tick();
    });
  }

  // The e2e harness asks for the rendered text of a view. Lists render every row meanwhile
  // (snapshot mode, 02 §4.3), so the text isn't windowed. The control server takes concurrent
  // connections, so requests queue here: one snapshot's `finally` can't end snapshot mode while
  // another is still reading.
  interface SnapshotRequest {
    nonce: string;
    select?: string;
    tab?: string;
    view: string;
  }
  let snapshots = Promise.resolve();
  async function e2eSnapshot(p: SnapshotRequest) {
    snapshot.full = true;
    try {
      navigate(p.view);
      await tick();
      if (p.select === "first" && requestsRef) {
        await requestsRef.selectFirst(p.tab === "req" ? "req" : "resp");
      }
      await new Promise((r) => setTimeout(r, 400));
      await api.e2eReport(p.nonce, document.body.innerText);
    } finally {
      snapshot.full = false;
    }
  }

  // O9: mark the window inactive while it isn't key, so app.css greys the selection and the
  // default button as AppKit does. WebKit fires window blur/focus when the NSWindow resigns or
  // becomes key; Tauri's onFocusChanged would cost ~3 KB of dashboard JS and grow the popover.
  const inactive = (on: boolean) =>
    document.documentElement.toggleAttribute("data-inactive", on);

  onMount(() => {
    inactive(!document.hasFocus());
    store.init();
    loadOnboarding();
    const subs = [
      listen("app-reset", () => {
        adding = false;
        sheetSession += 1;
        addTrigger = null;
        addedId = null;
        focusId = null;
        view = "tunnels";
        existingAuthorization = false;
        onboarding = null;
        history.replaceState(null, "", location.pathname + location.search);
        loadOnboarding();
      }),
      listen<string>("navigate", (e) => navigate(e.payload)),
      listen<SnapshotRequest>("e2e-snapshot", (e) => {
        snapshots = snapshots
          .then(() => e2eSnapshot(e.payload))
          .catch((err) => console.error("e2e snapshot", err));
      }),
    ];
    return () => {
      alive = false;
      for (const subscription of subs) {
        subscription.then((unsubscribe) => unsubscribe());
      }
    };
  });

  async function loadOnboarding() {
    onboardingRequest += 1;
    const request = onboardingRequest;
    onboardingError = null;
    try {
      const status = await api.onboardingStatus();
      if (!alive || request !== onboardingRequest) {
        return;
      }
      existingAuthorization = status.account_connected;
      onboarding = !status.complete;
      if (status.complete && location.hash) {
        navigate(location.hash);
      }
    } catch (e) {
      if (alive && request === onboardingRequest) {
        onboardingError = errorText(e);
      }
    }
  }

  async function finishOnboarding(mode: "project" | null, id?: string) {
    await api.completeOnboarding();
    await store.refresh();
    if (store.error) {
      throw new Error(store.error);
    }
    onboarding = false;
    focusId = id ?? null;
    view = "tunnels";
    await tick();
    if (mode === "project") {
      navigate("add:project");
    } else {
      document.querySelector<HTMLElement>("[data-add-tunnel]")?.focus();
    }
  }

  function openAdd(e: MouseEvent) {
    addTrigger = e.currentTarget as HTMLElement;
    addedId = null;
    addTab = "project";
    adding = true;
  }

  // Freeze the opening that owns a create request before its backend write begins. A later
  // popover navigation or reopened sheet must never be selected/closed by the old request.
  function afterCreation(session: number) {
    return async function finishCreatedProject(id: string) {
      if (session !== sheetSession || !adding) {
        return;
      }
      await store.refresh();
      if (session !== sheetSession || !adding) {
        return;
      }
      if (store.error) {
        throw new Error(store.error);
      }
      addTrigger = null;
      addedId = id;
      focusId = id;
      view = "tunnels";
      await tick();
      if (session === sheetSession && adding) {
        adding = false;
      }
    };
  }

  const nav: {
    id: View;
    label: string;
    icon: Component<{ class?: string }>;
  }[] = [
    { icon: Waypoints, id: "tunnels", label: "Projects" },
    { icon: List, id: "requests", label: "Requests" },
    { icon: ScrollText, id: "logs", label: "Logs" },
    { icon: SettingsIcon, id: "settings", label: "Settings" },
  ];
</script>
{#snippet sidebarNavigation()}
  <nav aria-label="Sections" class="relative isolate flex flex-col gap-1">
    <!-- The selection is one pill that slides to the current item, like the segmented
           control's thumb. Items are h-8 with a 4px gap, so item i sits i × (100% + 4px) down. -->
    <div
      aria-hidden="true"
      class="pointer-events-none absolute inset-x-0 top-0 -z-1 h-8 rounded-md bg-sidebar-accent transition-[translate] duration-200 ease-out motion-reduce:transition-none"
      style:translate="0 calc({nav.findIndex(
        (n) => n.id === view
      )} * (100% + 4px))"
    ></div>
    {#each nav as n}
      <button
        aria-current={view === n.id ? "page" : undefined}
        class={cn(
          "flex h-8 items-center gap-2.5 rounded-md px-2.5 text-left transition-colors",
          view === n.id
            ? "text-foreground"
            : "text-muted-foreground hover:bg-fill hover:text-foreground",
          kbRing
        )}
        onclick={() =>
          show(n.id, () => {
            view = n.id;
            focusId = null;
          })}
        type="button"
      >
        <n.icon class="size-[15px]" />
        <span class="flex-1">{n.label}</span>
        {#if n.id === "tunnels" && store.activeCount > 0}
          <span class="flex" transition:fade={{ duration: motion(150) }}
            ><Badge variant="ok">{store.activeCount}</Badge></span
          >
        {/if}
      </button>
    {/each}
  </nav>
{/snippet}

{#snippet cloudflaredStatus()}
  <div class="mt-auto px-2 text-xs">
    {#if store.overview?.cloudflared}
      <span class="text-muted-foreground"
        >cloudflared {store.overview.cloudflared.version ?? ""}</span
      >
    {:else if store.overview}
      <button
        class={cn(
          "flex items-center gap-1 rounded-xs text-degraded-text",
          kbRing
        )}
        onclick={() =>
          show("settings", () => {
            view = "settings";
          })}
        type="button"
      >
        <TriangleAlert class="size-3" />
        cloudflared not found
      </button>
    {/if}
  </div>
{/snippet}

<svelte:window onblur={() => inactive(true)} onfocus={() => inactive(false)} />

<!-- One tooltip provider and one toaster per window (02 §1.4, §2). -->
<TooltipProvider delayDuration={250} ignoreNonKeyboardFocus>
  {#if onboarding === null}
    <main class="flex h-screen items-center justify-center bg-background">
      <div class="max-w-md p-8 text-center">
        {#if onboardingError}
          <p class="mb-4 text-callout text-destructive" role="alert">
            {onboardingError}
          </p>
          <Button onclick={loadOnboarding}>Try again</Button>
        {:else}
          <p class="text-callout text-muted-foreground" role="status">
            Opening Underlane…
          </p>
        {/if}
      </div>
    </main>
  {:else if onboarding}
    <Onboarding {existingAuthorization} oncomplete={finishOnboarding} />
  {:else}
    <!-- Neutral sidebar and content surfaces share the same light/dark palette. -->
    <div class="grid h-screen grid-cols-[208px_1fr]">
      <aside
        class="flex flex-col border-e-[0.5px] border-border bg-sidebar px-2.5 pb-2.5"
        data-tauri-drag-region
      >
        <!-- Room for the traffic lights: the dashboard has an overlay title bar
         (TitleBarStyle::Overlay in src-tauri/src/tray.rs), so the webview starts at y=0. -->
        <div class="h-[46px] flex-none" data-tauri-drag-region></div>
        <div
          class="flex items-center gap-2 px-2 pb-4 text-lg font-semibold"
          data-tauri-drag-region
        >
          <LaneGlyph class="pointer-events-none text-foreground" size={20} />
          <span>Underlane</span>
        </div>
        {@render sidebarNavigation()}
        {@render cloudflaredStatus()}
      </aside>

      <main
        class="min-w-0 overflow-hidden bg-background [view-transition-name:page]"
      >
        {#if view === "tunnels"}
          <Tunnels {focusId} onadd={openAdd} />
        {:else if view === "requests"}
          <Requests {focusId} bind:this={requestsRef} />
        {:else if view === "logs"}
          <Logs {focusId} />
        {:else}
          <Settings />
        {/if}
      </main>
    </div>

    <!-- After adding one tunnel, focus goes to its row in the Tunnels list (it's selected through
     focusId). Otherwise, without a live trigger (opened from the popover, or the empty state went
     away after creation), focus goes to the Tunnels toolbar's New button. -->
    <AddSheet
      onadded={afterCreation(sheetSession)}
      returnFocus={() => {
        const row = addedId
          ? document.querySelector<HTMLElement>(`[data-tunnel-id="${addedId}"]`)
          : null;
        // Focus is restored with preventScroll, so bring a new row that's below the fold into view.
        row?.scrollIntoView({ block: "nearest" });
        return (
          row ||
          (addTrigger?.isConnected
            ? addTrigger
            : document.querySelector<HTMLElement>("[data-add-tunnel]"))
        );
      }}
      tab={addTab}
      bind:open={adding}
    />
  {/if}

  <Toaster
    position="bottom-right"
    theme={store.overview?.settings.theme ?? "system"}
    visibleToasts={3}
  />
</TooltipProvider>
