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
  // ui-lab (src/lab/RequestsLab.svelte) drives this same component for the R-49 check.
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
    fresh = {},
    cap = 500,
    onselect,
  }: {
    rows: RequestSummary[];
    /** The active request's id. */
    selected: string | null;
    /** Ids that just arrived: their row flashes the accent bar once. */
    fresh?: Record<string, boolean>;
    cap?: number;
    /** A user move (click, keys) or a fallback after the active row went away. */
    onselect: (id: string | null) => void;
  } = $props();

  let box = $state<HTMLDivElement>();
  let list = $state<VList<RequestSummary>>();
  let shift = $state(false);
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
    onselect(rows[i].id);
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

  /** The first row in view and how far it's scrolled past, to put it back after an update. */
  function anchor() {
    if (!list) return null;
    const offset = list.getScrollOffset();
    const i = list.findItemIndex(offset);
    return rows[i] ? { id: rows[i].id, delta: offset - list.getItemOffset(i) } : null;
  }

  /**
   * Put newly arrived requests (newest first) on top and keep at most `cap` rows. At the top of
   * the list they appear in view. Scrolled down, the rows in view stay put: below the cap virtua's
   * `shift` keeps them (the length grows); once the cap trims the tail the length doesn't change
   * and `shift` does nothing (decisions.md §17), so the first visible row is re-anchored by hand.
   */
  export async function prepend(batch: RequestSummary[]) {
    if (!batch.length) return;
    const ids = new Set(batch.map((r) => r.id));
    const merged = [...batch, ...rows.filter((r) => !ids.has(r.id))];
    const offset = list?.getScrollOffset() ?? 0;
    if (!list || snapshot.full || offset <= 0) {
      rows = merged.slice(0, cap);
    } else if (merged.length <= cap) {
      shift = true;
      rows = merged;
      await tick();
      shift = false;
    } else {
      const a = anchor();
      rows = merged.slice(0, cap);
      await tick();
      const i = a ? rows.findIndex((r) => r.id === a.id) : -1;
      if (i >= 0) list?.scrollTo(list.getItemOffset(i) + a!.delta);
    }
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
    <VList bind:this={list} data={rows} getKey={(r) => r.id} itemSize={ROW} {shift} keepMounted={keep}>
      {#snippet children(r, i)}{@render row(r, i)}{/snippet}
    </VList>
  {/if}
</div>
