<script lang="ts">
  // Shared empty / loading / error states. Copy tells the user what to do next.
  import type { Snippet } from "svelte";
  let {
    kind,
    title,
    message = "",
    children,
  }: { kind: "empty" | "loading" | "error"; title: string; message?: string; children?: Snippet } = $props();
</script>

<div class="panel {kind}" role={kind === "error" ? "alert" : "status"}>
  {#if kind === "loading"}
    <div class="spinner" aria-hidden="true"></div>
  {:else if kind === "error"}
    <div class="badge">!</div>
  {:else}
    <svg class="arch" viewBox="0 0 48 48" aria-hidden="true"><path d="M12 38V24a12 12 0 0 1 24 0v14M6 38h36" /></svg>
  {/if}
  <div class="title">{title}</div>
  {#if message}<div class="msg">{message}</div>{/if}
  {#if children}<div class="actions">{@render children()}</div>{/if}
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 32px 20px;
    text-align: center;
    color: var(--text-2);
    animation: rise 260ms var(--ease-out);
  }
  .title {
    color: var(--text-1);
    font-weight: 600;
    font-size: var(--text-sm);
  }
  .msg {
    max-width: 320px;
    font-size: var(--text-callout);
    line-height: 1.45;
  }
  .actions {
    display: flex;
    gap: 8px;
    margin-top: 8px;
  }
  .arch {
    width: 40px;
    height: 40px;
    fill: none;
    stroke: var(--text-3);
    stroke-width: 2.5;
    stroke-linecap: round;
    margin-bottom: 4px;
  }
  .badge {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border-radius: 50%;
    background: color-mix(in srgb, var(--bad) 14%, transparent);
    color: var(--bad);
    font-weight: 700;
  }
  .error .msg {
    color: var(--text-1);
    -webkit-user-select: text;
    user-select: text;
  }
  .spinner {
    width: 18px;
    height: 18px;
    border-radius: 50%;
    border: 2px solid var(--line-strong);
    border-top-color: var(--text-2);
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to { transform: rotate(360deg); }
  }
  @keyframes rise {
    from { opacity: 0; transform: translateY(4px); }
  }
</style>
