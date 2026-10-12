<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { store } from "../lib/store.svelte";
  import { api, errorText } from "../lib/api";
  import type { LogLine } from "../lib/types";
  import SearchIcon from "@lucide/svelte/icons/search";
  import { Button } from "$lib/components/ui/button/index.js";
  import * as InputGroup from "$lib/components/ui/input-group/index.js";
  import * as ToggleGroup from "$lib/components/ui/toggle-group/index.js";
  import Toolbar from "$lib/components/app/Toolbar.svelte";
  import StatePanel from "$lib/components/app/StatePanel.svelte";
  import TunnelPicker from "./TunnelPicker.svelte";
  import LogList from "./LogList.svelte";

  let { focusId = null }: { focusId?: string | null } = $props();
  let tunnelId = $state<string | null>(null);
  $effect(() => {
    if (focusId) tunnelId = focusId;
  });
  const LEVELS = ["debug", "info", "warn", "error"] as const;
  // A severity ladder for the filter's dots; shown labels take the level's text colour.
  const levelDot: Record<(typeof LEVELS)[number], string> = { debug: "var(--text-3)", info: "var(--info)", warn: "var(--degraded)", error: "var(--bad)" };
  const levelText: Partial<Record<(typeof LEVELS)[number], string>> = { info: "data-[state=on]:text-info-text", warn: "data-[state=on]:text-degraded-text", error: "data-[state=on]:text-bad-text" };
  let levels = $state<Record<string, boolean>>({ debug: true, info: true, warn: true, error: true });
  let search = $state("");
  let lines = $state<LogLine[]>([]);
  /** Only the first load shows the loading panel; later reloads keep the current view until they land. */
  let loading = $state(true);
  let error = $state<string | null>(null);
  let list = $state<LogList>();
  const MAX = 3000;

  const activeLevels = $derived(LEVELS.filter((l) => levels[l]).flatMap((l) => (l === "error" ? ["error", "fatal"] : [l])));
  const names = $derived(Object.fromEntries(store.tunnels.map((t) => [t.id, t.name])));

  let loadGen = 0;
  let loadKey: string | undefined;
  let inflight: Promise<void> | null = null;
  function load(): Promise<void> {
    // Each change of tunnel, level or search (and Clear) starts a new generation, so a slow
    // answer (or error) for an older one never lands, even after A -> B -> A. A load for the same
    // generation while one is in flight shares it (as in Requests).
    const lv = activeLevels;
    const key = `${tunnelId}\u0000${lv.join()}\u0000${search}`;
    if (key === loadKey && inflight) return inflight;
    if (key !== loadKey) {
      loadGen++;
      loadKey = key;
    }
    const g = loadGen;
    const current = () => g === loadGen;
    error = null;
    if (lv.length === 0) {
      lines = [];
      loading = false;
      return Promise.resolve();
    }
    const q = { tunnelId, levels: lv.length === 5 ? [] : lv, search, limit: 1000 };
    const run = (async () => {
      try {
        const r = await api.queryLogs(q);
        if (current()) {
          lines = r;
          list?.reset();
        }
      } catch (e) {
        if (current()) error = errorText(e);
      } finally {
        if (current()) loading = false;
      }
    })();
    inflight = run;
    run.finally(() => {
      if (inflight === run) inflight = null;
    });
    return run;
  }

  $effect(() => {
    void tunnelId;
    void search;
    void activeLevels.join();
    const t = setTimeout(load, search ? 180 : 0);
    return () => clearTimeout(t);
  });

  function matches(l: LogLine) {
    if (tunnelId && l.tunnel_id !== tunnelId) return false;
    if (!activeLevels.includes(l.level)) return false;
    if (search && !(l.message + l.fields).toLowerCase().includes(search.toLowerCase())) return false;
    return true;
  }

  onMount(() => {
    const un = listen<LogLine[]>("logs", (e) => {
      // Rows can already be in the snapshot loaded a moment ago; only append newer ids.
      const last = lines.length ? lines[lines.length - 1].id : 0;
      const add = e.payload.filter((l) => l.id > last && matches(l));
      if (!add.length) return;
      // The list caps and anchors (it defers the head trim while scrolled up); with no list
      // mounted (empty state) there's no view to keep.
      if (list) list.append(add);
      else lines = [...lines, ...add].slice(-MAX);
    });
    return () => un.then((f) => f());
  });

  async function clear() {
    // A load still in flight answers from before the Clear: drop it.
    loadGen++;
    loadKey = undefined;
    inflight = null;
    loading = false;
    await api.clearLogs(tunnelId);
    lines = [];
  }
</script>

<div class="flex h-full flex-col">
  <Toolbar title="Logs">
    <TunnelPicker bind:value={tunnelId} allowAll />
    <!-- Multiple selection: any set of levels, including none. -->
    <ToggleGroup.Root
      type="multiple"
      aria-label="Levels"
      bind:value={
        () => LEVELS.filter((l) => levels[l]),
        (v: string[]) => {
          for (const l of LEVELS) levels[l] = v.includes(l);
        }
      }
    >
      <!-- No per-item chips (four lit boxes in dark mode): a shown level has a filled
           severity dot and full-strength label, a hidden one a hollow ring and a muted label. -->
      {#each LEVELS as l (l)}
        <ToggleGroup.Item
          value={l}
          style="--lvl: {levelDot[l]}"
          class="group-data-[spacing=0]/toggle-group:hover:bg-fill group-data-[spacing=0]/toggle-group:data-[state=on]:border-transparent group-data-[spacing=0]/toggle-group:data-[state=on]:bg-transparent gap-1.5 text-muted-foreground data-[state=on]:text-foreground {levelText[l]}"
        >
          <span
            aria-hidden="true"
            class="size-1.5 rounded-full shadow-[inset_0_0_0_1px_var(--text-3)] transition-[background-color,box-shadow] duration-150 group-data-[state=on]/toggle:bg-(--lvl) group-data-[state=on]/toggle:shadow-none motion-reduce:transition-none"
          ></span>{l}
        </ToggleGroup.Item>
      {/each}
    </ToggleGroup.Root>
    <InputGroup.Root class="w-44">
      <InputGroup.Addon><SearchIcon /></InputGroup.Addon>
      <InputGroup.Input placeholder="Search" bind:value={search} aria-label="Search logs" autocorrect="off" autocapitalize="off" spellcheck="false" />
    </InputGroup.Root>
    <Button variant="outline" onclick={clear} disabled={!lines.length}>Clear</Button>
  </Toolbar>

  <div class="flex min-h-0 flex-1 flex-col">
    {#if loading && lines.length === 0}
      <StatePanel kind="loading" title="Loading logs…" />
    {:else if error}
      <StatePanel kind="error" title="Couldn't load logs" message={error}><Button variant="outline" onclick={load}>Try again</Button></StatePanel>
    {:else if lines.length === 0}
      <StatePanel kind="empty" title={search || activeLevels.length < 5 ? "No matching log lines" : "No logs yet"} message={search || activeLevels.length < 5 ? "Try a different search or level filter." : "Start a tunnel and cloudflared's output streams here live."} />
    {:else}
      <LogList bind:this={list} bind:lines {names} showTunnel={!tunnelId} cap={MAX} />
    {/if}
  </div>
</div>
