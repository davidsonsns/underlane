<script lang="ts">
  // The toolbar's tunnel pop-up: ui/select, drawn like the Combobox trigger. "All tunnels" is
  // the ALL sentinel, because Bits treats an empty value as "nothing selected".
  import { store } from "../lib/store.svelte";
  import type { Tunnel } from "../lib/types";
  import * as Select from "$lib/components/ui/select/index.js";
  let {
    value = $bindable(),
    allowAll = false,
    filter = () => true,
  }: { value: string | null; allowAll?: boolean; filter?: (t: Tunnel) => boolean } = $props();
  const ALL = "*";
  const options = $derived(store.tunnels.filter(filter));
  const label = $derived(options.find((t) => t.id === value)?.name ?? (allowAll ? "All tunnels" : ""));
</script>

<Select.Root type="single" value={value ?? ALL} onValueChange={(v) => (value = v === ALL ? null : v)}>
  <Select.Trigger class="max-w-55 min-w-40" aria-label="Tunnel">{label}</Select.Trigger>
  <Select.Content>
    {#if allowAll}<Select.Item value={ALL} label="All tunnels" />{/if}
    {#each options as t (t.id)}<Select.Item value={t.id} label={t.name} />{/each}
  </Select.Content>
</Select.Root>
