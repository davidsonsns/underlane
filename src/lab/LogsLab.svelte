<script lang="ts">
  // The real Logs list (src/dashboard/LogList.svelte: treegrid, VList, rows, keys, expansion,
  // follow, deferred head trim) on 3,000 fake lines, for 03 R-49. Every 7th line wraps, every
  // 4th has fields (expandable). The parent part mirrors Logs.svelte: live lines go through
  // LogList.append(). window.__loglab lets Playwright drive it.
  import { tick } from "svelte";
  import { Button } from "$lib/components/ui/button/index.js";
  import LogList from "../dashboard/LogList.svelte";
  import { snapshot } from "$lib/snapshot.svelte";
  import type { LogLine } from "$lib/types";

  const CAP = 3000;
  const words = "edge connection registered origin request served tunnel metrics quic heartbeat".split(" ");
  const levels: LogLine["level"][] = ["debug", "info", "info", "info", "warn", "error", "info", "fatal"];
  let seq = 1;
  const mk = (): LogLine => {
    const n = seq++;
    return {
      id: n,
      tunnel_id: n % 3 ? "lab" : "lab2",
      ts: Date.UTC(2026, 9, 11, 12, 0, 0) + n * 250,
      level: levels[n % levels.length],
      message: `${n}: ` + Array.from({ length: n % 7 === 0 ? 40 + (n % 3) * 20 : 6 + (n % 5) }, (_, i) => words[(n + i) % words.length]).join(" "),
      fields: n % 4 === 0 ? JSON.stringify({ connIndex: n % 4, event: "line", location: `lab${n % 9}`, ip: "198.41.200.13", detail: "x".repeat(n % 60) }) : "{}",
    };
  };
  const seed = () => Array.from({ length: CAP }, mk);
  /** A different result set of the same length (a filter change): short lines, no fields. */
  const plain = (): LogLine => {
    const l = mk();
    return { ...l, message: `${l.id}: plain line`, fields: "{}" };
  };

  let lines = $state<LogLine[]>(seed());
  let list = $state<LogList>();
  let all = $state(true);

  /** n lines arriving in one "logs" event, like a batch from the backend. */
  async function append(n: number) {
    const add = Array.from({ length: n }, mk);
    if (list) list.append(add);
    else lines = [...lines, ...add].slice(-CAP);
    await tick();
  }

  if (typeof window !== "undefined") {
    (window as unknown as { __loglab: unknown }).__loglab = {
      append,
      ids: () => lines.map((l) => l.id),
      /** E2E snapshot mode (every row rendered), as App.svelte's e2e handler sets it. */
      snapshot: async (full: boolean) => {
        snapshot.full = full;
        await tick();
      },
      openCount: () => list?.lab?.openCount() ?? 0,
      sizes: () => list?.lab?.sizes() ?? [],
      /** A tunnel/level/search change that returns the same number of lines, as Logs.load() does. */
      refilter: async () => {
        seq = 100000;
        lines = Array.from({ length: CAP }, plain);
        list?.reset();
        await tick();
      },
      reset: async () => {
        seq = 1;
        lines = seed();
        await tick();
        list?.reset();
        await tick();
      },
    };
  }
</script>

<div class="flex flex-col gap-2">
  <div class="text-callout text-muted-foreground" data-testid="loglab-info">{lines.length} lines · newest {lines.at(-1)?.id ?? "none"}</div>
  <div class="flex h-[360px] w-[640px] flex-col overflow-hidden rounded-lg bg-card shadow-(--shadow)" data-testid="loglab">
    {#if lines.length}
      <LogList bind:this={list} bind:lines names={{ lab: "lab", lab2: "lab-two" }} showTunnel={all} cap={CAP} />
    {:else}
      <div class="grid flex-1 place-items-center text-callout text-muted-foreground">No logs yet</div>
    {/if}
  </div>
  <div class="flex flex-wrap items-center gap-2">
    <Button variant="outline" size="sm" onclick={() => append(1)}>New line</Button>
    <Button variant="outline" size="sm" onclick={() => append(25)}>25 new lines</Button>
    <Button variant="outline" size="sm" onclick={() => (all = !all)}>Toggle tunnel column</Button>
  </div>
</div>
