<script lang="ts">
  // The real Requests list (src/dashboard/RequestList.svelte: listbox, VList, rows, keys,
  // anchoring) on 500 fake requests, for 03 R-49. The parent part mirrors Requests.svelte:
  // selection by id, a "detail" line that follows it, and new requests prepended through
  // RequestList.prepend(). window.__reqlab lets Playwright drive it.
  import { tick } from "svelte";
  import { Button } from "$lib/components/ui/button/index.js";
  import RequestList, { columns } from "../dashboard/RequestList.svelte";
  import type { RequestSummary } from "$lib/types";

  const CAP = 500;
  const methods = ["GET", "POST", "PUT", "DELETE"];
  let seq = 0;
  const mk = (): RequestSummary => {
    const n = seq++;
    return {
      id: `r${n}`,
      tunnel_id: "lab",
      ts: Date.UTC(2026, 9, 11, 12, 0, 0) + n * 1000,
      duration_ms: 20 + (n % 90),
      kind: "http",
      method: methods[n % 4],
      host: "lab.trycloudflare.com",
      path: `/api/items/${n}?page=${n % 7}`,
      status: n % 13 === 0 ? 500 : n % 11 === 0 ? 404 : 200,
      resp_size: 100 + n * 7,
      error: null,
    };
  };
  const seed = () => Array.from({ length: CAP }, mk).reverse();

  let rows = $state<RequestSummary[]>(seed());
  let selected = $state<string | null>(null);
  let list = $state<RequestList>();
  const detail = $derived(rows.find((r) => r.id === selected) ?? null);

  /** n requests arriving as separate events in the same task, like a burst of real ones. */
  async function add(n: number) {
    for (let i = 0; i < n; i++) list?.add(mk());
    await tick();
    await new Promise((r) => setTimeout(r, 0));
  }

  if (typeof window !== "undefined") {
    (window as unknown as { __reqlab: unknown }).__reqlab = {
      add,
      rows: () => rows.map((r) => r.id),
      selected: () => selected,
      detail: () => detail?.id ?? null,
      select: async (id: string | null) => {
        selected = id;
        await tick();
      },
      filter: async (keep: (id: string) => boolean) => {
        rows = rows.filter((r) => keep(r.id));
        await tick();
      },
      reset: async () => {
        seq = 0;
        rows = seed();
        selected = null;
        await tick();
      },
    };
  }
</script>

<div class="flex flex-col gap-2">
  <div class="text-callout text-muted-foreground" data-testid="reqlab-detail">
    {rows.length} rows · selected {selected ?? "none"} · detail {detail ? `${detail.method} ${detail.path}` : "none"}
  </div>
  <div class="flex h-[320px] w-[560px] flex-col overflow-hidden rounded-lg bg-card shadow-(--shadow)" data-testid="reqlab">
    <div class="{columns} h-7 flex-none border-b-[0.5px] border-border text-xs text-muted-foreground" aria-hidden="true">
      <span>Method</span><span>Path</span><span class="text-right">Status</span><span class="text-right">Time</span><span class="text-right">Size</span><span class="text-right">At</span>
    </div>
    <RequestList bind:this={list} bind:rows {selected} cap={CAP} onselect={(id) => (selected = id)} />
  </div>
  <div class="flex flex-wrap items-center gap-2">
    <Button variant="outline" size="sm" onclick={() => add(1)}>New request</Button>
    <Button variant="outline" size="sm" onclick={() => add(5)}>5 new requests</Button>
  </div>
</div>
