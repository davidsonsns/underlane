<script lang="ts">
  import { untrack, type Snippet } from "svelte";
  import type { HostnameDns } from "../lib/types";
  import { DNS_CHECK_INTERVAL_MS, readDnsCache, dnsCheckDue, markDnsCheck, saveDnsResult } from "$lib/dns-cache";
  import { DropdownMenu } from "bits-ui";
  import Ellipsis from "@lucide/svelte/icons/ellipsis";
  import { cn } from "$lib/utils";
  import { menuContent, menuItem } from "$lib/components/ui/mac";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { api, errorText } from "../lib/api";
  import { returnFocusTo } from "$lib/focus";
  import * as Dialog from "$lib/components/ui/dialog/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import { Label } from "$lib/components/ui/label/index.js";
  import { Badge } from "$lib/components/ui/badge/index.js";
  import { Spinner } from "$lib/components/ui/spinner/index.js";

  let { id, hostname, service }: { id: string; hostname: string; service: Snippet } = $props();
  let result = $state<HostnameDns | null>(untrack(() => readDnsCache(JSON.stringify([id, hostname])) ?? null));
  type Action = "checking" | "creating" | "connecting";
  let busy = $state<Action | null>(null);
  let connectOpen = $state(false);
  let token = $state("");
  let connectError = $state<string | null>(null);
  let connectTrigger = $state<HTMLButtonElement | null>(null);
  const uid = $props.id();
  const plainText = { autocorrect: "off", autocapitalize: "off", spellcheck: false, writingsuggestions: false } as const;
  let check = async () => {};
  let create = async () => {};
  let connect = async () => {};

  function closeConnect(value: boolean) {
    if (busy === "connecting") return;
    connectOpen = value;
    token = "";
    connectError = null;
  }

  function openTokenPage(e: MouseEvent) {
    e.preventDefault();
    const targetId = id;
    const targetHostname = hostname;
    void openUrl("https://dash.cloudflare.com/profile/api-tokens").catch((e) => {
      if (connectOpen && id === targetId && hostname === targetHostname) connectError = errorText(e);
    });
  }

  $effect(() => {
    const targetId = id;
    const targetHostname = hostname;
    const cacheKey = JSON.stringify([targetId, targetHostname]);
    let alive = true;
    let pending = false;
    result = readDnsCache(cacheKey) ?? null;
    busy = null;
    connectOpen = false;
    token = "";
    connectError = null;

    const run = async (action: Action = "checking", credential = "", automatic = false) => {
      if (!alive || pending || (action === "checking" && document.visibilityState !== "visible")) return;
      if (action === "connecting" && !credential) return;
      if (automatic && !dnsCheckDue(cacheKey)) {
        result = readDnsCache(cacheKey) ?? result ?? { status: "unknown", message: "", can_create: false, needs_dns_access: false };
        return;
      }
      const attemptedAt = Date.now();
      markDnsCheck(cacheKey, attemptedAt);
      pending = true;
      busy = action;
      if (action === "connecting") connectError = null;
      try {
        const next = action === "connecting"
          ? await api.connectHostnameDns(targetId, targetHostname, credential)
          : action === "creating"
            ? await api.createHostnameDns(targetId, targetHostname)
            : await api.checkHostnameDns(targetId, targetHostname);
        saveDnsResult(cacheKey, next, attemptedAt);
        if (alive) {
          result = next;
          if (action === "connecting") {
            token = "";
            connectOpen = false;
          }
        }
      } catch (e) {
        const previous = readDnsCache(cacheKey);
        const failed: HostnameDns = { status: "unknown", message: errorText(e), can_create: previous?.can_create ?? false, needs_dns_access: previous?.needs_dns_access ?? false };
        saveDnsResult(cacheKey, failed, attemptedAt);
        if (alive) {
          result = failed;
          if (action === "connecting") connectError = errorText(e);
        }
      } finally {
        pending = false;
        if (alive) busy = null;
      }
    };
    check = () => run();
    create = () => run("creating");
    connect = () => run("connecting", token.trim());
    const automaticCheck = () => run("checking", "", true);
    untrack(automaticCheck);
    const timer = setInterval(() => void automaticCheck(), DNS_CHECK_INTERVAL_MS);
    const visible = () => void automaticCheck();
    document.addEventListener("visibilitychange", visible);
    window.addEventListener("focus", visible);
    return () => {
      alive = false;
      token = "";
      connectOpen = false;
      clearInterval(timer);
      document.removeEventListener("visibilitychange", visible);
      window.removeEventListener("focus", visible);
    };
  });

  const labels = {
    connected: "DNS verified",
    missing: "DNS missing",
    conflict: "DNS needs attention",
    unknown: "DNS target unverified",
  };
</script>

<div class="flex shrink-0 items-center justify-self-end gap-3">
  <span class="inline-flex items-center" role="status" aria-live="polite" aria-busy={busy !== null}>
    <Badge variant={result?.status === "connected" ? "ok" : result?.status === "missing" || result?.status === "conflict" ? "warning" : "default"}>
      {#if busy === "creating"}Creating DNS record…{:else if busy === "connecting"}Connecting DNS…{:else}{result ? labels[result.status] : "DNS"}{/if}
      {#if busy !== null || !result}
        <Spinner class="text-current" role="presentation" aria-hidden="true" />
      {/if}
    </Badge>
  </span>
  {@render service()}
  <DropdownMenu.Root>
    <DropdownMenu.Trigger>
      {#snippet child({ props })}
        <Button {...props} bind:ref={connectTrigger} variant="ghost" size="icon-sm" aria-label="DNS actions for {hostname}">
          <Ellipsis class="size-3.5" />
        </Button>
      {/snippet}
    </DropdownMenu.Trigger>
    <DropdownMenu.Portal>
      <DropdownMenu.Content
        align="end"
        sideOffset={3}
        class={cn(menuContent, "min-w-40")}
        onCloseAutoFocus={(event) => { if (connectOpen) event.preventDefault(); }}
      >
        {#if result?.can_create && result.status === "missing" && !result.needs_dns_access}
          <DropdownMenu.Item class={menuItem} disabled={busy !== null} onSelect={() => create()}>Create DNS record</DropdownMenu.Item>
        {/if}
        <DropdownMenu.Item class={menuItem} disabled={busy !== null} onSelect={() => check()}>Check DNS</DropdownMenu.Item>
        {#if result?.needs_dns_access}
          <DropdownMenu.Item class={menuItem} disabled={busy !== null} onSelect={() => closeConnect(true)}>Use API token</DropdownMenu.Item>
        {/if}
      </DropdownMenu.Content>
    </DropdownMenu.Portal>
  </DropdownMenu.Root>
</div>

<Dialog.Root bind:open={() => connectOpen, closeConnect}>
  <Dialog.Content
    onCloseAutoFocus={returnFocusTo(() => connectTrigger)}
    onEscapeKeydown={(e) => { if (busy === "connecting") e.preventDefault(); }}
    onInteractOutside={(e) => { if (busy === "connecting") e.preventDefault(); }}
  >
    <Dialog.Header>
      <Dialog.Title>Optional DNS access</Dialog.Title>
      <Dialog.Description>Underlane uses your existing cloudflared login to verify DNS automatically. If that login cannot read this domain, you can use an API token with Zone Read and DNS Read for this domain.</Dialog.Description>
    </Dialog.Header>
    <p class="select-text wrap-anywhere text-callout">{hostname}</p>
    <a href="https://dash.cloudflare.com/profile/api-tokens" onclick={openTokenPage} class="text-link underline-offset-2 hover:underline">Create an API token</a>
    <form class="grid gap-4" aria-busy={busy === "connecting"} onsubmit={(e) => { e.preventDefault(); void connect(); }}>
      <div class="grid gap-2">
        <Label for="{uid}-dns-token">Cloudflare API token</Label>
        <Input id="{uid}-dns-token" type="password" {...plainText} autocomplete="off" bind:value={token} required disabled={busy === "connecting"} aria-describedby="{uid}-dns-token-help" />
        <p id="{uid}-dns-token-help" class="text-xs text-muted-foreground">Stored securely in your macOS Keychain. Connecting verifies access; creating a DNS record is a separate action.</p>
      </div>
      {#if connectError}<p class="select-text text-callout wrap-anywhere text-bad-text" role="alert">{connectError}</p>{/if}
      <Dialog.Footer>
        <Button variant="ghost" disabled={busy === "connecting"} onclick={() => closeConnect(false)}>Cancel</Button>
        <Button type="submit" disabled={busy !== null || !token.trim()}>{busy === "connecting" ? "Connecting…" : "Verify DNS"}</Button>
      </Dialog.Footer>
    </form>
  </Dialog.Content>
</Dialog.Root>
