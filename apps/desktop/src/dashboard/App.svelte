<script lang="ts">
  import List from "@lucide/svelte/icons/list";
  import ScrollText from "@lucide/svelte/icons/scroll-text";
  import SettingsIcon from "@lucide/svelte/icons/settings";
  import { listen } from "@tauri-apps/api/event";
  import { type Component, onMount, tick, untrack } from "svelte";
  import LaneGlyph from "$lib/components/app/LaneGlyph.svelte";
  import { Button } from "$lib/components/ui/button/index.js";
  import { kbRing } from "$lib/components/ui/mac";
  import { Toaster } from "$lib/components/ui/sonner/index.js";
  import { Provider as TooltipProvider } from "$lib/components/ui/tooltip/index.js";
  import { groupTunnels } from "$lib/tunnel-groups";
  import { cn } from "$lib/utils";
  import { version as appVersion } from "../../src-tauri/tauri.conf.json";
  import { api, errorText } from "../lib/api";
  import { snapshot } from "../lib/snapshot.svelte";
  import { store } from "../lib/store.svelte";
  import AddSheet from "./AddSheet.svelte";
  import type { AddTab } from "./AddSheetBody.svelte";
  import GroupSheet from "./GroupSheet.svelte";
  import Logs from "./Logs.svelte";
  import Onboarding from "./Onboarding.svelte";
  import ProjectSidebar from "./ProjectSidebar.svelte";
  import Requests from "./Requests.svelte";
  import { track } from "./reactivity";
  import Settings from "./Settings.svelte";
  import Tunnels from "./Tunnels.svelte";
  import UpdateButton from "./UpdateButton.svelte";

  type View = "tunnels" | "requests" | "logs" | "settings";
  let view = $state<View>("tunnels");
  let selectedId = $state<string | null>(null);
  let collapsedGroups = $state<string[]>(loadCollapsedGroups());
  let groupOpen = $state(false);
  let editingGroup = $state<{ id: string; name: string } | null>(null);
  let groupTrigger: HTMLElement | null = null;
  let addGroup = $state("");
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
    track(addGroup);
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

  function loadCollapsedGroups(): string[] {
    try {
      const value: unknown = JSON.parse(
        localStorage.getItem("underlane.projects.collapsed-groups") ?? "[]"
      );
      return Array.isArray(value)
        ? value.filter((id): id is string => typeof id === "string")
        : [];
    } catch {
      return [];
    }
  }

  $effect(() => {
    try {
      if (collapsedGroups.length) {
        localStorage.setItem(
          "underlane.projects.collapsed-groups",
          JSON.stringify(collapsedGroups)
        );
      } else {
        localStorage.removeItem("underlane.projects.collapsed-groups");
      }
    } catch {
      // Group navigation still works when browser storage is unavailable.
    }
  });

  $effect(() => {
    if (
      store.overview &&
      !store.tunnels.some((tunnel) => tunnel.id === selectedId)
    ) {
      selectedId = store.tunnels[0]?.id ?? null;
    }
  });

  function selectProject(id: string) {
    show("tunnels", () => {
      selectedId = id;
      focusId = null;
      view = "tunnels";
    });
  }

  function editGroup(
    trigger: HTMLElement,
    group?: { id: string; name: string }
  ) {
    groupTrigger = trigger;
    editingGroup = group ?? null;
    groupOpen = true;
  }

  async function removeProject(id: string, deleteConfig: boolean) {
    const order = groupTunnels(
      store.tunnels,
      store.overview?.groups ?? []
    ).flatMap((group) => group.tunnels);
    const index = order.findIndex((tunnel) => tunnel.id === id);
    const next = order[index + 1] ?? order[index - 1] ?? null;
    await api.deleteTunnel(id, deleteConfig);
    selectedId = next?.id ?? null;
    if (next) {
      collapsedGroups = collapsedGroups.filter(
        (group) => group !== (next.group_id ?? "ungrouped")
      );
    }
    await store.refresh();
    await tick();
    const row = next
      ? document.querySelector<HTMLElement>(`[data-tunnel-id="${next.id}"]`)
      : null;
    row?.scrollIntoView({ block: "nearest" });
    (row ?? document.querySelector<HTMLElement>("[data-add-tunnel]"))?.focus();
  }

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
      addGroup = "";
      addTrigger = null;
      addedId = null;
      view = "tunnels";
      adding = true;
      return;
    }
    adding = false;
    if (id && v === "tunnels") {
      show("tunnels", () => {
        selectedId = id;
        const tunnel = store.tunnels.find((item) => item.id === id);
        collapsedGroups = collapsedGroups.filter(
          (group) => group !== (tunnel?.group_id ?? "ungrouped")
        );
        view = "tunnels";
        focusId = null;
      });
      return;
    }
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
    action?: "start" | "stop";
    hostname?: string;
    inspect?: boolean;
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
      if (p.action && p.hostname) {
        const row = [
          ...document.querySelectorAll<HTMLElement>("[data-route-hostname]"),
        ].find((element) => element.dataset.routeHostname === p.hostname);
        const label = `${p.action === "start" ? "Start" : "Stop"} ${p.hostname}`;
        const button = [
          ...(row?.querySelectorAll<HTMLButtonElement>("button") ?? []),
        ].find((element) => element.getAttribute("aria-label") === label);
        if (!button || button.getAttribute("aria-disabled") === "true") {
          throw new Error(`Hostname action unavailable: ${label}`);
        }
        button.click();
        await new Promise((r) => setTimeout(r, 400));
        await tick();
      }
      const selected = store.tunnels.find((tunnel) => tunnel.id === selectedId);
      const sidebar = [
        ...document.querySelectorAll<HTMLElement>("[data-tunnel-id]"),
      ].find((element) => element.dataset.tunnelId === selectedId);
      await api.e2eReport(
        p.nonce,
        p.inspect
          ? JSON.stringify({
              headerStatus: document
                .querySelector('header [role="img"]')
                ?.getAttribute("aria-label"),
              rows: [
                ...document.querySelectorAll<HTMLElement>(
                  "[data-route-hostname]"
                ),
              ].map((row) => ({
                actions: [...row.querySelectorAll("button")].map((button) =>
                  button.getAttribute("aria-label")
                ),
                dots: [
                  ...row.querySelectorAll<HTMLElement>('[role="img"]'),
                ].map((dot) => ({
                  label: dot.getAttribute("aria-label"),
                  visible: !dot.classList.contains("invisible"),
                })),
                hostname: row.dataset.routeHostname,
              })),
              sidebar: sidebar?.getAttribute("aria-label"),
              sidebarStatus: sidebar
                ?.querySelector('[role="img"]')
                ?.getAttribute("aria-label"),
              state: selected?.state,
              text: document.body.innerText,
            })
          : document.body.innerText
      );
    } catch (error) {
      await api.e2eReport(p.nonce, JSON.stringify({ error: errorText(error) }));
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
        selectedId = null;
        collapsedGroups = [];
        groupOpen = false;
        addGroup = "";
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
    selectedId = id ?? null;
    view = "tunnels";
    await tick();
    if (mode === "project") {
      navigate("add:project");
    } else {
      document.querySelector<HTMLElement>("[data-add-tunnel]")?.focus();
    }
  }

  function openAdd(e: MouseEvent | HTMLElement, group = "") {
    addTrigger =
      e instanceof HTMLElement ? e : (e.currentTarget as HTMLElement);
    addedId = null;
    addTab = "project";
    addGroup = group;
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
      selectedId = id;
      const created = store.tunnels.find((tunnel) => tunnel.id === id);
      collapsedGroups = collapsedGroups.filter(
        (group) => group !== (created?.group_id ?? "ungrouped")
      );
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
    { icon: List, id: "requests", label: "Requests" },
    { icon: ScrollText, id: "logs", label: "Logs" },
    { icon: SettingsIcon, id: "settings", label: "Settings" },
  ];
</script>
{#snippet sidebarNavigation()}
  <nav aria-label="Sections" class="relative isolate flex flex-col gap-1">
    {#each nav as n}
      <button
        aria-current={view === n.id ? "page" : undefined}
        class={cn(
          "flex h-8 items-center gap-2.5 rounded-md px-2.5 text-left transition-colors",
          view === n.id
            ? "bg-sidebar-accent text-foreground"
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
      </button>
    {/each}
  </nav>
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
    <div class="grid h-screen grid-cols-[224px_minmax(0,1fr)]">
      <aside
        class="group/sidebar flex min-h-0 flex-col border-e-[0.5px] border-border bg-sidebar px-2.5 pb-2.5"
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
        <ProjectSidebar
          active={view === "tunnels"}
          onadd={openAdd}
          ongroup={editGroup}
          onselect={selectProject}
          {selectedId}
          bind:collapsed={collapsedGroups}
        />
        <div class="mt-auto flex flex-col gap-3 pt-4">
          {@render sidebarNavigation()}
          <UpdateButton />
          <div class="px-2 text-xs text-muted-foreground">
            Underlane {appVersion}
          </div>
        </div>
      </aside>

      <main
        class="min-w-0 overflow-hidden bg-window-background [view-transition-name:page]"
      >
        {#if view === "tunnels"}
          <Tunnels onadd={openAdd} onremove={removeProject} {selectedId} />
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
      group={addGroup}
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
    <GroupSheet
      group={editingGroup}
      onsaved={(id) => {
        collapsedGroups = collapsedGroups.filter((group) => group !== id);
      }}
      returnFocus={() =>
        groupTrigger?.isConnected
          ? groupTrigger
          : document.querySelector<HTMLElement>("[data-add-tunnel]")}
      bind:open={groupOpen}
    />
  {/if}

  <Toaster
    position="bottom-right"
    theme={store.overview?.settings.theme ?? "system"}
    visibleToasts={3}
  />
</TooltipProvider>
