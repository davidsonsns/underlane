<script lang="ts" generics="T extends string">
  // macOS segmented control (NSSegmentedControl, single selection) on ui/toggle-group.
  // Arrow keys move focus without selecting; Space/Enter selects. It can never be empty:
  // bits-ui writes value = "" when the selected segment is clicked again, before any
  // onValueChange (03-review R-20), so the function binding drops "" before it reaches state.
  import * as ToggleGroup from "$lib/components/ui/toggle-group/index.js";

  let {
    value = $bindable(),
    items,
    label,
    onchange,
    class: className,
  }: {
    value: T;
    items: readonly { value: T; label: string; disabled?: boolean }[];
    /** Accessible name of the group (e.g. "Appearance"). */
    label: string;
    /** Called with the new value after a user selection (not for "" or a no-op). */
    onchange?: (value: T) => void;
    class?: string;
  } = $props();
</script>

<ToggleGroup.Root
  type="single"
  role="radiogroup"
  aria-label={label}
  class={className}
  bind:value={
    () => value,
    (v: string) => {
      if (!v || v === value) return;
      value = v as T;
      onchange?.(v as T);
    }
  }
>
  {#each items as item (item.value)}
    <ToggleGroup.Item value={item.value} disabled={item.disabled}>{item.label}</ToggleGroup.Item>
  {/each}
</ToggleGroup.Root>
