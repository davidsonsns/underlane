<script lang="ts">
  // Pick one of `items` or create a new one (02 F6, NSComboBox-like): a pop-up Button opens a
  // filterable list (ui/popover + ui/command). It opens on the current value. Typed text filters
  // by substring and offers `Create "<text>"` first unless it names an item (comboRows). Enter
  // commits the highlighted row, Escape closes, and focus goes back to the trigger explicitly
  // (02 R7: WebKit doesn't focus a button on click), unless an outside click put it in another field.
  //   <Combobox value={group} items={names} placeholder="Move to group…" onpick={(name) => …} />
  import Check from "@lucide/svelte/icons/check";
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import { Command as CommandPrimitive } from "bits-ui";
  import { tick } from "svelte";
  import { comboRows } from "$lib/combobox";
  import { Button } from "$lib/components/ui/button/index.js";
  import {
    Empty as CommandEmpty,
    Group as CommandGroup,
    Input as CommandInput,
    Item as CommandItem,
    List as CommandList,
    Root as CommandRoot,
  } from "$lib/components/ui/command/index.js";
  import { menuIndicator } from "$lib/components/ui/mac";
  import {
    Content as PopoverContent,
    Root as PopoverRoot,
    Trigger as PopoverTrigger,
  } from "$lib/components/ui/popover/index.js";
  import { returnFocusOnClose } from "$lib/focus";
  import { cn } from "$lib/utils";

  let {
    value,
    items,
    placeholder,
    heading,
    onpick,
    open = $bindable(false),
    disabled = false,
    id,
    class: className,
    "aria-describedby": describedBy,
    "aria-labelledby": labelledBy,
  }: {
    value: string;
    items: string[];
    /** The search field's placeholder, e.g. "Move to group…". */
    placeholder: string;
    heading?: string;
    /** Called with an existing item or the typed name; not called when `value` is picked again. */
    onpick: (name: string) => void;
    open?: boolean;
    disabled?: boolean;
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
  // Matches the shared 28px menu rows, with a check column on the left.
  const row = "relative h-7 rounded-sm ps-[22px] pe-3";

  // Bits highlights the first row one tick after every search change. When the search is blank
  // again (cleared, or only spaces), put the highlight back on the current value, so Enter can't
  // move the tunnel to whatever group happens to be first.
  let cmd = $state<CommandPrimitive.Root | null>(null);
  async function restoreHighlight() {
    await tick();
    await tick();
    const i = items.indexOf(value);
    if (!query.trim() && cmd && i >= 0) {
      cmd.updateSelectedToIndex(i);
    }
  }

  function pick(name: string) {
    open = false;
    query = "";
    if (name !== value) {
      onpick(name);
    }
  }
</script>

<PopoverRoot
  onOpenChange={(o) => {
    if (!o) {
      query = "";
    }
  }}
  bind:open
>
  <PopoverTrigger bind:ref={trigger}>
    {#snippet child({
      props,
    })}
      <Button
        {...props}
        aria-describedby={describedBy}
        aria-labelledby={labelledBy}
        class={cn("w-55 justify-between", className)}
        {disabled}
        {id}
        title={value || undefined}
        variant="outline"
      >
        <span class="truncate">{value || "None"}</span>
        <ChevronDown class="size-3 text-muted-foreground" />
      </Button>
    {/snippet}
  </PopoverTrigger>
  <PopoverContent class="w-56" {...focusOnClose}>
    <!-- Filtering and order are ours (comboRows), so Bits neither scores nor reorders rows; it
         highlights the first row after each keystroke, and `value` on open. The Viewport gives
         the input aria-controls and aria-activedescendant (Bits reads both from it). -->
    <CommandRoot
      class="rounded-none bg-transparent p-0 backdrop-blur-none"
      shouldFilter={false}
      {value}
      bind:api={cmd}
    >
      <CommandInput
        autocapitalize="off"
        oninput={restoreHighlight}
        {placeholder}
        writingsuggestions="false"
        bind:value={query}
      />
      <CommandList>
        <CommandPrimitive.Viewport>
          {#if rows.create}
            <CommandItem
              class={row}
              onSelect={() => pick(rows.create ?? "")}
              value={CREATE}
              ><span class="truncate">Create “{rows.create}”</span></CommandItem
            >
          {/if}
          {#if rows.matches.length}
            <CommandGroup {heading}>
              {#each rows.matches as name (name)}
                <CommandItem
                  class={row}
                  onSelect={() => pick(name)}
                  value={name}
                >
                  {#if name === value}
                    <span class={menuIndicator}><Check /></span>
                  {/if}
                  <span class="truncate" title={name}>{name}</span>
                </CommandItem>
              {/each}
            </CommandGroup>
          {:else if !rows.create}
            <CommandEmpty forceMount>Type a name to create one</CommandEmpty>
          {/if}
        </CommandPrimitive.Viewport>
      </CommandList>
    </CommandRoot>
  </PopoverContent>
</PopoverRoot>
