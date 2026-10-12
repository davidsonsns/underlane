<script lang="ts">
  // The Logs lines (02 §4.2): one Tab stop, a role=treegrid wrapper that keeps focus while
  // aria-activedescendant points at the active line, and a virtua VList that renders only the
  // lines in view, with measured (variable) heights: wrapped messages and expanded fields render
  // inside the row so virtua measures them. The active line is tracked by id, never by index.
  // The list tails the newest line while the view is at the bottom ("follow") and stops when the
  // user scrolls up. ui-lab (src/lab/LogsLab.svelte) drives this same component for the R-49 check.
  import { tick, untrack } from "svelte";
  import { VList } from "virtua/svelte";
  import { next } from "$lib/listnav";
  import { snapshot } from "$lib/snapshot.svelte";
  import { formatClock } from "$lib/format";
  import { logFields } from "$lib/logfields";
  import { cn } from "$lib/utils";
  import type { LogLine } from "$lib/types";
  import { Button } from "$lib/components/ui/button/index.js";

  let {
    lines = $bindable(),
    names = {},
    showTunnel = false,
    cap = 3000,
    ceiling = cap * 2,
  }: {
    lines: LogLine[];
    /** Tunnel names by id, for the tunnel column. */
    names?: Record<string, string>;
    /** Show the tunnel column (the "All tunnels" view). */
    showTunnel?: boolean;
    /** Lines kept while following (the view at the bottom). */
    cap?: number;
    /** Lines kept while the user is scrolled up (the head trim waits until they're back). */
    ceiling?: number;
  } = $props();

  let box = $state<HTMLDivElement>();
  let list = $state<VList<LogLine>>();
  /** Tail the newest line: true while the view is at the bottom. */
  let follow = $state(true);
  /** Lines that arrived past the ceiling while scrolled up; shown once the user is back at the bottom. */
  let held: LogLine[] = [];
  let activeId = $state<number | null>(null);
  /** Expanded lines by id: only open ones are kept, and trimmed lines are pruned. */
  let open = $state<Record<number, true>>({});
  /** Bumped by reset(): a new result set remounts the VList, so virtua's per-index height
   *  cache (which survives a data change of the same length) starts empty. */
  let gen = $state(0);
  /** virtua's `shift`, set only for the update that trims the head on catching up. */
  let shift = $state(false);
  const active = $derived(activeId == null ? -1 : lines.findIndex((l) => l.id === activeId));
  // Keep the active line mounted while the pointer scrolls it away (03 R-22).
  const keep = $derived(active >= 0 ? [active] : []);

  const parsed = new WeakMap<LogLine, [string, string][]>();
  function fieldsOf(l: LogLine): [string, string][] {
    let f = parsed.get(l);
    if (!f) parsed.set(l, (f = logFields(l.fields)));
    return f;
  }
  /** Expandable only when the fields parse to a non-empty object. */
  const hasFields = (l: LogLine) => l.fields !== "{}" && l.fields !== "" && fieldsOf(l).length > 0;

  /** Drop expansions of lines trimmed from the head (ids ascend). */
  function prune() {
    const min = lines[0]?.id ?? Infinity;
    for (const k in open) if (+k < min) delete open[k];
  }
  /** ui-lab only (dev builds): the expansion map's size and virtua's cached row heights. */
  export const lab = import.meta.env.DEV
    ? { openCount: () => Object.keys(open).length, sizes: () => lines.map((_, i) => list?.getItemSize(i) ?? -1) }
    : undefined;

  // aria-activedescendant only once the row exists: virtua mounts a row one animation frame
  // after scrollToIndex, not on the next tick (decisions.md §17).
  let descendant = $state<string | undefined>();
  $effect(() => {
    const id = active >= 0 ? `log-${activeId}` : null;
    void snapshot.full;
    void gen;
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

  // The VList's scrolling element: follow is read from its scrollTop, which is current, unlike
  // virtua's offset (it catches up on the next scroll event; decisions.md §17).
  const viewport = () => (list ? (box?.firstElementChild as HTMLElement | null) : null);
  const atBottom = (vp: HTMLElement) => vp.scrollTop + vp.clientHeight >= vp.scrollHeight - 40;

  /** Scroll to the newest line (only while following). */
  export async function scrollToEnd() {
    await tick();
    if (follow && lines.length) list?.scrollToIndex(lines.length - 1, { align: "end" });
  }

  // Snapshot mode unmounts the VList; when it (or the list itself) mounts again, restore the
  // view from the saved state (02 §4.3): the end while following, else the active line.
  $effect(() => {
    const l = list;
    if (!l) return;
    untrack(() => {
      if (follow) scrollToEnd();
      else if (active >= 0) l.scrollToIndex(active, { align: "nearest" });
    });
  });

  /**
   * New lines (newer than every line). Following, they're appended, the head is trimmed to `cap`
   * and the view tails the end. Scrolled up, the head trim waits (decisions.md §17): appending
   * below the view doesn't move it, while a head trim would (virtua caches measured heights by
   * index, so dropping the head shifts every cached height). Past `ceiling` the list stops
   * growing and newer lines are held (at most `cap`, all that the trim would keep) until the user
   * is back at the bottom or picks "Jump to latest".
   */
  export function append(add: LogLine[]) {
    if (!add.length) return;
    if (follow) {
      lines = [...lines, ...held, ...add].slice(-cap);
      held = [];
      prune();
      scrollToEnd();
      return;
    }
    const room = Math.max(0, ceiling - lines.length);
    if (room) lines = [...lines, ...add.slice(0, room)];
    if (add.length > room) held = [...held, ...add.slice(room)].slice(-cap);
  }

  /**
   * The parent replaced `lines` (a new tunnel, filter or search): drop held lines and expansions,
   * remount the VList (fresh height cache) and open the new set at the end, following. The
   * remount effect above does the scrolling.
   */
  export function reset() {
    held = [];
    open = {};
    follow = true;
    up = 0;
    gen++;
  }

  /** Back at the live edge: show held lines, trim the head (off screen now) and tail again. */
  function catchUp() {
    follow = true;
    up = 0;
    if (lines.length > cap || held.length) {
      const next = [...lines, ...held].slice(-cap);
      // A shorter list: `shift` drops the trimmed head from virtua's per-index height cache
      // (exact with no held lines). virtua keeps the view across that shift with a scroll jump
      // up, which mustn't read as the user scrolling away.
      if (next.length < lines.length) {
        shift = trimming = true;
        tick().then(() => (shift = false));
      }
      lines = next;
      held = [];
      prune();
    }
    scrollToEnd().then(() => requestAnimationFrame(() => requestAnimationFrame(() => (trimming = false))));
  }

  let lastTop = 0;
  /** Upward movement since the view was last at the bottom. */
  let up = 0;
  /** catchUp() trimmed the head and virtua is re-anchoring: its scroll jump isn't the user's. */
  let trimming = false;
  function onscroll() {
    const vp = viewport();
    if (!vp) return;
    // Only moving up (wheel, scrollbar, keys) stops following, summed so that slow scrolls of a
    // pixel at a time count too. virtua re-measuring rows below the view, or content growing at
    // the end, doesn't move scrollTop, so it never adds up.
    if (atBottom(vp)) {
      up = 0;
      if (!follow || lines.length > cap || held.length) catchUp();
    } else if (!trimming) {
      up += Math.max(0, lastTop - vp.scrollTop);
      if (up > 1) follow = false;
    }
    lastTop = vp.scrollTop;
  }

  function jump() {
    const refocus = box?.parentElement?.contains(document.activeElement) ?? false;
    catchUp();
    // The button unmounts once following; keep keyboard focus in the list.
    if (refocus) box?.focus({ preventScroll: true });
  }

  function move(i: number) {
    if (i < 0 || i >= lines.length) return;
    if (i === lines.length - 1) {
      // The last line: catch up first, then make the newest line active (held lines arrive and
      // the old tail may be trimmed, so `i` is stale).
      catchUp();
      activeId = lines.at(-1)!.id;
    } else {
      activeId = lines[i].id;
      list?.scrollToIndex(i, { align: "nearest" });
    }
  }

  async function setOpen(i: number, value: boolean) {
    const l = lines[i];
    if (!l || !hasFields(l) || !!open[l.id] === value) return;
    if (!value) {
      delete open[l.id];
      return;
    }
    open[l.id] = true;
    // Bring the fields into view once virtua has measured the taller row.
    await tick();
    await new Promise((r) => requestAnimationFrame(r));
    if (lines[i]?.id === l.id && !follow) list?.scrollToIndex(i, { align: "nearest" });
    else if (follow) scrollToEnd();
  }

  /** The first or last line in view, for a list key pressed with no active line. */
  function edge(last: boolean) {
    const vp = viewport();
    if (!vp || !list) return last ? lines.length - 1 : 0;
    return list.findItemIndex(last ? vp.scrollTop + vp.clientHeight - 1 : vp.scrollTop);
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    if (e.key === "ArrowRight" || e.key === "Enter" || e.key === "ArrowLeft") {
      if (active < 0) return;
      e.preventDefault();
      setOpen(active, e.key !== "ArrowLeft");
      return;
    }
    let i: number | null;
    if (active < 0 && ["ArrowUp", "ArrowDown", "PageUp", "PageDown"].includes(e.key)) {
      // Nothing active yet: start on the line at the edge of the view the key points to.
      i = lines.length ? edge(e.key === "ArrowUp" || e.key === "PageUp") : null;
    } else {
      // A page is the lines in view, at about 18px a line.
      const page = Math.floor((viewport()?.clientHeight ?? 360) / 18);
      i = next(e.key, active, lines.length, page);
    }
    if (i == null) return;
    e.preventDefault();
    move(i);
  }

  function onclick(e: MouseEvent) {
    const el = (e.target as Element).closest<HTMLElement>("[role=row]");
    const i = el ? lines.findIndex((l) => l.id === +el.dataset.id!) : -1;
    if (i >= 0) {
      activeId = lines[i].id;
      // A click in the fields selects text there; elsewhere on the line it toggles them.
      if (!(e.target as Element).closest("[data-fields]")) setOpen(i, !open[lines[i].id]);
    }
    box?.focus({ preventScroll: true });
  }

  export function focus() {
    box?.focus();
  }

  const lvl = (l: LogLine) => (l.level === "fatal" ? "FTL" : l.level.slice(0, 3).toUpperCase());
  const lvlText: Record<LogLine["level"], string> = {
    debug: "text-muted-foreground",
    info: "text-muted-foreground",
    warn: "text-degraded-text",
    error: "text-bad-text",
    fatal: "text-bad-text",
  };
  const msgText: Partial<Record<LogLine["level"], string>> = { warn: "text-degraded-text", error: "text-bad-text", fatal: "text-bad-text" };
</script>

{#snippet row(l: LogLine, i: number)}
  {@const fields = hasFields(l)}
  <div
    role="row"
    id="log-{l.id}"
    data-id={l.id}
    aria-level="1"
    aria-expanded={fields ? !!open[l.id] : undefined}
    aria-rowindex={i + 1}
    class={cn(l.id === activeId ? "bg-(--selected)" : "hover:bg-fill", i === 0 && "pt-1.5", i === lines.length - 1 && "pb-3")}
  >
    <!-- 1.55 line height: the lines' rhythm from before the port (text-code's own is 16px). -->
    <div role="gridcell" class="px-4 py-px leading-[1.55]">
      <div class="flex gap-2.5">
        <span class="flex-none text-muted-foreground tabular-nums">{formatClock(l.ts)}</span>
        <span class={cn("w-7 flex-none", lvlText[l.level])}>{lvl(l)}</span>
        {#if showTunnel}<span class="max-w-[110px] flex-none truncate text-muted-foreground">{names[l.tunnel_id] ?? "removed"}</span>{/if}
        <span class={cn("min-w-0 break-words whitespace-pre-wrap", msgText[l.level])}>{l.message}</span>
      </div>
      {#if fields && open[l.id]}
        <div data-fields class="mt-0.5 mb-1.5 ml-24 rounded-md bg-(--code-bg) px-2.5 py-1.5 leading-normal break-all select-text">
          {#each fieldsOf(l) as [k, v], j (j)}<div><span class="text-muted-foreground">{k}</span> {v}</div>{/each}
        </div>
      {/if}
    </div>
  </div>
{/snippet}

<div class="relative flex min-h-0 flex-1 flex-col">
  <!-- Focus stays on this wrapper (never on a row, which virtua may unmount). The inset ring is
       an overlay so the row backgrounds can't cover it; it shows only during keyboard navigation.
       aria-live off: a live log would flood VoiceOver. -->
  <div
    bind:this={box}
    role="treegrid"
    aria-label="Log lines"
    aria-live="off"
    aria-rowcount={lines.length}
    tabindex="0"
    aria-activedescendant={descendant}
    class="relative min-h-0 flex-1 font-mono text-code outline-none after:pointer-events-none after:absolute after:inset-0 after:z-10 kb-focus:after:ring-[3px] kb-focus:after:ring-ring/50 kb-focus:after:ring-inset"
    {onkeydown}
    {onclick}
  >
    {#if snapshot.full}
      <div class="h-full overflow-y-auto">
        {#each lines as l, i (l.id)}{@render row(l, i)}{/each}
      </div>
    {:else}
      {#key gen}
        <VList bind:this={list} data={lines} getKey={(l) => l.id} keepMounted={keep} {shift} {onscroll}>
          {#snippet children(l, i)}{@render row(l, i)}{/snippet}
        </VList>
      {/key}
    {/if}
  </div>
  {#if !follow}
    <!-- An opaque base under the push button: its dark-mode fill is translucent, and the lines
         scroll underneath. -->
    <div class="absolute right-[18px] bottom-3.5 z-20 rounded-md bg-background shadow-(--shadow)">
      <Button variant="outline" onclick={jump}>Jump to latest</Button>
    </div>
  {/if}
</div>
