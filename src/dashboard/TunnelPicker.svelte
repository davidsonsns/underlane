<script lang="ts">
  import { store } from "../lib/store.svelte";
  import type { Tunnel } from "../lib/types";
  let {
    value = $bindable(),
    allowAll = false,
    filter = () => true,
  }: { value: string | null; allowAll?: boolean; filter?: (t: Tunnel) => boolean } = $props();
  const options = $derived(store.tunnels.filter(filter));
</script>

<select class="field picker" aria-label="Tunnel" value={value ?? ""} onchange={(e) => (value = (e.target as HTMLSelectElement).value || null)}>
  {#if allowAll}<option value="">All tunnels</option>{/if}
  {#each options as t (t.id)}<option value={t.id}>{t.name}</option>{/each}
</select>

<style>
  .picker {
    min-width: 160px;
    max-width: 220px;
  }
</style>
