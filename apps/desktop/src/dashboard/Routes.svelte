<script lang="ts">
  // A tunnel's ingress rules as an aligned table: public hostname, then the local service.
  // Each service is probed while the window is visible so it's obvious which ones are running.
  import Play from "@lucide/svelte/icons/play";
  import Square from "@lucide/svelte/icons/square";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { mergeProps } from "bits-ui";
  import CopyButton from "$lib/components/app/CopyButton.svelte";
  import IconAction from "$lib/components/app/IconAction.svelte";
  import Tip from "$lib/components/app/Tip.svelte";
  import { Button } from "$lib/components/ui/button/index.js";
  import { kbRing } from "$lib/components/ui/mac";
  import { Spinner } from "$lib/components/ui/spinner/index.js";
  import { currentConnection, routeFixAction } from "$lib/connection";
  import { activeTunnelOwner } from "$lib/tunnel-conflict";
  import { api, errorText } from "../lib/api";
  import {
    isOn,
    reachSummary,
    serviceLabel,
    shortService,
  } from "../lib/format";
  import { store } from "../lib/store.svelte";
  import { notify } from "../lib/toast";
  import type {
    RouteMapping,
    RouteTarget,
    ServiceCheck,
    Tunnel,
  } from "../lib/types";
  import ConnectionDetails from "./ConnectionDetails.svelte";
  import DnsStatus from "./DnsStatus.svelte";
  import { type Reach, routeReach, routeReachTip } from "./view-state";

  let HostnameSheet = $state<
    typeof import("./HostnameSheet.svelte").default | null
  >(null);
  let manageTrigger = $state<HTMLButtonElement | null>(null);
  let manageOpen = $state(false);
  let manageLoading = $state(false);
  let manageTarget = $state<{
    id: string;
    name: string;
    running: boolean;
  } | null>(null);
  let restartWarning = $state<{ id: string; message: string } | null>(null);
  let retryingRestart = $state(false);

  let {
    tunnel,
    operationBusy = false,
  }: { tunnel: Tunnel; operationBusy?: boolean } = $props();
  let pendingHostname = $state<{ id: string; key: string } | null>(null);
  const PROBE_EVERY_MS = 3000;
  const HTTP_SERVICE = /^https?:\/\//;

  const s = $derived(tunnel.state);
  const connection = $derived(currentConnection(tunnel));
  const sharedDiagnostics = $derived("connection" in tunnel);
  const live = $derived(s.status === "connected" || s.status === "degraded");
  const on = $derived(isOn(tunnel));
  const tunnelOwner = $derived(activeTunnelOwner(tunnel, store.tunnels));

  function controlReason(m: RouteMapping): string | null {
    if (m.control_supported === false) {
      return (
        m.note || "Individual controls are unavailable for this configuration."
      );
    }
    if (pendingHostname || operationBusy) {
      return "Wait for the current operation to finish.";
    }
    if (on && s.config_revision === 0) {
      return "Waiting for the connector to start…";
    }
    if (tunnel.setup_needed) {
      return "Set up this project's Cloudflare tunnel first.";
    }
    if (tunnel.separating_tunnel || tunnel.separate_tunnel_pending) {
      return "Finish creating this project's separate tunnel first.";
    }
    if (!store.overview?.cloudflared) {
      return "Install cloudflared first.";
    }
    if (!on && tunnelOwner) {
      return `${tunnelOwner.name} is already using this Cloudflare tunnel.`;
    }
    return null;
  }

  async function toggleHostname(m: RouteMapping) {
    if (!m.route_key || controlReason(m)) {
      return;
    }
    // Pin the identity and generation before awaiting; sidebar selection must not retarget the action.
    const { id } = tunnel;
    const target: RouteTarget = {
      config_revision: on ? s.config_revision : 0,
      key: { ...m.route_key },
      run: on ? s.run : 0,
    };
    const stopping = on && m.enabled !== false;
    pendingHostname = { id, key: JSON.stringify(target.key) };
    try {
      await (stopping
        ? api.stopHostname(id, target)
        : api.startHostname(id, target));
      store.scheduleRefresh();
    } catch (error) {
      notify.error(
        error,
        `Couldn't ${stopping ? "stop" : "start"} ${target.key.hostname}`
      );
    } finally {
      pendingHostname = null;
    }
  }

  async function manage() {
    if (manageLoading) {
      return;
    }
    // Freeze the target before the lazy load; a sidebar selection must never retarget a draft.
    manageTarget = {
      id: tunnel.id,
      name: tunnel.name,
      running: live || s.status === "starting",
    };
    manageLoading = true;
    try {
      HostnameSheet ??= (await import("./HostnameSheet.svelte")).default;
      manageOpen = true;
    } catch (e) {
      notify.error(e, "Couldn't open hostname editor");
    } finally {
      manageLoading = false;
    }
  }

  async function saved(id: string, restartError: string | null) {
    restartWarning = restartError ? { id, message: restartError } : null;
    await store.refresh();
    if (!restartError) {
      notify.success("Hostnames saved.");
    }
  }

  async function retryRestart() {
    if (!restartWarning || retryingRestart) {
      return;
    }
    const { id } = restartWarning;
    retryingRestart = true;
    try {
      await api.restart(id);
      restartWarning = null;
      await store.refresh();
    } catch (e) {
      restartWarning = { id, message: errorText(e) };
    } finally {
      retryingRestart = false;
    }
  }
  const routes: RouteMapping[] = $derived.by(() => {
    if (s.mappings.length) {
      return s.mappings;
    }
    if (tunnel.kind === "quick" && tunnel.origin_url) {
      return [
        {
          hostname: null,
          inspected: false,
          note: null,
          path: null,
          service: tunnel.origin_url,
        },
      ];
    }
    return [];
  });
  const services = $derived([...new Set(routes.map((m) => m.service))]);
  const servicesKey = $derived(services.join("\n"));

  const headingId = $props.id();

  /** service -> listening? (null: nothing to probe, e.g. http_status:404) */
  let reach = $state<Record<string, boolean | null>>({});

  $effect(() => {
    if (sharedDiagnostics) {
      return;
    }
    const list = servicesKey ? servicesKey.split("\n") : [];
    if (!list.length) {
      return;
    }
    let alive = true;
    const check = async () => {
      if (document.visibilityState !== "visible") {
        return;
      }
      try {
        const res = await api.probeServices(list);
        if (alive) {
          reach = {
            ...reach,
            ...Object.fromEntries(list.map((svc, i) => [svc, res[i]])),
          };
        }
      } catch {
        // A failed probe leaves the last known state; the next tick tries again.
      }
    };
    check();
    const timer = setInterval(check, PROBE_EVERY_MS);
    document.addEventListener("visibilitychange", check);
    return () => {
      alive = false;
      clearInterval(timer);
      document.removeEventListener("visibilitychange", check);
    };
  });

  const reachOf = (service: string, local?: ServiceCheck) => {
    if (!HTTP_SERVICE.test(service)) {
      return routeReach(null, live);
    }
    if (!local) {
      return routeReach(reach[service], live);
    }
    if (local.status === "unsupported") {
      return routeReach(undefined, live);
    }
    return routeReach(local.status === "reachable", live);
  };
  const reachTip = routeReachTip;

  // Counted per route, not per distinct service, so the summary matches the rows below it.
  const probed = $derived(
    routes.filter((m) => typeof reach[m.service] === "boolean")
  );
  const upCount = $derived(probed.filter((m) => reach[m.service]).length);
  // Local probes cannot establish public DNS or end-to-end hostname reachability.
  const summary = $derived(
    !sharedDiagnostics && probed.length
      ? reachSummary(upCount, probed.length, false)
      : null
  );

  // Links open in the default browser; the webview itself never navigates away.
  function open(e: MouseEvent, url: string) {
    e.preventDefault();
    openUrl(url);
  }

  const dot: Record<Reach, string> = {
    down: "bg-bad",
    idle: "border border-off", // Same hollow ring as a stopped StatusDot.
    none: "invisible", // keeps the column width so services stay aligned
    ready: "bg-off",
    unknown: "border border-off",
    up: "bg-ok",
  };
  const link = `cursor-pointer select-none rounded-xs hover:underline ${kbRing}`;

  function hostOf(m: (typeof routes)[number]): string | null {
    if (tunnel.kind === "quick") {
      return s.public_url?.replace("https://", "") ?? null;
    }
    return m.hostname;
  }
</script>
{#snippet localService(
  m: (typeof routes)[number],
  r: Reach,
  tip: string | undefined
)}
  <span
    class="flex items-center gap-1.75 justify-self-end text-callout whitespace-nowrap text-muted-foreground"
  >
    <Tip disabled={!tip} text={tip ?? ""}>
      {#snippet children({
        props,
      })}
        <!-- The dot itself is the trigger (no extra button); ::before widens the 8px hover target. -->
        <span
          {...{ ...props, tabindex: undefined }}
          aria-label={tip}
          class="relative size-2 flex-none rounded-full transition-[background-color,box-shadow] duration-240 before:absolute before:-inset-1 {dot[
            r
          ]}"
          role="img"
        ></span>
      {/snippet}
    </Tip>
    <!-- The service carries the same explanation, so hovering it, or Tab to its link, shows it too (as before). -->
    <Tip disabled={!tip} text={tip ?? ""}>
      {#snippet children({
        props,
      })}
        {#if /^https?:\/\//.test(m.service)}
          <a
            {...mergeProps(props, {
              onclick: (e: MouseEvent) => open(e, m.service),
            })}
            class="font-mono {link}"
            href={m.service}
            >{shortService(m.service)}</a
          >
        {:else}
          <span
            {...{ ...props, tabindex: undefined }}
            class={serviceLabel(m.service) === shortService(m.service)
              ? "font-mono select-text"
              : undefined}
            >{serviceLabel(m.service)}</span
          >
        {/if}
      {/snippet}
    </Tip>
  </span>
{/snippet}

{#snippet unlinkedHostname(
  m: (typeof routes)[number],
  host: string | null
)}
  <span
    class="truncate select-text {host
      ? "text-foreground"
      : "text-muted-foreground"}"
    title={host ? `${host}${m.path ?? ""}` : undefined}
  >
    {#if host}
      {host}
    {:else if tunnel.kind === "quick"}
      Assigned when connected
    {:else}
      Any other hostname
    {/if}
    {#if m.path}
      <span class="text-muted-foreground">{m.path}</span>
    {/if}
  </span>
{/snippet}

{#snippet routeHeading()}
  <div class="mb-3 ms-3.5 flex flex-wrap items-center gap-3">
    <h2 class="text-sm font-medium" id={headingId}>Hostnames</h2>
    {#if summary}
      <span
        class="text-xs {live && upCount < probed.length
          ? "text-bad-text"
          : "text-muted-foreground"}"
        role="status"
        >{summary}</span
      >
    {/if}
    {#if tunnel.kind === "config"}
      <Button
        class="ms-auto"
        disabled={manageLoading}
        onclick={manage}
        variant="outline"
        bind:ref={manageTrigger}
        >Manage hostnames</Button
      >
    {/if}
  </div>
{/snippet}

{#snippet routeRow(
  m: (typeof routes)[number],
  index: number
)}
  {@const host = hostOf(m)}
  {@const diagnostic = connection?.routes.find(
    (route) =>
      route.rule_index === index &&
      route.hostname === host &&
      route.path === m.path &&
      route.service === m.service
  )}
  {@const r = reachOf(m.service, diagnostic?.local)}
  {@const stopped = !on || m.enabled === false}
  {@const tip =
    m.enabled === false && r !== "none" && r !== "unknown"
      ? `Local service ${r === "up" || r === "ready" ? "is listening" : "is not listening"}. This hostname is stopped. Underlane doesn't manage the local service.`
      : reachTip(m.service, r)}
  {@const linkable = live && !stopped && host && !host.includes("*")}

  <li
    class="routes-row min-h-11 items-center py-2.5 not-first:border-t not-first:border-border"
    data-route-hostname={host ?? undefined}
  >
    <div class="flex min-w-0 items-center gap-2">
      {@render routeHostname(m, host, linkable)}
      {#if tunnel.kind === "config" && host && m.route_key}
        {@const pending =
          pendingHostname?.id === tunnel.id &&
          pendingHostname.key === JSON.stringify(m.route_key)}
        {@const label = `${stopped ? "Start" : "Stop"} ${host}${m.path ?? ""}`}
        <!-- Unavailable controls stay focusable so the tooltip can explain why. -->
        <IconAction
          aria-busy={pending}
          aria-disabled={!!controlReason(m)}
          class="shrink-0 aria-disabled:[&_svg]:opacity-45"
          {label}
          onclick={() => toggleHostname(m)}
          size="icon-sm"
          tip={controlReason(m) ?? label}
        >
          {#if pending}
            <Spinner aria-hidden="true" role="presentation" />
          {:else if stopped}
            <Play class="size-3 fill-current" />
          {:else}
            <Square class="size-2.5 fill-current" />
          {/if}
        </IconAction>
      {/if}
    </div>
    {#snippet service()}
      {@render localService(m, stopped && r !== "none" ? "idle" : r, tip)}
    {/snippet}
    {#if tunnel.kind === "config" && host}
      <DnsStatus
        active={!stopped}
        dns={diagnostic?.dns}
        hostname={host}
        id={tunnel.id}
        oncheck={sharedDiagnostics
          ? async () => {
              const { id } = tunnel;
              const next = await api.checkConnection(id, host, index);
              if (id === tunnel.id) {
                store.applyConnection(id, next);
              }
            }
          : undefined}
        {service}
        useBackend={sharedDiagnostics}
      />
    {:else}
      {@render service()}
    {/if}
    {#if diagnostic && routeFixAction(tunnel, diagnostic)}
      <ConnectionDetails route={diagnostic} {tunnel} />
    {/if}
    {#if m.note && tunnel.inspect}
      <span class="col-span-full mt-0.5 text-xs text-muted-foreground"
        >{m.note}</span
      >
    {/if}
  </li>
{/snippet}

{#snippet routeHostname(
  m: (typeof routes)[number],
  host: string | null,
  linkable: boolean | string | null
)}
  <span class="flex min-w-0 items-center gap-1">
    {#if linkable}
      <!-- The path is a regex in cloudflared, so the link opens the hostname alone. -->
      <Tip text="Open https://{host}">
        {#snippet children({
          props,
        })}
          <a
            {...mergeProps(props, {
              onclick: (e: MouseEvent) => open(e, `https://${host}`),
            })}
            class="truncate text-foreground {link}"
            href="https://{host}"
          >
            {host}
            {#if m.path}
              <span class="text-muted-foreground">{m.path}</span>
            {/if}
          </a>
        {/snippet}
      </Tip>
    {:else}
      <!-- Truncated, so the native title reveals the whole string (02 §2 Tip exception). -->
      {@render unlinkedHostname(m, host)}
    {/if}
    {#if host}
      <CopyButton size="icon-sm" value="https://{host}" what="https://{host}" />
    {/if}
  </span>
{/snippet}

<section aria-labelledby={headingId}>
  {@render routeHeading()}

  {#if restartWarning?.id === tunnel.id}
    <div
      class="mb-3 flex items-start justify-between gap-3 rounded-md bg-fill px-3.5 py-2"
      role="status"
    >
      <div class="min-w-0 text-callout">
        <p>Hostnames saved. The tunnel couldn't restart.</p>
        <p class="select-text text-xs text-bad-text">
          {restartWarning.message}
        </p>
      </div>
      <Button
        disabled={retryingRestart}
        onclick={retryRestart}
        variant="outline"
        >{retryingRestart ? "Restarting…" : "Restart tunnel"}</Button
      >
    </div>
  {/if}

  {#if routes.length === 0}
    <p class="mx-0.5 text-xs text-muted-foreground">
      {#if tunnel.kind === "token"}
        Hostnames for token tunnels are configured in the Cloudflare dashboard.
        They appear here once the tunnel connects.
      {:else}
        This tunnel has no hostname rules.
      {/if}
    </p>
  {:else}
    <!-- routes-grid / routes-row (app.css): rows are subgrids, so every service sits in one right-hand column. -->
    <ul class="routes-grid rounded-xl bg-card px-4">
      {#each routes as m, i (i)}
        {@render routeRow(m, i)}
      {/each}
    </ul>
  {/if}
</section>

{#if HostnameSheet && manageTarget}
  <HostnameSheet
    onsaved={saved}
    returnFocus={() => manageTrigger}
    target={manageTarget}
    bind:open={manageOpen}
  />
{/if}
