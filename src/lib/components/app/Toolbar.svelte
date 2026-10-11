<script lang="ts">
  // The 52px window toolbar of every dashboard view. The drag regions are load-bearing:
  // the window has no title bar. leading-legacy keeps base.css's line height until Phase 3.
  import type { Snippet } from "svelte";
  let { title, subtitle = "", children }: { title: string; subtitle?: string; children?: Snippet } = $props();
</script>

<div class="flex h-(--toolbar-h) flex-none items-center justify-between gap-3 border-b-[0.5px] border-border pr-4 pl-5 leading-legacy" data-tauri-drag-region>
  <div class="flex min-w-0 items-baseline gap-2.5" data-tauri-drag-region>
    <h1 class="m-0 text-lg leading-legacy font-semibold" data-tauri-drag-region>{title}</h1>
    {#if subtitle}<span class="truncate text-callout leading-legacy text-muted-foreground" data-tauri-drag-region>{subtitle}</span>{/if}
  </div>
  <div class="flex items-center gap-2">
    {#if children}{@render children()}{/if}
  </div>
</div>
