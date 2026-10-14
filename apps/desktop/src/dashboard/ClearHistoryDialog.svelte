<script lang="ts">
  import {
    Cancel,
    Content,
    Description,
    Footer,
    Header,
    Root,
    Title,
  } from "$lib/components/ui/alert-dialog/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { returnFocusTo } from "$lib/focus";

  let {
    open = $bindable(false),
    clearing,
    kind,
    target,
    failure,
    trigger,
    onclear,
  }: {
    open?: boolean;
    clearing: boolean;
    kind: "logs" | "requests";
    target?: string;
    failure: string | null;
    trigger: () => HTMLElement | null;
    onclear: () => Promise<void>;
  } = $props();
  function preventWhileClearing(event: Event) {
    if (clearing) {
      event.preventDefault();
    }
  }
</script>
<Root bind:open
  ><Content
    onCloseAutoFocus={returnFocusTo(trigger)}
    onEscapeKeydown={preventWhileClearing}
    onInteractOutside={preventWhileClearing}
  >
    <Header
      ><Title>Clear {kind === "logs" ? "log" : "request"} history?</Title
      ><Description
        >Delete all {kind === "logs" ? "log lines" : "captured requests"} for “{target}”,
        including {kind === "logs" ? "lines" : "requests"} hidden by filters.
        This can't be undone.</Description
      ></Header
    >
    {#if failure}
      <p class="text-xs text-destructive" role="alert">{failure}</p>
    {/if}
    <Footer
      ><Cancel disabled={clearing}>Cancel</Cancel
      ><Button disabled={clearing} onclick={onclear} variant="destructive"
        >{clearing ? "Clearing…" : "Clear history"}</Button
      ></Footer
    >
  </Content></Root
>
