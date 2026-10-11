<script lang="ts">
  // Shared empty / loading / error states. Copy tells the user what to do next.
  // Built on ui/empty. The loading ring stays the drawn 18px ring (not ui/spinner) so the
  // panel looks exactly as before the port; leading-legacy keeps base.css's line height.
  import type { Snippet } from "svelte";
  import * as Empty from "$lib/components/ui/empty/index.js";
  import ArchGlyph from "./ArchGlyph.svelte";
  let {
    kind,
    title,
    message = "",
    children,
  }: { kind: "empty" | "loading" | "error"; title: string; message?: string; children?: Snippet } = $props();
</script>

<Empty.Root class="animate-rise leading-legacy" role={kind === "error" ? "alert" : "status"}>
  {#if kind === "loading"}
    <div class="size-[18px] animate-spinner rounded-full border-2 border-(color:--line-strong) border-t-muted-foreground" aria-hidden="true"></div>
  {:else if kind === "error"}
    <div class="grid size-7 place-items-center rounded-full bg-bad/14 font-bold text-bad">!</div>
  {:else}
    <ArchGlyph class="mb-1 text-faint" />
  {/if}
  <Empty.Title class="leading-legacy">{title}</Empty.Title>
  {#if message}<Empty.Description class={kind === "error" ? "leading-[1.45] text-foreground select-text" : "leading-[1.45]"}>{message}</Empty.Description>{/if}
  {#if children}<Empty.Content>{@render children()}</Empty.Content>{/if}
</Empty.Root>
