<script lang="ts">
  // virtua 0.53.3 probes for 02 §4.2 (03-review R-21, R-22). Two lists:
  //   Requests-like: 500 fixed 28px rows (itemSize, getKey, shift on prepend, keepMounted)
  //   Logs-like: 3,000 variable-height rows (some wrap, some expanded), append + head trim
  // The buttons drive the same operations the e2e-less Playwright probe calls through
  // window.__virtua, so the owner can repeat them by hand.
  import { tick } from "svelte";
  import { VList } from "virtua/svelte";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Switch } from "$lib/components/ui/switch/index.js";

  type Req = { id: number; method: string; path: string; status: number };
  type Line = { id: number; level: string; msg: string; fields: number };

  const CAP = 500;
  const LOG_CAP = 3000;
  const methods = ["GET", "POST", "PUT", "DELETE"];
  let nextReq = CAP;
  let nextLine = LOG_CAP;
  const mkReq = (id: number): Req => ({ id, method: methods[id % 4], path: `/api/items/${id}?page=${id % 7}`, status: id % 13 === 0 ? 500 : 200 });
  const words = "edge connection registered origin request served tunnel metrics quic heartbeat".split(" ");
  const mkLine = (id: number): Line => ({
    id,
    level: ["debug", "info", "info", "warn", "error"][id % 5],
    // every 7th line wraps to 2-3 lines; every 11th is "expanded" with fields
    msg: `${id}: ` + Array.from({ length: id % 7 === 0 ? 40 + (id % 3) * 20 : 6 + (id % 5) }, (_, i) => words[(id + i) % words.length]).join(" "),
    fields: id % 11 === 0 ? 3 : 0,
  });

  let reqs = $state<Req[]>(Array.from({ length: CAP }, (_, i) => mkReq(CAP - 1 - i)));
  let lines = $state<Line[]>(Array.from({ length: LOG_CAP }, (_, i) => mkLine(i)));
  let reqShift = $state(false);
  let lineShift = $state(false);
  let keep = $state<number[]>([]);
  let keepFive = $state(false);
  let reqList = $state<VList<Req>>();
  let lineList = $state<VList<Line>>();
  let reqScrolls = $state(0);
  let lastReqOffset = $state(0);
  let lineScrolls = $state(0);

  $effect(() => {
    keep = keepFive ? [5] : [];
  });

  /** Prepend n requests (newest first). With trim, the tail is cut so the length stays at CAP. */
  async function prependReqs(n: number, trim: boolean, shift: boolean) {
    const fresh = Array.from({ length: n }, () => mkReq(nextReq++)).reverse();
    reqShift = shift;
    reqs = trim ? [...fresh, ...reqs].slice(0, CAP) : [...fresh, ...reqs];
    await tick();
    reqShift = false;
  }
  /** Append n log lines; with trim the head is cut so the length stays at LOG_CAP. */
  async function appendLines(n: number, trim: boolean, shift: boolean) {
    const fresh = Array.from({ length: n }, () => mkLine(nextLine++));
    lineShift = shift;
    lines = trim ? [...lines, ...fresh].slice(-LOG_CAP) : [...lines, ...fresh];
    await tick();
    lineShift = false;
  }

  // Playwright entry points (dev-only page).
  if (typeof window !== "undefined") {
    (window as unknown as { __virtua: unknown }).__virtua = {
      req: () => reqList,
      line: () => lineList,
      prependReqs,
      appendLines,
      resetReqs: async () => {
        reqs = Array.from({ length: CAP }, (_, i) => mkReq(CAP - 1 - i));
        nextReq = CAP;
        await tick();
      },
      setKeep: async (k: number[]) => {
        keepFive = false;
        await tick();
        keep = k;
        await tick();
      },
      reqIds: () => reqs.map((r) => r.id),
      lineIds: () => lines.map((l) => l.id),
      counters: () => ({ reqScrolls, lastReqOffset, lineScrolls }),
    };
  }

  const levelColor: Record<string, string> = { debug: "text-muted-foreground", info: "text-foreground", warn: "text-degraded-text", error: "text-bad-text" };
</script>

<div class="grid grid-cols-2 gap-4">
  <div class="flex min-w-0 flex-col gap-2">
    <div class="text-callout text-muted-foreground">Requests: {reqs.length} rows, itemSize 28, getKey id. Scroll events: {reqScrolls}, offset {Math.round(lastReqOffset)}</div>
    <div class="h-[280px] overflow-hidden rounded-lg bg-card shadow-(--shadow)" data-testid="req-viewport">
      <VList
        bind:this={reqList}
        data={reqs}
        getKey={(r) => r.id}
        itemSize={28}
        shift={reqShift}
        keepMounted={keep}
        onscroll={(o) => {
          reqScrolls++;
          lastReqOffset = o;
        }}
        data-testid="req-list"
      >
        {#snippet children(r)}
          <div id="req-{r.id}" data-row={r.id} class="flex h-7 items-center gap-2 border-b-[0.5px] border-border px-3 text-callout">
            <span class="w-12 font-mono text-muted-foreground">{r.method}</span>
            <span class="min-w-0 flex-1 truncate font-mono">{r.path}</span>
            <span class={r.status >= 500 ? "text-bad-text" : "text-ok-text"}>{r.status}</span>
          </div>
        {/snippet}
      </VList>
    </div>
    <div class="flex flex-wrap items-center gap-2">
      <Button variant="outline" size="sm" onclick={() => prependReqs(1, false, (reqList?.getScrollOffset() ?? 0) > 0)}>Prepend 1</Button>
      <Button variant="outline" size="sm" onclick={() => prependReqs(1, true, true)}>Prepend 1 at cap (shift)</Button>
      <Button variant="outline" size="sm" onclick={() => reqList?.scrollToIndex(400)}>scrollToIndex(400)</Button>
      <label class="flex items-center gap-1.5 text-callout"><Switch size="sm" bind:checked={keepFive} />keepMounted [5]</label>
    </div>
  </div>

  <div class="flex min-w-0 flex-col gap-2">
    <div class="text-callout text-muted-foreground">Logs: {lines.length} variable-height lines. Scroll events: {lineScrolls}</div>
    <div class="h-[280px] overflow-hidden rounded-lg bg-card shadow-(--shadow)" data-testid="line-viewport">
      <VList bind:this={lineList} data={lines} getKey={(l) => l.id} shift={lineShift} onscroll={() => lineScrolls++} data-testid="line-list">
        {#snippet children(l)}
          <div id="line-{l.id}" data-row={l.id} class="border-b-[0.5px] border-border px-3 py-1 font-mono text-code">
            <span class={levelColor[l.level]}>{l.level.padEnd(5)}</span> <span class="select-text">{l.msg}</span>
            {#if l.fields}
              <div class="mt-1 grid gap-0.5 ps-6 text-muted-foreground">
                {#each Array.from({ length: l.fields }) as _, i (i)}<div>field_{i}=value-{l.id}-{i}</div>{/each}
              </div>
            {/if}
          </div>
        {/snippet}
      </VList>
    </div>
    <div class="flex flex-wrap items-center gap-2">
      <Button variant="outline" size="sm" onclick={() => lineList?.scrollToIndex(lines.length - 1, { align: "end" })}>Jump to latest</Button>
      <Button variant="outline" size="sm" onclick={() => appendLines(5, true, true)}>Append 5 at cap (shift)</Button>
    </div>
  </div>
</div>
