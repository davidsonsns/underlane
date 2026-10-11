<script lang="ts">
  // The Add tunnel sheet (02 2c): a Dialog that owns its open state. The body lives in
  // AddSheetBody, inside Dialog.Content, so it remounts on every open; the Root stays mounted for
  // the close animation and the return focus.
  import { untrack } from "svelte";
  import { returnFocusTo } from "$lib/focus";
  import * as Dialog from "$lib/components/ui/dialog/index.js";
  import AddSheetBody, { type AddTab } from "./AddSheetBody.svelte";

  let {
    open = $bindable(false),
    tab = "import",
    returnFocus,
    onadded,
  }: {
    open?: boolean;
    tab?: AddTab;
    /** Where focus goes when the sheet closes (02 R7: WebKit doesn't focus a clicked button). */
    returnFocus: () => HTMLElement | null | undefined;
    onadded: (id: string | null) => void;
  } = $props();

  // Bits keeps Content mounted until the 150ms close animation ends, so a reopen inside that
  // window would keep the old body (and a typed token). Key the body on each opening instead,
  // and on the tab, so an add:<tab> link while open switches to that tab.
  let opens = $state(0);
  $effect.pre(() => {
    if (open) untrack(() => opens++);
  });
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="w-[min(560px,calc(100vw-48px))]" onCloseAutoFocus={returnFocusTo(returnFocus)}>
    {#key `${opens}:${tab}`}
      <AddSheetBody initialTab={tab} onclose={() => (open = false)} {onadded} />
    {/key}
  </Dialog.Content>
</Dialog.Root>
