<script lang="ts">
  import Ellipsis from "@lucide/svelte/icons/ellipsis";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { DropdownMenu } from "bits-ui";
  import { type Snippet, untrack } from "svelte";
  import Tip from "$lib/components/app/Tip.svelte";
  import { Badge } from "$lib/components/ui/badge/index.js";
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
  import { kbRing, menuContent, menuItem } from "$lib/components/ui/mac";
  import { Spinner } from "$lib/components/ui/spinner/index.js";
  import {
    DNS_CHECK_INTERVAL_MS,
    dnsCheckDue,
    markDnsCheck,
    readDnsCache,
    saveDnsResult,
  } from "$lib/dns-cache";
  import { returnFocusTo } from "$lib/focus";
  import { cn } from "$lib/utils";
  import { api, errorText } from "../lib/api";
  import type { HostnameDns } from "../lib/types";

  let {
    id,
    hostname,
    service,
    dns,
    active = true,
    useBackend = false,
    oncheck,
  }: {
    id: string;
    hostname: string;
    service: Snippet;
    dns?: HostnameDns;
    active?: boolean;
    useBackend?: boolean;
    oncheck?: () => Promise<void>;
  } = $props();
  let result = $state<HostnameDns | null>(
    untrack(() => readDnsCache(JSON.stringify([id, hostname])) ?? null)
  );
  type Action = "checking" | "creating" | "connecting";
  let busy = $state<Action | null>(null);
  let showInactiveResult = $state(false);
  let connectOpen = $state(false);
  let token = $state("");
  let connectError = $state<string | null>(null);
  let connectTrigger = $state<HTMLButtonElement | null>(null);
  const uid = $props.id();
  const plainText = {
    autocapitalize: "off",
    autocorrect: "off",
    spellcheck: false,
    writingsuggestions: false,
  } as const;
  let check: () => Promise<void> = async () => undefined;
  let create: () => Promise<void> = async () => undefined;
  let connect: () => Promise<void> = async () => undefined;
  const shared = $derived(useBackend || dns !== undefined);

  $effect(() => {
    if (shared) {
      result = dns ?? null;
    }
  });

  function closeConnect(value: boolean) {
    if (busy === "connecting") {
      return;
    }
    connectOpen = value;
    token = "";
    connectError = null;
  }

  function openTokenPage(e: MouseEvent) {
    e.preventDefault();
    const targetId = id;
    const targetHostname = hostname;
    openUrl("https://dash.cloudflare.com/profile/api-tokens").catch((cause) => {
      if (connectOpen && id === targetId && hostname === targetHostname) {
        connectError = errorText(cause);
      }
    });
  }

  $effect(() => {
    const targetId = id;
    const targetHostname = hostname;
    const sharedDiagnostics = shared;
    const automaticEnabled = active;
    const cacheKey = JSON.stringify([targetId, targetHostname]);
    let alive = true;
    let pending = false;
    if (!sharedDiagnostics) {
      result = readDnsCache(cacheKey) ?? null;
    }
    busy = null;
    showInactiveResult = false;
    connectOpen = false;
    token = "";
    connectError = null;

    function canRun(action: Action, credential: string) {
      if (!alive || pending) {
        return false;
      }
      if (action === "checking") {
        return document.visibilityState === "visible";
      }
      return action !== "connecting" || Boolean(credential);
    }
    function query(action: Action, credential: string) {
      if (action === "connecting") {
        return api.connectHostnameDns(targetId, targetHostname, credential);
      }
      if (action === "creating") {
        return api.createHostnameDns(targetId, targetHostname);
      }
      return api.checkHostnameDns(targetId, targetHostname);
    }
    function applyResult(next: HostnameDns, action?: Action) {
      if (!alive) {
        return;
      }
      result = next;
      if (action === "connecting") {
        token = "";
        connectOpen = false;
      }
    }
    async function runShared(action: Action, credential: string) {
      if (!canRun(action, credential)) {
        return;
      }
      pending = true;
      busy = action;
      connectError = null;
      try {
        if (action !== "checking") {
          applyResult(await query(action, credential), action);
        }
        await oncheck?.();
      } catch (cause) {
        if (alive) {
          connectError = errorText(cause);
        }
      } finally {
        pending = false;
        if (alive) {
          busy = null;
        }
      }
    }
    const run = async (
      action: Action = "checking",
      credential = "",
      automatic = false
    ) => {
      if (!canRun(action, credential)) {
        return;
      }
      if (automatic && !dnsCheckDue(cacheKey)) {
        result = readDnsCache(cacheKey) ??
          result ?? {
            can_create: false,
            message: "",
            needs_dns_access: false,
            status: "unknown",
          };
        return;
      }
      const attemptedAt = Date.now();
      markDnsCheck(cacheKey, attemptedAt);
      pending = true;
      busy = action;
      if (action === "connecting") {
        connectError = null;
      }
      try {
        const next = await query(action, credential);
        saveDnsResult(cacheKey, next, attemptedAt);
        applyResult(next, action);
      } catch (e) {
        const previous = readDnsCache(cacheKey);
        const failed: HostnameDns = {
          can_create: previous?.can_create ?? false,
          message: errorText(e),
          needs_dns_access: previous?.needs_dns_access ?? false,
          status: "unknown",
        };
        saveDnsResult(cacheKey, failed, attemptedAt);
        applyResult(failed);
        if (alive && action === "connecting") {
          connectError = errorText(e);
        }
      } finally {
        pending = false;
        if (alive) {
          busy = null;
        }
      }
    };
    check = () => {
      showInactiveResult = true;
      return sharedDiagnostics ? runShared("checking", "") : run();
    };
    create = () => {
      showInactiveResult = true;
      return sharedDiagnostics ? runShared("creating", "") : run("creating");
    };
    connect = () => {
      showInactiveResult = true;
      return sharedDiagnostics
        ? runShared("connecting", token.trim())
        : run("connecting", token.trim());
    };
    if (sharedDiagnostics) {
      return () => {
        alive = false;
      };
    }
    const automaticCheck = () => automaticEnabled && run("checking", "", true);
    untrack(automaticCheck);
    const timer = setInterval(() => automaticCheck(), DNS_CHECK_INTERVAL_MS);
    const visible = () => automaticCheck();
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

  const variants = {
    conflict: "warning",
    connected: "ok",
    missing: "warning",
    unknown: "default",
  } as const;
  const labels = {
    conflict: "DNS needs attention",
    connected: "DNS verified",
    missing: "DNS missing",
    unknown: "DNS target unverified",
  };
  const label = $derived.by(() => {
    if (busy === "creating") {
      return "Creating DNS record…";
    }
    if (busy === "connecting") {
      return "Connecting DNS…";
    }
    if (result?.status === "conflict" && result.reason === "wrong_tunnel") {
      return "Wrong tunnel";
    }
    return result ? labels[result.status] : "DNS";
  });
  const explanation = $derived(
    result?.message || "Checking this hostname’s DNS record…"
  );
</script>

{#snippet dnsTokenDialog()}
  <DialogRoot bind:open={() => connectOpen, closeConnect}>
    <DialogContent
      onCloseAutoFocus={returnFocusTo(() => connectTrigger)}
      onEscapeKeydown={(e) => {
        if (busy === "connecting") {
          e.preventDefault();
        }
      }}
      onInteractOutside={(e) => {
        if (busy === "connecting") {
          e.preventDefault();
        }
      }}
    >
      <DialogHeader>
        <DialogTitle>Optional DNS access</DialogTitle>
        <DialogDescription
          >Underlane uses your existing cloudflared login to verify DNS
          automatically. If that login cannot read this domain, you can use an
          API token with Zone Read and DNS Read for this
          domain.</DialogDescription
        >
      </DialogHeader>
      <p class="select-text wrap-anywhere text-callout">{hostname}</p>
      <a
        class="text-link underline-offset-2 hover:underline"
        href="https://dash.cloudflare.com/profile/api-tokens"
        onclick={openTokenPage}
        >Create an API token</a
      >
      <form
        aria-busy={busy === "connecting"}
        class="grid gap-4"
        onsubmit={(e) => {
          e.preventDefault();
          connect();
        }}
      >
        <div class="grid gap-2">
          <Label for="{uid}-dns-token">Cloudflare API token</Label>
          <Input
            id="{uid}-dns-token"
            type="password"
            {...plainText}
            aria-describedby="{uid}-dns-token-help"
            autocomplete="off"
            disabled={busy === "connecting"}
            required
            bind:value={token}
          />
          <p class="text-xs text-muted-foreground" id="{uid}-dns-token-help">
            Stored securely in your macOS Keychain. Connecting verifies access;
            creating a DNS record is a separate action.
          </p>
        </div>
        {#if connectError}
          <p
            class="select-text text-callout wrap-anywhere text-bad-text"
            role="alert"
          >
            {connectError}
          </p>
        {/if}
        <DialogFooter>
          <Button
            disabled={busy === "connecting"}
            onclick={() => closeConnect(false)}
            variant="ghost"
            >Cancel</Button
          >
          <Button disabled={busy !== null || !token.trim()} type="submit"
            >{busy === "connecting" ? "Connecting…" : "Verify DNS"}</Button
          >
        </DialogFooter>
      </form>
    </DialogContent>
  </DialogRoot>
{/snippet}

<div class="flex shrink-0 items-center justify-self-end gap-3">
  {#if (active || showInactiveResult) &&
    (result?.status === "conflict" || result?.status === "missing")}
    <span
      aria-busy={busy !== null}
      aria-live="polite"
      class="inline-flex items-center"
      role="status"
    >
      <Tip side="top" text={explanation}>
        {#snippet children({
          props,
        })}
          <button
            {...props}
            aria-label="{label} for {hostname}"
            class="inline-flex cursor-help rounded-[4px] {kbRing}"
            type="button"
          >
            <Badge variant={result ? variants[result.status] : "default"}>
              {label}
              {#if busy !== null || !result}
                <Spinner
                  aria-hidden="true"
                  class="text-current"
                  role="presentation"
                />
              {/if}
            </Badge>
          </button>
        {/snippet}
        {#snippet content()}
          <div
            class="grid max-w-72 gap-2 p-2 text-xs leading-relaxed wrap-anywhere"
          >
            <div>
              <p class="font-medium">{label}</p>
              <p class="text-2xs text-muted-foreground">{hostname}</p>
            </div>
            <p>{explanation}</p>
            <p
              class="border-t border-border pt-2 text-2xs text-muted-foreground"
            >
              Use “Check DNS” in the ⋯ menu to refresh.
            </p>
          </div>
        {/snippet}
      </Tip>
    </span>
  {/if}
  {@render service()}
  <DropdownMenu.Root>
    <DropdownMenu.Trigger>
      {#snippet child({
        props,
      })}
        <Button
          {...props}
          aria-label="DNS actions for {hostname}"
          size="icon-sm"
          variant="ghost"
          bind:ref={connectTrigger}
        >
          <Ellipsis class="size-3.5" />
        </Button>
      {/snippet}
    </DropdownMenu.Trigger>
    <DropdownMenu.Portal>
      <DropdownMenu.Content
        align="end"
        class={cn(menuContent, "min-w-40")}
        onCloseAutoFocus={(event) => {
          if (connectOpen) {
            event.preventDefault();
          }
        }}
        sideOffset={3}
      >
        {#if result?.status === "unknown"}
          <div class="max-w-64 px-2 py-1.5 text-xs" role="status">
            <p class="font-medium">{labels.unknown}</p>
            <p class="mt-1 text-muted-foreground">{explanation}</p>
          </div>
        {/if}
        {#if result?.can_create &&
          result.status === "missing" &&
          !result.needs_dns_access}
          <DropdownMenu.Item
            class={menuItem}
            disabled={busy !== null}
            onSelect={() => create()}
            >Create DNS record</DropdownMenu.Item
          >
        {/if}
        <DropdownMenu.Item
          class={menuItem}
          disabled={busy !== null}
          onSelect={() => check()}
          >Check DNS</DropdownMenu.Item
        >
        {#if result?.needs_dns_access}
          <DropdownMenu.Item
            class={menuItem}
            disabled={busy !== null}
            onSelect={() => closeConnect(true)}
            >Use API token</DropdownMenu.Item
          >
        {/if}
      </DropdownMenu.Content>
    </DropdownMenu.Portal>
  </DropdownMenu.Root>
</div>

{#if connectError && !connectOpen}
  <p class="col-span-full text-xs text-bad-text wrap-anywhere" role="alert">
    {connectError}
  </p>
{/if}
{@render dnsTokenDialog()}
