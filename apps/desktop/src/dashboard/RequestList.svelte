<script lang="ts" module>
  /** Fixed row height: virtua's itemSize, and the unit of the by-hand anchoring below. */
  export const ROW = 28;
  /** The grid shared by the header and the rows; the column template (gridTemplate in
   *  $lib/columns) goes in their style, since the shown columns and the At width vary. */
  export const columns = "grid items-center gap-1.5 px-3";
</script>

<script lang="ts">
  // The Requests list (02 §4.2): one Tab stop, a role=listbox wrapper that keeps focus while
  // aria-activedescendant points at the active row, and a virtua VList that renders only the
  // rows in view. The active row is tracked by id (the parent's `selected`), never by index, so
  // a prepend, a filter, Clear or the cap can't move the selection to another request.
  import { tick, untrack } from "svelte";
  import { VList } from "virtua/svelte";
  import { columnPrefs } from "$lib/columns.svelte";
  import {
    formatAt,
    formatBytes,
    formatDuration,
    methodText,
    statusClass,
  } from "$lib/format";
  import { snapshot } from "$lib/snapshot.svelte";
  import type { RequestSummary } from "$lib/types";
  import { cn } from "$lib/utils";
  import { observeDescendant } from "./descendant";
  import { prependRequestWindow, requestKeyAction } from "./list-state";
  import { track } from "./reactivity";

  let {
    rows = $bindable(),
    selected,
    template,
    now,
    cap = 500,
    ceiling = cap * 2,
    onselect,
  }: {
    rows: RequestSummary[];
    /** The active request's id. */
    selected: string | null;
    /** The grid-template-columns, as for the header. */
    template: string;
    /** The time At labels are relative to: a request from another day shows its date. */
    now: number;
    /** Rows kept while the list is at the top. */
    cap?: number;
    /** Rows kept while the user is scrolled away from the top (the trim waits until they're back). */
    ceiling?: number;
    /** A user move (click, keys; null when the active row is clicked again or on Escape) or a
     *  fallback after the active row went away. */
    onselect: (id: string | null) => void;
  } = $props();

  let box = $state<HTMLDivElement>();
  let list = $state<VList<RequestSummary>>();
  let shift = $state(false);
  /** Ids that just arrived: their row flashes the accent bar once. */
  let fresh = $state<Record<string, boolean>>({});
  const active = $derived(
    selected === null ? -1 : rows.findIndex((r) => r.id === selected)
  );
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
    if (selected === null) {
      return;
    }
    untrack(() =>
      onselect(
        rows.length
          ? rows[Math.min(Math.max(lastIndex, 0), rows.length - 1)].id
          : null
      )
    );
  });

  // aria-activedescendant only once the row exists: virtua mounts a row one animation frame
  // after scrollToIndex, not on the next tick (decisions.md §17).
  let descendant = $state<string | undefined>();
  $effect(() => {
    const id = selected === null ? null : `req-${selected}`;
    track(active);
    track(snapshot.full);
    return observeDescendant(
      () => box,
      id,
      (value) => {
        descendant = value;
      }
    );
  });

  // Snapshot mode unmounts the VList; when it (or the list itself) mounts again, bring the
  // active row back into view (02 §4.3).
  $effect(() => {
    const l = list;
    if (!l) {
      return;
    }
    untrack(() => active >= 0 && l.scrollToIndex(active, { align: "nearest" }));
  });

  function move(i: number) {
    if (i < 0 || i >= rows.length) {
      return;
    }
    if (i !== active) {
      onselect(rows[i].id);
    }
    list?.scrollToIndex(i, { align: "nearest" });
  }

  const pageSize = () =>
    Math.floor((list?.getViewportSize() ?? box?.clientHeight ?? ROW) / ROW);
  function onkeydown(e: KeyboardEvent) {
    const action = requestKeyAction(
      e,
      selected !== null,
      active,
      rows.length,
      pageSize()
    );
    if (action === null) {
      return;
    }
    e.preventDefault();
    if (action.type === "clear") {
      onselect(null);
    } else {
      move(action.index);
    }
  }

  function onclick(e: MouseEvent) {
    const el = (e.target as Element).closest<HTMLElement>("[role=option]");
    const i = el ? rows.findIndex((r) => r.id === el.dataset.id) : -1;
    // Clicking the active row again closes its detail.
    if (i >= 0 && i === active) {
      onselect(null);
    } else if (i >= 0) {
      move(i);
    }
    box?.focus({ preventScroll: true });
  }

  // New requests arrive as events, sometimes in bursts: queue them and put each batch on top in
  // one update, so the anchoring below runs once per batch and never interleaves with another.
  let incoming: RequestSummary[] = [];
  let flushing = false;

  /** A filter/scope change or pause discards events waiting to join the current view. */
  export function discardPending() {
    incoming = [];
  }

  /** A request that just arrived (newer than every row). Updates of a known id go to the parent. */
  export function add(r: RequestSummary) {
    const q = incoming.findIndex((x) => x.id === r.id);
    if (q >= 0) {
      incoming[q] = r;
    } else {
      incoming.push(r);
    }
    flush();
  }

  async function flush() {
    if (flushing) {
      return;
    }
    flushing = true;
    try {
      while (incoming.length) {
        const batch = incoming.reverse(); // newest first
        incoming = [];
        // biome-ignore lint/performance/noAwaitInLoops: Batches must re-anchor serially so scrolling does not jump.
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
  const viewport = () =>
    list ? (box?.firstElementChild as HTMLElement | null) : null;

  /**
   * Put a batch (newest first) on top (decisions.md §17). At the top of the list the new rows
   * appear in view and the list keeps `cap` rows. Scrolled down, the rows in view stay put: the
   * tail trim waits, so every prepend grows the list and virtua's `shift` keeps the view (it acts
   * only on a length change). Back at the top, onscroll trims to `cap`. Past `ceiling` the trim
   * can't wait: the list stays at `ceiling` and the first visible row is re-anchored by hand.
   */
  async function prepend(batch: RequestSummary[]) {
    const vp = viewport();
    const window = prependRequestWindow(
      rows,
      batch,
      vp?.scrollTop ?? null,
      snapshot.full,
      cap,
      ceiling,
      ROW
    );
    ({ rows } = window);
    if (!window.wait) {
      return;
    }
    ({ shift } = window);
    await tick();
    shift = false;
    restoreAnchor(vp, window.anchor);
  }

  // A trimmed-away anchor aims past the new end; browser clamping keeps the oldest retained rows visible.
  function restoreAnchor(vp: HTMLElement | null, anchor: number | null) {
    if (vp && anchor !== null) {
      vp.scrollTop = anchor;
    }
  }

  // Back at the top: drop the rows kept while the user was scrolled away. The trimmed rows are
  // off screen, and an active row among them falls back like any other trimmed row.
  function onscroll() {
    if (rows.length > cap && (viewport()?.scrollTop ?? 1) <= 0) {
      rows = rows.slice(0, cap);
    }
  }

  export function focus() {
    box?.focus();
  }

  const hidden = $derived(new Set(columnPrefs.hidden));
  const statusText = {
    client: "text-degraded-text",
    none: "",
    ok: "text-ok-text",
    redirect: "text-muted-foreground",
    server: "text-bad-text",
  };
</script>

{#snippet row(
  r: RequestSummary,
  i: number
)}
  <div
    aria-label="{r.method} {r.path}, status {r.status ?? "none"}"
    aria-selected={r.id === selected}
    class={cn(
      columns,
      "h-7 text-callout tabular-nums",
      r.id === selected && "bg-(--selected)",
      r.id !== selected &&
        (i % 2 ? "bg-fill/50 hover:bg-fill" : "hover:bg-fill"),
      fresh[r.id] && "animate-arrive"
    )}
    data-id={r.id}
    id="req-{r.id}"
    role="option"
    tabindex="-1"
    style:grid-template-columns={template}
  >
    {#if !hidden.has("method")}
      <span class={cn("font-semibold text-xs", methodText(r.method))}
        >{r.kind === "websocket" ? "WS" : r.method}</span
      >
    {/if}
    {#if !hidden.has("route")}
      <span class="truncate text-muted-foreground" title={r.host}
        >{r.host}</span
      >
    {/if}
    <span class="truncate font-mono text-code" title={r.path}>{r.path}</span>
    {#if !hidden.has("status")}
      <span
        class={cn(
          "text-right font-semibold",
          statusText[statusClass(r.status)]
        )}
        >{r.status ?? "—"}</span
      >
    {/if}
    {#if !hidden.has("time")}
      <span class="text-right">{formatDuration(r.duration_ms)}</span>
    {/if}
    {#if !hidden.has("size")}
      <span class="text-right">{formatBytes(r.resp_size)}</span>
    {/if}
    {#if !hidden.has("at")}
      <span class="text-right whitespace-nowrap text-muted-foreground"
        >{formatAt(r.ts, now)}</span
      >
    {/if}
  </div>
{/snippet}

<!-- Focus stays on this wrapper (never on a row, which virtua may unmount). The inset ring is an
     overlay so the row backgrounds can't cover it; it shows only during keyboard navigation. -->
<div
  aria-activedescendant={descendant}
  aria-label="Requests"
  class="relative min-h-0 flex-1 outline-none after:pointer-events-none after:absolute after:inset-0 after:z-10 kb-focus:after:ring-[3px] kb-focus:after:ring-ring/50 kb-focus:after:ring-inset"
  {onclick}
  {onkeydown}
  role="listbox"
  tabindex="0"
  bind:this={box}
>
  {#if snapshot.full}
    <!-- overflow-y: scroll keeps the scrollbar gutter even when the rows fit, so the columns line
         up with the header in Requests.svelte, which pads by the same --scrollbar-size. -->
    <div class="h-full overflow-y-scroll">
      {#each rows as r, i (r.id)}
        {@render row(r, i)}
      {/each}
    </div>
  {:else}
    <VList
      data={rows}
      getKey={(r) => r.id}
      itemSize={ROW}
      keepMounted={keep}
      {onscroll}
      {shift}
      style="overflow-y: scroll"
      bind:this={list}
    >
      {#snippet children(
        r,
        i
      )}
        {@render row(r, i)}
      {/snippet}
    </VList>
  {/if}
</div>
