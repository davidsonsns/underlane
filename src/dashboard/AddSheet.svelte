<script lang="ts">
  // The Add tunnel sheet (02 2c): a Dialog that owns its open state. The body lives in
  // AddSheetBody, inside Dialog.Content, so it remounts on every open; the Root stays mounted for
  // the close animation and the return focus.
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
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="w-[min(560px,calc(100vw-48px))]" onCloseAutoFocus={returnFocusTo(returnFocus)}>
    <AddSheetBody initialTab={tab} onclose={() => (open = false)} {onadded} />
  </Dialog.Content>
</Dialog.Root>
