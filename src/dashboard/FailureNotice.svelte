<script lang="ts">
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import type { FailureExplanation, Status } from "$lib/types";
  import CopyButton from "$lib/components/app/CopyButton.svelte";
  import { kbRing } from "$lib/components/ui/mac";
  import * as Alert from "$lib/components/ui/alert/index.js";

  let { failure, status = "crashed", detail = null }: {
    failure: FailureExplanation;
    status?: Status;
    detail?: string | null;
  } = $props();
</script>

<Alert.Root variant={status === "degraded" ? "warning" : "destructive"} role="status">
  {#snippet icon()}
    {#if status === "degraded"}<TriangleAlert />{:else}<CircleAlert />{/if}
  {/snippet}
  <Alert.Title>{failure.cause}</Alert.Title>
  {#if failure.action}<Alert.Description class="whitespace-pre-wrap">{failure.action}</Alert.Description>{/if}
  {#if detail && detail !== failure.cause && detail !== failure.raw}
    <Alert.Description class="pt-1.5 whitespace-pre-wrap">{detail}</Alert.Description>
  {/if}
  <details class="mt-2 text-muted-foreground">
    <summary class="cursor-pointer rounded-xs {kbRing}">Raw diagnostic</summary>
    <div class="mt-2 space-y-2">
      <pre class="max-h-60 overflow-auto whitespace-pre-wrap break-words font-mono text-xs text-foreground select-text">{failure.raw}</pre>
      <CopyButton value={failure.raw} what="diagnostic" label="Copy diagnostic" size="sm" />
    </div>
  </details>
</Alert.Root>
