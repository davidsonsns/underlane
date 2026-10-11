<script lang="ts">
  // A plain native <select>: app.css's `select { all: revert }` draws the real AppKit pop-up
  // button and opens a real NSMenu (01 §5.11). Layout utilities only, never a background,
  // border, radius or appearance utility, or the bezel goes.
  import { store } from "../lib/store.svelte";
  import type { Tunnel } from "../lib/types";
  let {
    value = $bindable(),
    allowAll = false,
    filter = () => true,
  }: { value: string | null; allowAll?: boolean; filter?: (t: Tunnel) => boolean } = $props();
  const options = $derived(store.tunnels.filter(filter));
</script>

<select class="max-w-55 min-w-40" aria-label="Tunnel" value={value ?? ""} onchange={(e) => (value = e.currentTarget.value || null)}>
  {#if allowAll}<option value="">All tunnels</option>{/if}
  {#each options as t (t.id)}<option value={t.id}>{t.name}</option>{/each}
</select>
