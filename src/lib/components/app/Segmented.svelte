<script generics="T extends string" lang="ts">
  // macOS segmented control (NSSegmentedControl, single selection) on ui/toggle-group.
  // Arrow keys move focus without selecting; Space/Enter selects. It can never be empty:
  // bits-ui writes value = "" when the selected segment is clicked again, before any
  // onValueChange (03-review R-20), so the function binding drops "" before it reaches state.
  // Bits renders the items as role="radio" inside a role="group" (it overrides a passed role).
  import {
    Item as ToggleGroupItem,
    Root as ToggleGroupRoot,
  } from "$lib/components/ui/toggle-group/index.js";

  let {
    value = $bindable(),
    items,
    label,
    onchange,
    class: className,
    ...rest
  }: {
    value: T;
    items: readonly { value: T; label: string; disabled?: boolean }[];
    /** Accessible name of the group (e.g. "Appearance"). */
    label: string;
    /** Called with the new value after a user selection (not for "" or a no-op). */
    onchange?: (value: T) => void;
    class?: string;
    /** Wiring from SettingsRow: id, aria-labelledby, aria-describedby. */
    id?: string;
    "aria-labelledby"?: string;
    "aria-describedby"?: string;
  } = $props();
</script>

<ToggleGroupRoot
  {...rest}
  aria-label={rest["aria-labelledby"] ? undefined : label}
  class={className}
  type="single"
  bind:value={
    () => value,
    (v: string) => {
    if (!v || v === value) {
      return;
    }
    value = v as T;
    onchange?.(v as T);
  }
  }
>
  {#each items as item (item.value)}
    <ToggleGroupItem disabled={item.disabled} value={item.value}
      >{item.label}</ToggleGroupItem
    >
  {/each}
</ToggleGroupRoot>
