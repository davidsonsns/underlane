<script lang="ts">
  // The real Requests list (src/dashboard/RequestList.svelte: listbox, VList, rows, keys,
  // anchoring) on 500 fake requests, for 03 R-49. The parent part mirrors Requests.svelte:
  // selection by id, a "detail" line that follows it, the list unmounted (and the selection
  // cleared) when it's empty, the detail's tab and body view kept across selections, and new
  // requests queued through RequestList.add(). window.__reqlab lets Playwright drive it.
  import { tick, untrack } from "svelte";
  import { Button } from "$lib/components/ui/button/index.js";
  import RequestList, { columns } from "../dashboard/RequestList.svelte";
  import RequestDetail from "../dashboard/RequestDetail.svelte";
  import type { BodyView, RequestDetail as Detail, RequestSummary } from "$lib/types";

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
  let side = $state<"req" | "resp">("resp");
  let mode = $state<"pretty" | "raw" | "hex">("pretty");
  $effect(() => {
    if (rows.length === 0 && selected != null) untrack(() => (selected = null));
  });

  // A fake detail for the selected row: a long JSON response (scrolls inside its tab panel), a
  // truncated request body (warning badge + the curl Tip), and redacted / long headers. Request
  // n % 5 == 1 answers plain text (no Pretty) and n % 5 == 2 a binary body (Hex only).
  const view = (text: string, extra: Partial<BodyView> = {}): BodyView => ({
    size: text.length, stored: text.length, truncated: false, content_type: "application/json", decoded_from: null,
    decode_partial: false, is_text: true, text, base64: btoa(text), ...extra,
  });
  const full = $derived<Detail | null>(
    detail && {
      ...detail,
      req_headers: [["host", detail.host], ["authorization", "[redacted]"], ["content-type", "application/json"], ["x-a-very-long-header-name-that-wraps", "a-very-long-value-".repeat(4)]],
      resp_headers: [["content-type", "application/json"], ["content-length", "2048"]],
      req: view(JSON.stringify({ id: detail.id, note: "only part of this body was stored" }), { size: 4 << 20, truncated: true, decoded_from: "gzip" }),
      resp: respFor(detail),
    },
  );

  function respFor(r: RequestSummary): BodyView {
    const n = +r.id.slice(1);
    if (n % 5 === 1) return view(Array.from({ length: 60 }, (_, i) => `line ${i} of ${r.path}`).join("\n"), { content_type: "text/plain" });
    if (n % 5 === 2) return view("\u0000\u0001binary\u00ff".repeat(64), { content_type: "application/octet-stream", is_text: false, text: null });
    return view(JSON.stringify({ id: r.id, items: Array.from({ length: 40 }, (_, i) => ({ i, path: r.path })) }));
  }

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
    {#if rows.length}
      <RequestList bind:this={list} bind:rows {selected} cap={CAP} onselect={(id) => (selected = id)} />
    {:else}
      <div class="grid flex-1 place-items-center text-callout text-muted-foreground">No matching requests</div>
    {/if}
  </div>
  <div class="flex h-[420px] w-[480px] flex-col overflow-hidden rounded-lg bg-card shadow-(--shadow)" data-testid="reqlab-detail-pane">
    {#if full}{#key full.id}<RequestDetail detail={full} bind:side bind:mode />{/key}{:else}<div class="grid flex-1 place-items-center text-callout text-muted-foreground">Select a request</div>{/if}
  </div>
  <div class="flex flex-wrap items-center gap-2">
    <Button variant="outline" size="sm" onclick={() => add(1)}>New request</Button>
    <Button variant="outline" size="sm" onclick={() => add(5)}>5 new requests</Button>
  </div>
</div>
