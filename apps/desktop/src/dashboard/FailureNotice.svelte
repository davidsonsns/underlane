<script lang="ts">
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import CopyButton from "$lib/components/app/CopyButton.svelte";
  import {
    Description as AlertDescription,
    Root as AlertRoot,
    Title as AlertTitle,
  } from "$lib/components/ui/alert/index.js";
  import { kbRing } from "$lib/components/ui/mac";
  import type { FailureExplanation, Status } from "$lib/types";

  let {
    failure,
    status = "crashed",
    detail = null,
  }: {
    failure: FailureExplanation;
    status?: Status;
    detail?: string | null;
  } = $props();
</script>

<AlertRoot
  role="status"
  variant={status === "degraded" ? "warning" : "destructive"}
>
  {#snippet icon()}
    {#if status === "degraded"}
      <TriangleAlert />
    {:else}
      <CircleAlert />
    {/if}
  {/snippet}
  <AlertTitle>{failure.cause}</AlertTitle>
  {#if failure.action}
    <AlertDescription class="whitespace-pre-wrap"
      >{failure.action}</AlertDescription
    >
  {/if}
  {#if detail && detail !== failure.cause && detail !== failure.raw}
    <AlertDescription class="pt-1.5 whitespace-pre-wrap"
      >{detail}</AlertDescription
    >
  {/if}
  <details class="mt-2 text-muted-foreground">
    <summary class="cursor-pointer rounded-xs {kbRing}">Raw diagnostic</summary>
    <div class="mt-2 space-y-2">
      <pre
        class="max-h-60 overflow-auto whitespace-pre-wrap break-words font-mono text-xs text-foreground select-text"
      >{failure.raw}</pre>
      <CopyButton
        label="Copy diagnostic"
        size="sm"
        value={failure.raw}
        what="diagnostic"
      />
    </div>
  </details>
</AlertRoot>
