<script lang="ts">
  import { writeText } from "@tauri-apps/plugin-clipboard-manager";
  import { base64ToBytes, formatBytes, formatClock, formatDuration, hexDump, prettyJson, statusClass, toCurl } from "../lib/format";
  import type { BodyView, RequestDetail } from "../lib/types";
  import Icon from "../components/Icon.svelte";

  let { detail, side: initial = "resp" }: { detail: RequestDetail; side?: "req" | "resp" } = $props();
  // svelte-ignore state_referenced_locally
  let side = $state<"req" | "resp">(initial);
  let mode = $state<"pretty" | "raw" | "hex">("pretty");
  let copied = $state(false);

  const body = $derived<BodyView>(side === "req" ? detail.req : detail.resp);
  const headers = $derived(side === "req" ? detail.req_headers : detail.resp_headers);
  const pretty = $derived(body.text != null ? prettyJson(body.text) : null);
  const effectiveMode = $derived(!body.is_text ? "hex" : mode === "pretty" && !pretty ? "raw" : mode);

  async function copyCurl() {
    await writeText(toCurl(detail));
    copied = true;
    setTimeout(() => (copied = false), 1400);
  }
</script>

<div class="wrap">
  <div class="summary">
    <div class="line1">
      <span class="method">{detail.method}</span>
      <span class="url mono selectable">{detail.path}</span>
    </div>
    <div class="line2">
      <span class="status s-{statusClass(detail.status)}">{detail.status ?? "no response"}</span>
      <span>{formatDuration(detail.duration_ms)}</span>
      <span class="muted">{detail.host}</span>
      <span class="faint">{formatClock(detail.ts)}</span>
      {#if detail.kind !== "http"}<span class="pill">{detail.kind === "sse" ? "Server-sent events" : "WebSocket"}, body not stored</span>{/if}
      <span class="spacer"></span>
      <button class="btn" onclick={copyCurl} title={detail.req.truncated ? "The request body was truncated; the command includes only the stored part" : "Copy this request as a curl command"}><Icon name={copied ? "check" : "copy"} size={13} /> {copied ? "Copied" : "Copy as curl"}</button>
    </div>
    {#if detail.error}<div class="error selectable">{detail.error}</div>{/if}
  </div>

  <div class="tabs" role="tablist">
    <button role="tab" aria-selected={side === "req"} class:active={side === "req"} onclick={() => (side = "req")}>Request</button>
    <button role="tab" aria-selected={side === "resp"} class:active={side === "resp"} onclick={() => (side = "resp")}>Response</button>
  </div>

  <div class="scroll">
    <h3>Headers <span class="faint">{headers.length}</span></h3>
    <table class="headers">
      <tbody>
        {#each headers as [k, v], i (i)}
          <tr><th class="mono">{k}</th><td class="mono selectable" class:redacted={v === "[redacted]"}>{v}</td></tr>
        {:else}
          <tr><td class="faint">No headers</td></tr>
        {/each}
      </tbody>
    </table>

    <div class="body-head">
      <h3>Body</h3>
      <span class="faint">{formatBytes(body.size)}{body.content_type ? `, ${body.content_type.split(";")[0]}` : ""}</span>
      {#if body.truncated}<span class="pill warn" title="Only the first {formatBytes(body.stored)} were stored">Truncated at {formatBytes(body.stored)}</span>{/if}
      {#if body.decoded_from}<span class="pill">Decoded {body.decoded_from}{body.decode_partial ? " (partial)" : ""}</span>{/if}
      <span class="spacer"></span>
      {#if body.stored > 0}
        <div class="seg" role="group" aria-label="Body view">
          {#if body.is_text}
            <button class:active={effectiveMode === "pretty"} disabled={!pretty} onclick={() => (mode = "pretty")}>Pretty</button>
            <button class:active={effectiveMode === "raw"} onclick={() => (mode = "raw")}>Raw</button>
          {/if}
          <button class:active={effectiveMode === "hex"} onclick={() => (mode = "hex")}>Hex</button>
        </div>
      {/if}
    </div>
    {#if body.stored === 0}
      <div class="placeholder">{body.size > 0 ? `${formatBytes(body.size)} went through, but this body wasn't stored (capture limit is 0 or the stream isn't captured).` : "Empty body"}</div>
    {:else if effectiveMode === "hex"}
      <pre class="code hex">{hexDump(base64ToBytes(body.base64))}</pre>
    {:else if effectiveMode === "pretty" && pretty}
      <pre class="code">{pretty}</pre>
    {:else}
      <pre class="code">{body.text}</pre>
    {/if}
  </div>
</div>

<style>
  .wrap {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
  .summary {
    padding: 12px 16px 10px;
    border-bottom: 0.5px solid var(--line);
  }
  .line1 {
    display: flex;
    gap: 8px;
    align-items: baseline;
  }
  .method {
    font-weight: 700;
  }
  .url {
    font-size: 12px;
    word-break: break-all;
  }
  .line2 {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 6px;
    font-size: var(--text-sm);
    flex-wrap: wrap;
  }
  .status { font-weight: 600; }
  .s-ok { color: var(--ok); }
  .s-client { color: var(--degraded); }
  .s-server { color: var(--bad); }
  .spacer { flex: 1; }
  .error {
    margin-top: 8px;
    color: var(--bad);
    font-size: var(--text-sm);
  }
  .tabs {
    display: flex;
    gap: 2px;
    padding: 8px 14px 0;
  }
  .tabs button {
    height: 26px;
    padding: 0 12px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--text-2);
  }
  .tabs button.active {
    background: var(--selected);
    color: var(--text-1);
  }
  .scroll {
    flex: 1;
    overflow: auto;
    padding: 6px 16px 20px;
  }
  h3 {
    margin: 12px 0 6px;
    font-size: var(--text-sm);
    font-weight: 600;
  }
  .headers {
    width: 100%;
    border-collapse: collapse;
    font-size: 11.5px;
  }
  .headers th {
    width: 34%;
    padding: 3px 10px 3px 0;
    text-align: left;
    vertical-align: top;
    font-weight: 500;
    color: var(--text-2);
    font-size: inherit;
    overflow-wrap: anywhere;
  }
  .headers td {
    padding: 3px 0;
    word-break: break-all;
    font-size: inherit;
  }
  .redacted {
    color: var(--text-3);
    font-style: italic;
  }
  .body-head {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 12px;
    font-size: var(--text-xs);
  }
  .body-head h3 {
    margin: 0;
  }
  .pill {
    padding: 1px 7px;
    border-radius: 999px;
    background: var(--hover);
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .pill.warn {
    background: color-mix(in srgb, var(--starting) 18%, transparent);
    color: color-mix(in srgb, var(--starting) 60%, var(--text-1));
  }
  .seg {
    display: inline-flex;
    padding: 2px;
    border-radius: 7px;
    background: var(--hover);
  }
  .seg button {
    height: 20px;
    padding: 0 9px;
    border: 0;
    border-radius: 5px;
    background: transparent;
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .seg button.active {
    background: var(--surface);
    color: var(--text-1);
    box-shadow: var(--shadow);
  }
  .seg button:disabled {
    opacity: 0.4;
  }
  .code {
    margin: 8px 0 0;
    padding: 10px 12px;
    border-radius: var(--radius);
    background: var(--code-bg);
    font-family: var(--mono);
    font-size: 11.5px;
    line-height: 1.5;
    white-space: pre-wrap;
    word-break: break-all;
  }
  .hex {
    white-space: pre;
    word-break: normal;
    overflow-x: auto;
  }
  .placeholder {
    margin-top: 8px;
    padding: 14px;
    border-radius: var(--radius);
    background: var(--code-bg);
    color: var(--text-2);
    font-size: var(--text-sm);
  }
</style>
