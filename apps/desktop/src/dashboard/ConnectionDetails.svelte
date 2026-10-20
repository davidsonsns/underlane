<script lang="ts">
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import { onDestroy } from "svelte";
  import { api, errorText } from "$lib/api";
  import { Button } from "$lib/components/ui/button/index.js";
  import {
    Content as DialogContent,
    Description as DialogDescription,
    Footer as DialogFooter,
    Header as DialogHeader,
    Root as DialogRoot,
    Title as DialogTitle,
  } from "$lib/components/ui/dialog/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import { Label } from "$lib/components/ui/label/index.js";
  import {
    checkedTime,
    currentConnection,
    routePresentation,
    safeTestPath,
  } from "$lib/connection";
  import { returnFocusTo } from "$lib/focus";
  import { store } from "$lib/store.svelte";
  import type { DnsRepairPreview, RouteConnection, Tunnel } from "$lib/types";

  let { tunnel, route }: { tunnel: Tunnel; route: RouteConnection } = $props();
  let expanded = $state(false);
  let busy = $state(false);
  let diagnosticError = $state<string | null>(null);
  let repairOpen = $state(false);
  let preview = $state<DnsRepairPreview | null>(null);
  let repairTrigger = $state<HTMLButtonElement | null>(null);
  let testPath = $state("");
  let now = $state(Date.now());
  const timer = setInterval(() => {
    now = Date.now();
  }, 10_000);
  let alive = true;
  onDestroy(() => {
    alive = false;
    clearInterval(timer);
  });
  const uid = $props.id();
  const presentation = $derived(routePresentation(tunnel, route, now));
  const connection = $derived(currentConnection(tunnel));
  const checking = $derived(busy || route.checking || !!connection?.checking);
  const previewExpired = $derived(
    preview
      ? now >=
          (preview.expires_at < 1e12
            ? preview.expires_at * 1000
            : preview.expires_at)
      : true
  );

  $effect(() => {
    testPath = route.test_path ?? "";
  });
  $effect(() => {
    if (route.public.checked_at) {
      now = Date.now();
    }
  });

  async function run(action: (id: string) => Promise<void>) {
    if (busy) {
      return;
    }
    const { id } = tunnel;
    const host = route.hostname;
    busy = true;
    diagnosticError = null;
    try {
      await action(id);
    } catch (cause) {
      if (alive && id === tunnel.id && host === route.hostname) {
        diagnosticError = errorText(cause);
      }
    } finally {
      if (alive && id === tunnel.id && host === route.hostname) {
        busy = false;
      }
    }
  }

  async function check(id: string, path?: string) {
    const next = await api.checkConnection(
      id,
      route.hostname,
      route.rule_index,
      path
    );
    if (alive && id === tunnel.id) {
      store.applyConnection(id, next);
    }
  }

  function review() {
    run(async (id) => {
      const next = await api.previewDnsRepair(id, route.hostname);
      if (alive && id === tunnel.id && next.hostname === route.hostname) {
        preview = next;
        repairOpen = true;
      }
    });
  }

  function confirmRepair() {
    if (!preview || previewExpired) {
      return;
    }
    const reviewId = preview.review_id;
    run(async (id) => {
      await api.repairDns(id, reviewId);
      if (!alive || id !== tunnel.id) {
        return;
      }
      repairOpen = false;
      preview = null;
      await check(id);
    });
  }

  function closeRepair(value: boolean) {
    if (busy) {
      return;
    }
    repairOpen = value;
    if (!value) {
      preview = null;
    }
  }

  function checkPath(event: SubmitEvent) {
    event.preventDefault();
    const path = testPath.trim();
    if (path && !safeTestPath(path)) {
      diagnosticError =
        "Enter a path starting with /, without a hostname, fragment, spaces, or backslashes.";
      return;
    }
    run((id) => check(id, path || undefined));
  }
</script>

<div
  class="col-span-full min-w-0 pb-1"
  data-connection-hostname={route.hostname}
>
  <div
    class="mt-2 flex flex-wrap items-start gap-3 rounded-lg p-3 {presentation.tone ===
    "warning"
      ? "bg-degraded/10"
      : "bg-fill"}"
    role="status"
  >
    {#if presentation.tone === "warning"}
      <TriangleAlert
        aria-hidden="true"
        class="mt-0.5 size-4 shrink-0 text-degraded-text"
      />
    {/if}
    <div class="min-w-0 flex-1 basis-60">
      <p class="text-callout font-medium">{presentation.title}</p>
      <p
        class="mt-1 text-xs leading-relaxed text-muted-foreground wrap-anywhere"
      >
        {presentation.message}
      </p>
    </div>
    {#if presentation.action === "repair"}
      <Button disabled={checking} onclick={review} bind:ref={repairTrigger}
        >Repair DNS…</Button
      >
    {:else if route.dns.status === "missing" &&
      route.dns.can_create &&
      !route.dns.needs_dns_access}
      <Button
        disabled={checking}
        onclick={() =>
          run(async (id) => {
            await api.createHostnameDns(id, route.hostname);
            await check(id);
          })}
        >Create DNS record</Button
      >
    {/if}
  </div>
  <div class="flex flex-wrap items-center justify-between gap-2 pt-2">
    <Button
      aria-controls="{uid}-checks"
      aria-expanded={expanded}
      onclick={() => {
        expanded = !expanded;
      }}
      size="sm"
      variant="ghost"
    >
      <ChevronDown
        aria-hidden="true"
        class="size-3.5 {expanded ? "rotate-180" : ""}"
      />{expanded ? "Hide connection details" : "Connection details"}
    </Button>
    <span class="text-2xs text-muted-foreground"
      >{checking ? "Checking…" : checkedTime(connection?.checked_at)}</span
    >
  </div>
  {#if expanded}
    <div class="mt-2 border-t border-border pt-3" id="{uid}-checks">
      <dl class="grid gap-3 text-xs">
        <div class="grid gap-1 sm:grid-cols-[8rem_1fr]">
          <dt class="font-medium">DNS record</dt>
          <dd class="text-muted-foreground wrap-anywhere">
            {route.dns.message}<span class="mt-1 block text-2xs"
              >{route.dns.source ?? "DNS check"}
              · {checkedTime(route.dns.checked_at)}</span
            >
          </dd>
        </div>
        <div class="grid gap-1 sm:grid-cols-[8rem_1fr]">
          <dt class="font-medium">Connector</dt>
          <dd class="text-muted-foreground">
            {tunnel.state.status === "connected"
              ? `Connected to Cloudflare · ${tunnel.state.connections} connections`
              : `Connector ${tunnel.state.status}`}
            {#if connection?.config_changed}
              <span class="block text-degraded-text"
                >Config changed. Restart to apply it.</span
              >
            {/if}
          </dd>
        </div>
        <div class="grid gap-1 sm:grid-cols-[8rem_1fr]">
          <dt class="font-medium">Local service</dt>
          <dd class="text-muted-foreground wrap-anywhere">
            <span class="block font-mono select-text">{route.service}</span>
            {route.local.message}<span class="mt-1 block text-2xs"
              >{checkedTime(route.local.checked_at)}</span
            >
          </dd>
        </div>
        <div class="grid gap-1 sm:grid-cols-[8rem_1fr]">
          <dt class="font-medium">Public endpoint</dt>
          <dd class="text-muted-foreground wrap-anywhere">
            {#if route.public.status === "verified"}
              2xx response
            {:else if route.public.status === "auth_required" ||
              route.public.status === "responded"}
              Responded · application verification incomplete
            {:else}
              {route.public.status.replaceAll("_", " ")}
            {/if}
            {route.public.http_status
              ? ` · HTTP ${route.public.http_status}`
              : ""}
            <p>{route.public.message}</p>
            {#if route.public.test_url}
              <span class="block font-mono select-text"
                >{route.public.test_url}</span
              >
            {/if}
            <span class="mt-1 block text-2xs"
              >{checkedTime(route.public.checked_at)}</span
            >
          </dd>
        </div>
      </dl>
      <form class="mt-4 flex flex-wrap items-end gap-2" onsubmit={checkPath}>
        <div class="grid min-w-40 flex-1 gap-1.5">
          <Label for="{uid}-path">Public test path</Label>
          <Input
            aria-describedby="{uid}-path-help"
            autocomplete="off"
            disabled={checking}
            id="{uid}-path"
            placeholder={route.path ? "Enter a path matching this rule" : "/"}
            spellcheck={false}
            bind:value={testPath}
          />
        </div>
        <Button disabled={checking} type="submit" variant="outline"
          >{checking ? "Checking…" : "Check connection"}</Button
        >
        <p class="w-full text-2xs text-muted-foreground" id="{uid}-path-help">
          Use a path served by this application, such as /health. It must match
          this hostname’s ingress rule.
        </p>
      </form>
      {#if connection?.error}
        <p class="mt-2 text-xs text-bad-text" role="alert">
          {connection.error}
        </p>
      {/if}
    </div>
  {/if}
  {#if diagnosticError && !repairOpen}
    <p class="mt-2 text-xs text-bad-text wrap-anywhere" role="alert">
      {diagnosticError}
    </p>
  {/if}
</div>

<DialogRoot bind:open={() => repairOpen, closeRepair}>
  <DialogContent
    onCloseAutoFocus={returnFocusTo(() => repairTrigger)}
    onEscapeKeydown={(event) => {
      if (busy) {
        event.preventDefault();
      }
    }}
    onInteractOutside={(event) => {
      if (busy) {
        event.preventDefault();
      }
    }}
  >
    <DialogHeader
      ><DialogTitle>Route this hostname to {tunnel.name}?</DialogTitle
      ><DialogDescription
        >Update the existing Cloudflare DNS record to use this project’s
        tunnel.</DialogDescription
      ></DialogHeader
    >
    {#if preview}
      <p class="font-medium select-text wrap-anywhere">{preview.hostname}</p>
      <dl class="grid gap-4 text-xs">
        <div>
          <dt class="mb-1 text-muted-foreground">
            Current tunnel{preview.current_project_name
              ? ` · ${preview.current_project_name}`
              : ""}
          </dt>
          <dd class="font-mono select-text wrap-anywhere">
            {preview.current_tunnel_id}
          </dd>
          <dd class="mt-1 text-muted-foreground select-text wrap-anywhere">
            {preview.current_target}
          </dd>
        </div>
        <div>
          <dt class="mb-1 text-muted-foreground">{tunnel.name}’s tunnel</dt>
          <dd class="font-mono select-text wrap-anywhere">
            {preview.expected_tunnel_id}
          </dd>
          <dd class="mt-1 text-muted-foreground select-text wrap-anywhere">
            {preview.expected_target}
          </dd>
        </div>
      </dl>
      <p class="rounded-md bg-degraded/10 p-3 text-xs leading-relaxed">
        Traffic for this hostname will move from the current tunnel{preview.current_project_name
          ? ` used by ${preview.current_project_name}`
          : ""}
        to this project. Requests to the previous project may be interrupted.
      </p>
      <p class="text-2xs text-muted-foreground">
        CNAME · Cloudflare proxy enabled
      </p>
      {#if previewExpired}
        <p class="text-xs text-bad-text" role="alert">
          This review expired. Cancel and review the current DNS record again.
        </p>
      {/if}
    {/if}
    {#if diagnosticError}
      <p class="text-xs text-bad-text wrap-anywhere" role="alert">
        {diagnosticError}
      </p>
    {/if}
    <DialogFooter
      ><Button
        disabled={busy}
        onclick={() => closeRepair(false)}
        variant="ghost"
        >Cancel</Button
      ><Button
        disabled={busy || !preview || previewExpired}
        onclick={confirmRepair}
        >{busy ? "Repairing…" : "Repair DNS"}</Button
      ></DialogFooter
    >
  </DialogContent>
</DialogRoot>
