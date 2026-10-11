<script lang="ts">
  // Pick one of `items` or create a new one (02 F6, NSComboBox-like): a pop-up Button opens a
  // filterable list (ui/popover + ui/command). Text that matches no item offers
  // `Create "<text>"`. Enter commits the highlighted row, Escape closes, and focus goes back to
  // the trigger explicitly (02 R7: WebKit doesn't focus a button on click), unless an outside
  // click put it in another field.
  //   <Combobox value={project} items={names} placeholder="Move to project…" onpick={(name) => …} />
  import Check from "@lucide/svelte/icons/check";
  import ChevronsUpDown from "@lucide/svelte/icons/chevrons-up-down";
  import * as Popover from "$lib/components/ui/popover/index.js";
  import * as Command from "$lib/components/ui/command/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { returnFocusOnClose } from "$lib/focus";
  import { cn } from "$lib/utils";

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
  const typed = $derived(query.trim());
  const canCreate = $derived(typed !== "" && !items.some((x) => x.toLowerCase() === typed.toLowerCase()));
  // Back to the trigger on Escape or a pick; to the field an outside click went to (02 R7).
  const focusOnClose = returnFocusOnClose(() => trigger);
  const row = "relative h-[22px] rounded-sm ps-[22px] pe-3"; // menuItem height (01 §5.18)

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
    <Command.Root class="rounded-none bg-transparent p-0 backdrop-blur-none">
      <Command.Input {placeholder} bind:value={query} />
      <Command.List>
        <Command.Group {heading}>
          {#each items as name (name)}
            <Command.Item value={name} onSelect={() => pick(name)} class={row}>
              {#if name === value}<Check class="absolute start-[6px] size-3 stroke-[2.5]" />{/if}<span class="truncate">{name}</span>
            </Command.Item>
          {/each}
        </Command.Group>
        {#if canCreate}
          <Command.Separator />
          <Command.Item value={`create ${typed}`} forceMount onSelect={() => pick(typed)} class={row}><span class="truncate">Create “{typed}”</span></Command.Item>
        {:else}
          <Command.Empty>Type a name to create one</Command.Empty>
        {/if}
      </Command.List>
    </Command.Root>
  </Popover.Content>
</Popover.Root>
