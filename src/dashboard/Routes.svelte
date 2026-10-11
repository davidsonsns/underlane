<script lang="ts">
  // A tunnel's ingress rules as an aligned table: public hostname, then the local service.
  // Each service is probed while the window is visible so it's obvious which ones are running.
  import { writeText } from "@tauri-apps/plugin-clipboard-manager";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { store } from "../lib/store.svelte";
  import { api } from "../lib/api";
  import { reachSummary, serviceLabel, shortService } from "../lib/format";
  import type { Tunnel } from "../lib/types";
  import { tooltip } from "../lib/tooltip";
  import Icon from "../components/Icon.svelte";

  let { tunnel }: { tunnel: Tunnel } = $props();
  const PROBE_EVERY_MS = 3000;

  const s = $derived(tunnel.state);
  const live = $derived(s.status === "connected" || s.status === "degraded");
  const routes = $derived(
    s.mappings.length
      ? s.mappings
      : tunnel.kind === "quick" && tunnel.origin_url
        ? [{ hostname: null, path: null, service: tunnel.origin_url, inspected: false, note: null }]
        : [],
  );
  const services = $derived([...new Set(routes.map((m) => m.service))]);
  const servicesKey = $derived(services.join("\n"));

  /** service -> listening? (null: nothing to probe, e.g. http_status:404) */
  let reach = $state<Record<string, boolean | null>>({});

  $effect(() => {
    const list = servicesKey ? servicesKey.split("\n") : [];
    if (!list.length) return;
    let alive = true;
    const check = async () => {
      if (document.visibilityState !== "visible") return;
      try {
        const res = await api.probeServices(list);
        if (alive) reach = { ...reach, ...Object.fromEntries(list.map((svc, i) => [svc, res[i]])) };
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

  type Reach = "up" | "ready" | "down" | "idle" | "none";
  function reachOf(service: string): Reach {
    const r = reach[service];
    if (r == null) return "none";
    // Green only while traffic can actually arrive; a stopped tunnel's listening services are just "ready".
    if (r) return live ? "up" : "ready";
    // Nothing listening matters most while the tunnel is live: visitors are getting errors.
    return live ? "down" : "idle";
  }

  // Burrow never starts or stops these services, so the tips say what the dot means for traffic.
  function reachTip(service: string, r: Reach): string | undefined {
    const addr = shortService(service);
    if (r === "up") return `Live. Traffic to this route reaches ${addr}.`;
    if (r === "ready") return `Something is listening on ${addr}, but the tunnel is stopped. Start it to route traffic here. Burrow doesn't manage this service.`;
    if (r === "down") return `Nothing is listening on ${addr}. Visitors get a 502 error until it starts.`;
    if (r === "idle") return `Nothing is listening on ${addr}. Start it before starting the tunnel.`;
    return undefined;
  }

  // Counted per route, not per distinct service, so the summary matches the rows below it.
  const probed = $derived(routes.filter((m) => typeof reach[m.service] === "boolean"));
  const upCount = $derived(probed.filter((m) => reach[m.service]).length);
  const summary = $derived(probed.length ? reachSummary(upCount, probed.length, live) : null);

  // Links open in the default browser; the webview itself never navigates away.
  function open(e: MouseEvent, url: string) {
    e.preventDefault();
    openUrl(url);
  }

  function hostOf(m: (typeof routes)[number]): string | null {
    if (tunnel.kind === "quick") return s.public_url?.replace("https://", "") ?? null;
    return m.hostname;
  }
</script>

<section>
  <div class="heading">
    <h2>Routes</h2>
    {#if summary}
      <span class="summary" class:alert={live && upCount < probed.length} role="status">{summary}</span>
    {/if}
  </div>

  {#if routes.length === 0}
    <p class="empty">
      {#if tunnel.kind === "token"}Routes for token tunnels are configured in the Cloudflare dashboard. They appear here once the tunnel connects.{:else}This tunnel has no routes.{/if}
    </p>
  {:else}
    <ul class="routes">
      {#each routes as m, i (i)}
        {@const host = hostOf(m)}
        {@const r = reachOf(m.service)}
        {@const linkable = live && host && !host.includes("*")}
        <li>
          <span class="host">
            {#if linkable}
              <!-- The path is a regex in cloudflared, so the link opens the hostname alone. -->
              <a class="name link" href="https://{host}" title="Open https://{host}" onclick={(e) => open(e, `https://${host}`)}>
                {host}{#if m.path}<span class="path">{m.path}</span>{/if}
              </a>
            {:else}
              <span class="name" class:other={!host} title={host ? `${host}${m.path ?? ""}` : undefined}>
                {#if host}{host}{:else if tunnel.kind === "quick"}Assigned when connected{:else}Any other hostname{/if}{#if m.path}<span class="path">{m.path}</span>{/if}
              </span>
            {/if}
            {#if linkable}
              <button class="icon-btn copy" title="Copy https://{host}" aria-label="Copy https://{host}" onclick={() => writeText(`https://${host}`)}><Icon name="copy" size={13} /></button>
            {/if}
          </span>
          <span class="svc" class:other={r === "none"} use:tooltip={reachTip(m.service, r)}>
            <span class="reach {r}" role={r === "none" ? undefined : "img"} aria-label={r === "none" ? undefined : reachTip(m.service, r)}>
              <!-- Requests through an inspected route flash the dot, like a network activity light. -->
              {#key store.pulses[tunnel.id]}
                {#if m.inspected && store.pulses[tunnel.id]}<span class="ping"></span>{/if}
              {/key}
            </span>
            {#if /^https?:\/\//.test(m.service)}
              <a class="link mono" href={m.service} onclick={(e) => open(e, m.service)}>{shortService(m.service)}</a>
            {:else}
              <span class:mono={serviceLabel(m.service) === shortService(m.service)}>{serviceLabel(m.service)}</span>
            {/if}
          </span>
          {#if m.note && tunnel.inspect}<span class="note">{m.note}</span>{/if}
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  section {
    margin-top: 22px;
  }
  .heading {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 8px;
  }
  h2 {
    margin: 0;
    font-size: var(--text-sm);
    font-weight: 600;
  }
  .summary {
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .summary.alert {
    color: var(--bad);
  }
  .empty {
    margin: 0 2px;
    font-size: var(--text-xs);
    color: var(--text-2);
  }

  /* One grid for the whole list (rows are subgrids), so every service lines up in one column
     flush with the right edge, however long the hostname in front of it is. */
  .routes {
    display: grid;
    grid-template-columns: minmax(0, 1fr) max-content;
    column-gap: 16px;
    list-style: none;
    margin: 0;
    padding: 0;
    background: var(--surface);
    border-radius: var(--radius);
    box-shadow: var(--shadow);
  }
  .routes li {
    display: grid;
    grid-column: 1 / -1;
    grid-template-columns: subgrid;
    align-items: center;
    min-height: 36px;
    padding: 6px 12px;
  }
  .routes li + li {
    border-top: 0.5px solid var(--line);
  }

  .host {
    display: flex;
    align-items: center;
    gap: 4px;
    min-width: 0;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-1);
    user-select: text;
  }
  .link {
    color: inherit;
    text-decoration: none;
    cursor: pointer;
    user-select: none;
  }
  .link:hover {
    text-decoration: underline;
  }
  .link:focus-visible {
    outline: 2px solid var(--tint);
    outline-offset: 2px;
    border-radius: 3px;
  }
  .path {
    color: var(--text-2);
  }
  .other {
    color: var(--text-2);
  }

  .svc {
    justify-self: end;
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: var(--text-callout);
    color: var(--text-2);
    white-space: nowrap;
  }
  .reach {
    position: relative;
    flex: none;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    transition: background-color 240ms ease, box-shadow 240ms ease;
  }
  .reach.up {
    background: var(--ok);
  }
  .reach.ready {
    background: var(--off);
  }
  .reach.down {
    background: var(--bad);
  }
  .reach.idle {
    box-shadow: inset 0 0 0 1.5px var(--off);
  }
  .ping {
    position: absolute;
    inset: 0;
    border-radius: 50%;
    background: var(--tint);
    animation: ping 650ms var(--ease-out) forwards;
  }
  @keyframes ping {
    from { transform: scale(1); opacity: 0.9; }
    to { transform: scale(3); opacity: 0; }
  }
  @media (prefers-reduced-motion: reduce) {
    .ping { display: none; }
  }
  .svc.other .reach {
    visibility: hidden;
  }

  .copy {
    flex: none;
    width: 20px;
    height: 20px;
    opacity: 0;
    transition: opacity var(--dur) ease;
  }
  li:hover .copy,
  .copy:focus-visible {
    opacity: 1;
  }

  .note {
    grid-column: 1 / -1;
    margin-top: 2px;
    font-size: var(--text-xs);
    color: var(--text-2);
  }
</style>
