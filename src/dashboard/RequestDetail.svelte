<script lang="ts">
  import { base64ToBytes, formatBytes, formatClock, formatDuration, hexDump, prettyJson, statusClass, toCurl } from "../lib/format";
  import type { BodyView, RequestDetail } from "../lib/types";
  import * as Tabs from "$lib/components/ui/tabs/index.js";
  import { Badge } from "$lib/components/ui/badge/index.js";
  import CopyButton from "$lib/components/app/CopyButton.svelte";
  import Segmented from "$lib/components/app/Segmented.svelte";
  import Tip from "$lib/components/app/Tip.svelte";

  type Side = "req" | "resp";
  type Mode = "pretty" | "raw" | "hex";
  let { detail, side: initial = "resp" }: { detail: RequestDetail; side?: Side } = $props();
  // svelte-ignore state_referenced_locally
  let side = $state<Side>(initial);
  let mode = $state<Mode>("pretty");

  const body = $derived<BodyView>(side === "req" ? detail.req : detail.resp);
  const headers = $derived(side === "req" ? detail.req_headers : detail.resp_headers);
  const pretty = $derived(body.text != null ? prettyJson(body.text) : null);
  const effectiveMode = $derived<Mode>(!body.is_text ? "hex" : mode === "pretty" && !pretty ? "raw" : mode);
  const modes = $derived(
    body.is_text
      ? [
          { value: "pretty" as const, label: "Pretty", disabled: !pretty },
          { value: "raw" as const, label: "Raw" },
          { value: "hex" as const, label: "Hex" },
        ]
      : [{ value: "hex" as const, label: "Hex" }],
  );
  const statusText = { ok: "text-ok-text", redirect: "", client: "text-degraded-text", server: "text-bad-text", none: "" };
  const code = "mt-2 rounded-lg bg-(--code-bg) px-3 py-2.5 font-mono text-code leading-normal";
</script>

<div class="flex h-full min-h-0 flex-col">
  <div class="border-b-[0.5px] border-border px-4 pt-3 pb-2.5">
    <div class="flex items-baseline gap-2">
      <span class="font-bold">{detail.method}</span>
      <span class="font-mono text-callout break-all select-text">{detail.path}</span>
    </div>
    <div class="mt-1.5 flex flex-wrap items-center gap-2.5 text-callout">
      <span class={["font-semibold", statusText[statusClass(detail.status)]]}>{detail.status ?? "no response"}</span>
      <span>{formatDuration(detail.duration_ms)}</span>
      <span class="text-muted-foreground">{detail.host}</span>
      <span class="text-muted-foreground">{formatClock(detail.ts)}</span>
      {#if detail.kind !== "http"}<Badge>{detail.kind === "sse" ? "Server-sent events" : "WebSocket"}, body not stored</Badge>{/if}
      <span class="flex-1"></span>
      <!-- "Copy as curl" is read by the e2e (02 §0.3): it stays a text button with this label. -->
      <CopyButton
        value={() => toCurl(detail)}
        what="curl command"
        label="Copy as curl"
        tip={detail.req.truncated ? "The request body was truncated; the command includes only the stored part" : "Copy this request as a curl command"}
      />
    </div>
    {#if detail.error}<div class="mt-2 text-callout text-bad-text select-text">{detail.error}</div>{/if}
  </div>

  <Tabs.Root class="flex min-h-0 flex-1 flex-col" bind:value={() => side, (v) => (side = v as Side)}>
    <Tabs.List class="mx-4 mt-2">
      <Tabs.Trigger value="req">Request</Tabs.Trigger>
      <Tabs.Trigger value="resp">Response</Tabs.Trigger>
    </Tabs.List>
    <!-- Bits hides the inactive panel, and its body (or hex dump) isn't even computed. -->
    <Tabs.Content value="req" tabindex={-1} class="min-h-0 overflow-auto px-4 pt-1.5 pb-5">{#if side === "req"}{@render panel()}{/if}</Tabs.Content>
    <Tabs.Content value="resp" tabindex={-1} class="min-h-0 overflow-auto px-4 pt-1.5 pb-5">{#if side === "resp"}{@render panel()}{/if}</Tabs.Content>
  </Tabs.Root>
</div>

{#snippet panel()}
  <h3 class="mt-3 mb-1.5 text-callout font-semibold">Headers <span class="text-muted-foreground">{headers.length}</span></h3>
  <table class="w-full border-collapse font-mono text-code">
    <tbody>
      {#each headers as [k, v], i (i)}
        <tr>
          <th class="w-[34%] py-0.75 pe-2.5 text-left align-top font-medium wrap-anywhere text-muted-foreground">{k}</th>
          <td class={["py-0.75 break-all select-text", v === "[redacted]" && "text-muted-foreground italic"]}>{v}</td>
        </tr>
      {:else}
        <tr><td class="font-sans text-muted-foreground">No headers</td></tr>
      {/each}
    </tbody>
  </table>

  <div class="mt-3 flex items-center gap-2 text-xs">
    <h3 class="text-callout font-semibold">Body</h3>
    <span class="text-muted-foreground">{formatBytes(body.size)}{body.content_type ? `, ${body.content_type.split(";")[0]}` : ""}</span>
    {#if body.truncated}
      <Tip text="Only the first {formatBytes(body.stored)} were stored">
        {#snippet children({ props })}<Badge {...{ ...props, tabindex: undefined }} variant="warning">Truncated at {formatBytes(body.stored)}</Badge>{/snippet}
      </Tip>
    {/if}
    {#if body.decoded_from}<Badge>Decoded {body.decoded_from}{body.decode_partial ? " (partial)" : ""}</Badge>{/if}
    <span class="flex-1"></span>
    {#if body.stored > 0}
      <Segmented label="Body view" items={modes} bind:value={() => effectiveMode, (v) => (mode = v)} />
    {/if}
  </div>
  {#if body.stored === 0}
    <div class="mt-2 rounded-lg bg-(--code-bg) p-3.5 text-callout text-muted-foreground">{body.size > 0 ? `${formatBytes(body.size)} went through, but this body wasn't stored (capture limit is 0 or the stream isn't captured).` : "Empty body"}</div>
  {:else if effectiveMode === "hex"}
    <pre class="{code} overflow-x-auto whitespace-pre">{hexDump(base64ToBytes(body.base64))}</pre>
  {:else if effectiveMode === "pretty" && pretty}
    <pre class="{code} break-all whitespace-pre-wrap">{pretty}</pre>
  {:else}
    <pre class="{code} break-all whitespace-pre-wrap">{body.text}</pre>
  {/if}
{/snippet}
