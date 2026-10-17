<script lang="ts">
  // The New sheet: a Dialog that owns its open state. The body lives in
  // AddSheetBody, inside DialogContent, so it remounts on every open; the Root stays mounted for
  // the close animation and the return focus.
  import { untrack } from "svelte";
  import {
    Content as DialogContent,
    Root as DialogRoot,
  } from "$lib/components/ui/dialog/index.js";
  import { returnFocusTo } from "$lib/focus";
  import AddSheetBody, { type AddTab } from "./AddSheetBody.svelte";
  import { track } from "./reactivity";

  let {
    open = $bindable(false),
    tab = "project",
    group = "",
    returnFocus,
    onadded,
  }: {
    open?: boolean;
    tab?: AddTab;
    group?: string;
    /** Where focus goes when the sheet closes (02 R7: WebKit doesn't focus a clicked button). */
    returnFocus: () => HTMLElement | null | undefined;
    onadded: (id: string) => void | Promise<void>;
  } = $props();

  // Bits keeps Content mounted until the 150ms close animation ends, so a reopen inside that
  // window would keep the old body (and a typed token). Key the body on each opening instead,
  // and on the tab, so an add:<tab> link while open switches to that tab.
  let opens = $state(0);
  let busy = $state(false);
  $effect.pre(() => {
    track(tab);
    track(group);
    if (open) {
      untrack(() => {
        opens += 1;
        busy = false;
      });
    }
  });
</script>

<DialogRoot bind:open>
  <DialogContent
    class="w-[min(560px,calc(100vw-48px))]"
    onCloseAutoFocus={returnFocusTo(returnFocus)}
    onEscapeKeydown={(e) => {
      if (busy) {
        e.preventDefault();
      }
    }}
    onInteractOutside={(e) => {
      if (busy) {
        e.preventDefault();
      }
    }}
  >
    {#key `${opens}:${tab}`}
      <AddSheetBody
        initialGroup={group}
        initialTab={tab}
        {onadded}
        onclose={() => {
          if (!busy) {
            open = false;
          }
        }}
        bind:busy
      />
    {/key}
  </DialogContent>
</DialogRoot>
