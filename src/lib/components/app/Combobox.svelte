<script lang="ts">
  // Pick one of `items` or create a new one (02 F6, NSComboBox-like): a pop-up Button opens a
  // filterable list (ui/popover + ui/command). It opens on the current value. Typed text filters
  // by substring and offers `Create "<text>"` first unless it names an item (comboRows). Enter
  // commits the highlighted row, Escape closes, and focus goes back to the trigger explicitly
  // (02 R7: WebKit doesn't focus a button on click), unless an outside click put it in another field.
  //   <Combobox value={project} items={names} placeholder="Move to project…" onpick={(name) => …} />
  import Check from "@lucide/svelte/icons/check";
  import ChevronsUpDown from "@lucide/svelte/icons/chevrons-up-down";
  import * as Popover from "$lib/components/ui/popover/index.js";
  import { Command as CommandPrimitive } from "bits-ui";
  import * as Command from "$lib/components/ui/command/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { returnFocusOnClose } from "$lib/focus";
  import { menuIndicator } from "$lib/components/ui/mac";
  import { cn } from "$lib/utils";
  import { comboRows } from "$lib/combobox";

  let {
    value,
    items,
    placeholder,
    heading,
    onpick,
    open = $bindable(false),
    id,
    class: className,
    "aria-describedby": describedBy,
    "aria-labelledby": labelledBy,
  }: {
    value: string;
    items: string[];
    /** The search field's placeholder, e.g. "Move to project…". */
    placeholder: string;
    heading?: string;
    /** Called with an existing item or the typed name; not called when `value` is picked again. */
    onpick: (name: string) => void;
    open?: boolean;
    id?: string;
    class?: string;
    "aria-describedby"?: string;
    "aria-labelledby"?: string;
  } = $props();

  let trigger = $state<HTMLElement | null>(null);
  let query = $state("");
  const rows = $derived(comboRows(items, query));
  // The Create row's Bits value: leading whitespace, so it can't equal a (trimmed) item name.
  const CREATE = " create";
  // Back to the trigger on Escape or a pick; to the field an outside click went to (02 R7).
  const focusOnClose = returnFocusOnClose(() => trigger);
  // menuItem geometry (01 §5.18): 22px rows, check column on the left.
  const row = "relative h-[22px] rounded-sm ps-[22px] pe-3";

  function pick(name: string) {
    open = false;
    query = "";
    if (name !== value) onpick(name);
  }
</script>

<Popover.Root bind:open onOpenChange={(o) => !o && (query = "")}>
  <Popover.Trigger bind:ref={trigger}>
    {#snippet child({ props })}
      <Button {...props} {id} aria-describedby={describedBy} aria-labelledby={labelledBy} variant="outline" class={cn("w-55 justify-between", className)}>
        <span class="truncate">{value}</span><ChevronsUpDown class="size-3 text-muted-foreground" />
      </Button>
    {/snippet}
  </Popover.Trigger>
  <Popover.Content class="w-56" {...focusOnClose}>
    <!-- Filtering and order are ours (comboRows), so Bits neither scores nor reorders rows; it
         highlights the first row after each keystroke, and `value` on open. The Viewport gives
         the input aria-controls and aria-activedescendant (Bits reads both from it). -->
    <Command.Root {value} shouldFilter={false} class="rounded-none bg-transparent p-0 backdrop-blur-none">
      <Command.Input {placeholder} bind:value={query} autocapitalize="off" writingsuggestions="false" />
      <Command.List>
        <CommandPrimitive.Viewport>
          {#if rows.create}
            <Command.Item value={CREATE} onSelect={() => pick(rows.create!)} class={row}><span class="truncate">Create “{rows.create}”</span></Command.Item>
          {/if}
          {#if rows.matches.length}
            <Command.Group {heading}>
              {#each rows.matches as name (name)}
                <Command.Item value={name} onSelect={() => pick(name)} class={row}>
                  {#if name === value}<span class={menuIndicator}><Check /></span>{/if}<span class="truncate">{name}</span>
                </Command.Item>
              {/each}
            </Command.Group>
          {:else if !rows.create}
            <Command.Empty forceMount>Type a name to create one</Command.Empty>
          {/if}
        </CommandPrimitive.Viewport>
      </Command.List>
    </Command.Root>
  </Popover.Content>
</Popover.Root>
