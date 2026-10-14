<script lang="ts">
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import X from "@lucide/svelte/icons/x";
  import type { Snippet } from "svelte";
  import CopyButton from "$lib/components/app/CopyButton.svelte";
  import IconAction from "$lib/components/app/IconAction.svelte";
  import Segmented from "$lib/components/app/Segmented.svelte";
  import Tip from "$lib/components/app/Tip.svelte";
  import { Badge } from "$lib/components/ui/badge/index.js";
  import {
    Content as CollapsibleContent,
    Root as CollapsibleRoot,
    Trigger as CollapsibleTrigger,
  } from "$lib/components/ui/collapsible/index.js";
  import {
    Content as TabsContent,
    List as TabsList,
    Root as TabsRoot,
    Trigger as TabsTrigger,
  } from "$lib/components/ui/tabs/index.js";
  import {
    base64ToBytes,
    formatBytes,
    formatClock,
    formatDuration,
    hexDump,
    jsonTokens,
    methodText,
    prettyJson,
    statusClass,
    toCurl,
  } from "../lib/format";
  import {
    type SectionId,
    sectionPrefs,
    setOpen,
  } from "../lib/sections.svelte";
  import type { BodyView, RequestDetail } from "../lib/types";

  type Side = "req" | "resp";
  type Mode = "pretty" | "raw" | "hex";
  let {
    detail,
    side = $bindable("resp"),
    mode = $bindable("pretty"),
    onclose,
  }: {
    detail: RequestDetail;
    /** The open tab; bound by Requests so it persists across selections. */
    side?: Side;
    /** The chosen body view. A body that can't show it uses `effectiveMode` and leaves it as is. */
    mode?: Mode;
    /** Closes the pane (the close button). */
    onclose: () => void;
  } = $props();

  const body = $derived<BodyView>(side === "req" ? detail.req : detail.resp);
  const headers = $derived(
    side === "req" ? detail.req_headers : detail.resp_headers
  );
  const pretty = $derived(body.text === null ? null : prettyJson(body.text));
  const effectiveMode = $derived.by<Mode>(() => {
    if (body.is_text) {
      if (mode === "pretty" && !pretty) {
        return "raw";
      }
      return mode;
    }
    return "hex";
  });
  const modes = $derived(
    body.is_text
      ? [
          { disabled: !pretty, label: "Pretty", value: "pretty" as const },
          { label: "Raw", value: "raw" as const },
          { label: "Hex", value: "hex" as const },
        ]
      : [{ label: "Hex", value: "hex" as const }]
  );
  const statusText = {
    client: "text-degraded-text",
    none: "",
    ok: "text-ok-text",
    redirect: "",
    server: "text-bad-text",
  };
  const tokenClass = {
    key: "text-syn-key",
    literal: "text-syn-literal",
    number: "text-syn-number",
    punct: "",
    string: "text-syn-string",
  };
  const code =
    "mt-2 rounded-lg bg-(--code-bg) px-3 py-2.5 font-mono text-code leading-normal";
</script>

<div class="flex h-full min-h-0 flex-col">
  <!-- One row: what was called on the left (the host is a muted prefix that gives way first in a
       narrow pane), how it went on the right. -->
  <div class="border-b-[0.5px] border-border ps-4 pe-2.5">
    <div class="flex h-10 items-center gap-2.5 text-callout">
      <span class={["shrink-0 font-bold", methodText(detail.method)]}
        >{detail.method}</span
      >
      <span
        class="flex min-w-0 flex-1 font-mono select-text"
        title="{detail.host}{detail.path}"
      >
        <span class="min-w-0 shrink-[100] truncate text-muted-foreground"
          >{detail.host}</span
        >
        <span class="min-w-0 truncate">{detail.path}</span>
      </span>
      <div class="flex shrink-0 items-center gap-2.5 tabular-nums">
        {#if detail.kind !== "http"}
          <Tip text="Streamed, so the body isn't stored">
            {#snippet children({
              props,
            })}
              <Badge {...{ ...props, tabindex: undefined }}
                >{detail.kind === "sse" ? "SSE" : "WebSocket"}</Badge
              >
            {/snippet}
          </Tip>
        {/if}
        <span class={["font-semibold", statusText[statusClass(detail.status)]]}
          >{detail.status ?? "No response"}</span
        >
        <span class="text-muted-foreground"
          >{formatDuration(detail.duration_ms)}</span
        >
        <span
          class="text-muted-foreground"
          title={new Date(detail.ts).toLocaleString()}
          >{formatClock(detail.ts)}</span
        >
      </div>
      <div class="flex shrink-0 items-center gap-0.5">
        <!-- "Copy as curl" is read by the e2e (02 §0.3): it stays a text button with this label. -->
        <CopyButton
          label="Copy as curl"
          tip={detail.req.truncated
            ? "The request body was truncated; the command includes only the stored part"
            : "Copy this request as a curl command"}
          value={() => toCurl(detail)}
          variant="ghost"
          what="curl command"
        />
        <IconAction label="Close" onclick={onclose} shortcut="Esc"
          ><X /></IconAction
        >
      </div>
    </div>
    {#if detail.error}
      <div class="-mt-1 pb-2.5 text-callout text-bad-text select-text">
        {detail.error}
      </div>
    {/if}
  </div>

  <TabsRoot
    class="flex min-h-0 flex-1 flex-col gap-0"
    bind:value={
      () => side,
      (v) => {
    side = v as Side;
  }
    }
  >
    <TabsList class="mx-4 mt-2">
      <TabsTrigger value="req">Request</TabsTrigger>
      <TabsTrigger value="resp">Response</TabsTrigger>
    </TabsList>
    <!-- Bits hides the inactive panel, and its body (or hex dump) isn't even computed. -->
    <TabsContent
      class="min-h-0 overflow-auto px-4 pt-1.5 pb-5"
      tabindex={-1}
      value="req"
      >{#if side === "req"}
        {@render panel()}
      {/if}</TabsContent
    >
    <TabsContent
      class="min-h-0 overflow-auto px-4 pt-1.5 pb-5"
      tabindex={-1}
      value="resp"
      >{#if side === "resp"}
        {@render panel()}
      {/if}</TabsContent
    >
  </TabsRoot>
</div>

<!-- A section heading that folds its content; which sections are folded is remembered (sections.svelte.ts). -->
{#snippet trigger(
  title: string,
  meta: string,
  metaClass = ""
)}
  <h3 class="text-callout">
    <CollapsibleTrigger
      class="group/sec -ms-1 flex items-center gap-1 rounded-sm px-1 outline-none focus-visible:ring-2 focus-visible:ring-ring"
    >
      <ChevronRight
        class="size-3.5 text-muted-foreground transition-transform duration-200 group-data-[state=open]/sec:rotate-90"
      />
      <span class="font-semibold">{title}</span>
      <span class={["whitespace-nowrap text-muted-foreground", metaClass]}
        >{meta}</span
      >
    </CollapsibleTrigger>
  </h3>
{/snippet}

{#snippet section(
  id: SectionId,
  head: Snippet,
  content: Snippet
)}
  <CollapsibleRoot
    class="mt-3"
    bind:open={
      () => !sectionPrefs.collapsed.includes(id),
      (v) => setOpen(id, v)
    }
  >
    {@render head()}
    <CollapsibleContent
      class="overflow-hidden data-[state=closed]:animate-collapsible-up data-[state=open]:animate-collapsible-down"
    >
      {@render content()}
    </CollapsibleContent>
  </CollapsibleRoot>
{/snippet}

{#snippet panel()}
  {@render section("headers", headersHead, headersTable)}
  {@render section("body", bodyHead, bodyContent)}
{/snippet}

{#snippet headersHead()}
  {@render trigger("Headers", String(headers.length))}
{/snippet}

{#snippet headersTable()}
  <table class="mt-1.5 w-full border-collapse font-mono text-code">
    <tbody>
      {#each headers as [k, v], i (i)}
        <tr>
          <!-- Names get about a third of the width (wrapping anywhere) so long values keep most of it. -->
          <th
            class="w-[34%] py-0.75 pe-2.5 text-left align-top font-medium wrap-anywhere text-muted-foreground"
          >
            {k}
          </th>
          <td
            class={[
              "py-0.75 break-all select-text",
              v === "[redacted]" && "text-muted-foreground italic",
            ]}
          >
            {v}
          </td>
        </tr>
      {:else}
        <tr>
          <td class="font-sans text-muted-foreground">No headers</td>
        </tr>
      {/each}
    </tbody>
  </table>
{/snippet}

{#snippet bodyHead()}
  <div class="flex flex-wrap items-center gap-x-2 gap-y-1.5 text-xs">
    {@render trigger(
      "Body",
      `${formatBytes(body.size)}${body.content_type ? `, ${body.content_type.split(";")[0]}` : ""}`,
      "text-xs"
    )}
    {#if body.truncated}
      <Tip text="Only the first {formatBytes(body.stored)} were stored">
        {#snippet children({
          props,
        })}
          <Badge {...{ ...props, tabindex: undefined }} variant="warning"
            >Truncated at {formatBytes(body.stored)}</Badge
          >
        {/snippet}
      </Tip>
    {/if}
    {#if body.decoded_from}
      <Badge
        >Decoded {body.decoded_from}{body.decode_partial
          ? " (partial)"
          : ""}</Badge
      >
    {/if}
    {#if body.stored > 0 && !sectionPrefs.collapsed.includes("body")}
      <!-- ms-auto keeps it on the right, also when it wraps under the badges in a narrow pane. -->
      <Segmented
        class="ms-auto"
        items={modes}
        label="Body view"
        bind:value={
          () => effectiveMode,
          (v) => {
    mode = v;
  }
        }
      />
    {/if}
  </div>
{/snippet}

{#snippet bodyContent()}
  {#if body.stored === 0}
    <div
      class="mt-2 rounded-lg bg-(--code-bg) p-3.5 text-callout text-muted-foreground"
    >
      {body.size > 0
        ? `${formatBytes(body.size)} went through, but this body wasn't stored (capture limit is 0 or the stream isn't captured).`
        : "Empty body"}
    </div>
  {:else if effectiveMode === "hex"}
    <pre
      class="{code} overflow-x-auto whitespace-pre"
    >{hexDump(base64ToBytes(body.base64))}</pre>
  {:else if effectiveMode === "pretty" && pretty}
    <pre
      class="{code} break-all whitespace-pre-wrap"
    >{#each jsonTokens(pretty) as t, i (i)}<span class={tokenClass[t.kind]}>{t.text}</span>{/each}</pre>
  {:else}
    <pre class="{code} break-all whitespace-pre-wrap">{body.text}</pre>
  {/if}
{/snippet}
