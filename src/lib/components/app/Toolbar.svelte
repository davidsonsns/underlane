<script lang="ts">
  import type { Snippet } from "svelte";
  let { title, subtitle = "", children }: { title: string; subtitle?: string; children?: Snippet } = $props();
</script>

<div class="toolbar" data-tauri-drag-region>
  <div class="heading" data-tauri-drag-region>
    <h1 data-tauri-drag-region>{title}</h1>
    {#if subtitle}<span class="sub" data-tauri-drag-region>{subtitle}</span>{/if}
  </div>
  <div class="tools">
    {#if children}{@render children()}{/if}
  </div>
</div>

<style>
  .toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    height: 52px;
    padding: 0 16px 0 20px;
    border-bottom: 0.5px solid var(--line);
    flex: none;
  }
  .heading {
    display: flex;
    align-items: baseline;
    gap: 10px;
    min-width: 0;
  }
  h1 {
    margin: 0;
    font-size: var(--text-lg);
    font-weight: 600;
  }
  .sub {
    color: var(--text-2);
    font-size: var(--text-callout);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tools {
    display: flex;
    align-items: center;
    gap: 8px;
  }
</style>
