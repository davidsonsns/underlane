<script lang="ts">
  // Shared empty / loading / error states. Copy tells the user what to do next.
  // Built on ui/empty, with ui/spinner for loading (02 §2).
  import type { Snippet } from "svelte";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import * as Empty from "$lib/components/ui/empty/index.js";
  import { Spinner } from "$lib/components/ui/spinner/index.js";
  import LaneGlyph from "./LaneGlyph.svelte";
  let {
    kind,
    title,
    message = "",
    children,
  }: { kind: "empty" | "loading" | "error"; title: string; message?: string; children?: Snippet } = $props();
</script>

<!-- flex-1: in a flex-column pane (Logs, the request list) it fills the pane and centres there. -->
<Empty.Root class="flex-1 animate-rise" role={kind === "error" ? "alert" : "status"}>
  {#if kind === "loading"}
    <!-- The panel is the status region and its title the announcement: the spinner is decoration. -->
    <Spinner role="presentation" aria-hidden="true" />
  {:else if kind === "error"}
    <div class="grid size-7 place-items-center rounded-md bg-control text-bad-text" aria-hidden="true"><CircleAlert class="size-4" /></div>
  {:else}
    <LaneGlyph class="mb-1 text-faint" />
  {/if}
  <Empty.Title>{title}</Empty.Title>
  {#if message}<Empty.Description class={kind === "error" ? "leading-[1.45] text-foreground select-text" : "leading-[1.45]"}>{message}</Empty.Description>{/if}
  {#if children}<Empty.Content>{@render children()}</Empty.Content>{/if}
</Empty.Root>
