<script lang="ts" module>
  /** Fixed row height: virtua's itemSize, and the unit of the by-hand anchoring below. */
  export const ROW = 28;
  /** The column template shared by the header and the rows (Method, Path, Status, Time, Size, At). */
  export const columns = "grid grid-cols-[46px_minmax(90px,1fr)_44px_56px_56px_54px] items-center gap-1.5 px-3";
</script>

<script lang="ts">
  // The Requests list (02 §4.2): one Tab stop, a role=listbox wrapper that keeps focus while
  // aria-activedescendant points at the active row, and a virtua VList that renders only the
  // rows in view. The active row is tracked by id (the parent's `selected`), never by index, so
  // a prepend, a filter, Clear or the cap can't move the selection to another request.
  import { tick, untrack } from "svelte";
  import { VList } from "virtua/svelte";
  import { next } from "$lib/listnav";
  import { snapshot } from "$lib/snapshot.svelte";
  import { formatBytes, formatClock, formatDuration, statusClass } from "$lib/format";
  import { cn } from "$lib/utils";
  import type { RequestSummary } from "$lib/types";

  let {
    rows = $bindable(),
    selected,
    cap = 500,
    ceiling = cap * 2,
    onselect,
  }: {
    rows: RequestSummary[];
    /** The active request's id. */
    selected: string | null;
    /** Rows kept while the list is at the top. */
    cap?: number;
    /** Rows kept while the user is scrolled away from the top (the trim waits until they're back). */
    ceiling?: number;
    /** A user move (click, keys) or a fallback after the active row went away. */
    onselect: (id: string | null) => void;
  } = $props();

  let box = $state<HTMLDivElement>();
  let list = $state<VList<RequestSummary>>();
  let shift = $state(false);
  /** Ids that just arrived: their row flashes the accent bar once. */
  let fresh = $state<Record<string, boolean>>({});
  const active = $derived(selected == null ? -1 : rows.findIndex((r) => r.id === selected));
  // Keep the active row mounted while the pointer scrolls it away, so aria-activedescendant
  // never points at a recycled row (03 R-22).
  const keep = $derived(active >= 0 ? [active] : []);

  // The active id went away (filtered out, cleared, trimmed by the cap): fall back to the row now
  // at its last index, or to none. The parent's detail pane follows through onselect.
  let lastIndex = -1;
  $effect(() => {
    const i = active;
    if (i >= 0) {
      lastIndex = i;
      return;
    }
    if (selected == null) return;
    untrack(() => onselect(rows.length ? rows[Math.min(Math.max(lastIndex, 0), rows.length - 1)].id : null));
  });

  // aria-activedescendant only once the row exists: virtua mounts a row one animation frame
  // after scrollToIndex, not on the next tick (decisions.md §17).
  let descendant = $state<string | undefined>();
  $effect(() => {
    const id = selected == null ? null : `req-${selected}`;
    void active;
    void snapshot.full;
    let live = true;
    (async () => {
      await tick();
      for (let n = 0; live && n < 4; n++) {
        if (!id || box?.querySelector(`[id="${id}"]`)) break;
        await new Promise((r) => requestAnimationFrame(r));
      }
      if (live) descendant = id && box?.querySelector(`[id="${id}"]`) ? id : undefined;
    })();
    return () => {
      live = false;
    };
  });

  // Snapshot mode unmounts the VList; when it (or the list itself) mounts again, bring the
  // active row back into view (02 §4.3).
  $effect(() => {
    const l = list;
    if (!l) return;
    untrack(() => active >= 0 && l.scrollToIndex(active, { align: "nearest" }));
  });

  function move(i: number) {
    if (i < 0 || i >= rows.length) return;
    if (i !== active) onselect(rows[i].id);
    list?.scrollToIndex(i, { align: "nearest" });
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    const page = Math.floor((list?.getViewportSize() ?? box?.clientHeight ?? ROW) / ROW);
    const i = next(e.key, active, rows.length, page);
    if (i == null) return;
    e.preventDefault();
    move(i);
  }

  function onclick(e: MouseEvent) {
    const el = (e.target as Element).closest<HTMLElement>("[role=option]");
    const i = el ? rows.findIndex((r) => r.id === el.dataset.id) : -1;
    if (i >= 0) move(i);
    box?.focus({ preventScroll: true });
  }

  // New requests arrive as events, sometimes in bursts: queue them and put each batch on top in
  // one update, so the anchoring below runs once per batch and never interleaves with another.
  let incoming: RequestSummary[] = [];
  let flushing = false;

  /** A request that just arrived (newer than every row). Updates of a known id go to the parent. */
  export function add(r: RequestSummary) {
    const q = incoming.findIndex((x) => x.id === r.id);
    if (q >= 0) incoming[q] = r;
    else incoming.push(r);
    flush();
  }

  async function flush() {
    if (flushing) return;
    flushing = true;
    try {
      while (incoming.length) {
        const batch = incoming.reverse(); // newest first
        incoming = [];
        await prepend(batch);
        for (const r of batch) {
          fresh[r.id] = true;
          setTimeout(() => delete fresh[r.id], 1200);
        }
      }
    } finally {
      flushing = false;
    }
  }

  // The VList's scrolling element. Anchoring reads and writes its scrollTop, not
  // list.getScrollOffset(): virtua's offset only catches up on the next scroll event, so in a
  // burst it would still hold the offset from before the previous batch's re-anchoring.
  const viewport = () => (list ? (box?.firstElementChild as HTMLElement | null) : null);

  /**
   * Put a batch (newest first) on top (decisions.md §17). At the top of the list the new rows
   * appear in view and the list keeps `cap` rows. Scrolled down, the rows in view stay put: the
   * tail trim waits, so every prepend grows the list and virtua's `shift` keeps the view (it acts
   * only on a length change). Back at the top, onscroll trims to `cap`. Past `ceiling` the trim
   * can't wait: the list stays at `ceiling` and the first visible row is re-anchored by hand.
   */
  async function prepend(batch: RequestSummary[]) {
    const ids = new Set(batch.map((r) => r.id));
    const merged = [...batch, ...rows.filter((r) => !ids.has(r.id))];
    const vp = viewport();
    const top = vp?.scrollTop ?? 0;
    if (!vp || snapshot.full || top <= 0) {
      rows = merged.slice(0, cap);
    } else if (merged.length <= ceiling) {
      shift = true;
      rows = merged;
      await tick();
      shift = false;
    } else {
      // Hold the first visible row at its offset. If the trim removed it (the view was within a
      // batch of the end), its untrimmed index aims past the new end and the browser clamps the
      // scroll: the view shows the oldest rows kept. Nothing that was on screen survived, so
      // there's no row to hold still; the same clamp applies when the view was already at the end.
      const i = Math.floor(top / ROW);
      const j = merged.findIndex((r) => r.id === rows[i]?.id);
      rows = merged.slice(0, ceiling);
      await tick();
      if (j >= 0) vp.scrollTop = j * ROW + (top - i * ROW);
    }
  }

  // Back at the top: drop the rows kept while the user was scrolled away. The trimmed rows are
  // off screen, and an active row among them falls back like any other trimmed row.
  function onscroll() {
    if (rows.length > cap && (viewport()?.scrollTop ?? 1) <= 0) rows = rows.slice(0, cap);
  }

  export function focus() {
    box?.focus();
  }

  const methodText = (m: string) =>
    m === "DELETE"
      ? "text-bad-text"
      : m === "POST" || m === "PUT" || m === "PATCH"
        ? // A darkened accent, as before the port (the raw accent is under 4.5:1 on white).
          "text-[color-mix(in_srgb,var(--tint)_80%,var(--foreground))]"
        : "text-muted-foreground";
  const statusText = { ok: "text-ok-text", redirect: "text-muted-foreground", client: "text-degraded-text", server: "text-bad-text", none: "" };
</script>

{#snippet row(r: RequestSummary, i: number)}
  <div
    role="option"
    id="req-{r.id}"
    data-id={r.id}
    aria-selected={r.id === selected}
    aria-label="{r.method} {r.path}, status {r.status ?? 'none'}"
    class={cn(
      columns,
      "h-7 text-callout tabular-nums",
      r.id === selected ? "bg-(--selected)" : i % 2 ? "bg-fill/50 hover:bg-fill" : "hover:bg-fill",
      fresh[r.id] && "animate-arrive",
    )}
  >
    <span class={cn("text-xs font-semibold", methodText(r.method))}>{r.kind === "websocket" ? "WS" : r.method}</span>
    <span class="truncate font-mono text-code" title={r.path}>{r.path}</span>
    <span class={cn("text-right font-semibold", statusText[statusClass(r.status)])}>{r.status ?? "—"}</span>
    <span class="text-right">{formatDuration(r.duration_ms)}</span>
    <span class="text-right">{formatBytes(r.resp_size)}</span>
    <span class="text-right text-muted-foreground">{formatClock(r.ts).slice(0, 8)}</span>
  </div>
{/snippet}

<!-- Focus stays on this wrapper (never on a row, which virtua may unmount). The inset ring is an
     overlay so the row backgrounds can't cover it; it shows only during keyboard navigation. -->
<div
  bind:this={box}
  role="listbox"
  aria-label="Requests"
  tabindex="0"
  aria-activedescendant={descendant}
  class="relative min-h-0 flex-1 outline-none after:pointer-events-none after:absolute after:inset-0 after:z-10 kb-focus:after:ring-[3px] kb-focus:after:ring-ring/50 kb-focus:after:ring-inset"
  {onkeydown}
  {onclick}
>
  {#if snapshot.full}
    <div class="h-full overflow-y-auto">
      {#each rows as r, i (r.id)}{@render row(r, i)}{/each}
    </div>
  {:else}
    <VList bind:this={list} data={rows} getKey={(r) => r.id} itemSize={ROW} {shift} keepMounted={keep} {onscroll}>
      {#snippet children(r, i)}{@render row(r, i)}{/snippet}
    </VList>
  {/if}
</div>
