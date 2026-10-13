<script lang="ts">
  // A tunnel's ingress rules as an aligned table: public hostname, then the local service.
  // Each service is probed while the window is visible so it's obvious which ones are running.
  import { onMount, tick, untrack } from "svelte";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import Check from "@lucide/svelte/icons/check";
  import { store } from "../lib/store.svelte";
  import { api, errorText } from "../lib/api";
  import { reachSummary, serviceLabel, shortService } from "../lib/format";
  import type { IngressMatch, Tunnel } from "../lib/types";
  import { ingressTester, sameIngressRules } from "$lib/ingress-test";
  import { mergeProps } from "bits-ui";
  import CopyButton from "$lib/components/app/CopyButton.svelte";
  import Tip from "$lib/components/app/Tip.svelte";
  import { kbRing } from "$lib/components/ui/mac";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Input } from "$lib/components/ui/input/index.js";

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

  const testTarget = $derived(tunnel.kind === "config" && tunnel.config_path
    ? { id: tunnel.id, source_path: tunnel.config_path, run: s.run, config_revision: s.config_revision }
    : null);
  const testKey = $derived(JSON.stringify(testTarget));
  let testOpen = $state(false);
  let testUrl = $state("");
  let testInput = $state<HTMLInputElement | null>(null);
  let testing = $state(false);
  let match = $state<IngressMatch | null>(null);
  let testError = $state<string | null>(null);
  let expired = $state(false);
  const tester = ingressTester(
    () => testTarget,
    api.testIngressRule,
    api.cancelIngressTest,
    (result) => { match = result; testing = false; },
    (error) => { testError = errorText(error); testing = false; },
  );
  function clearTest(stale = false) {
    tester.cancel();
    expired = stale && (expired || match !== null || testing);
    match = null;
    testError = null;
    testing = false;
  }
  $effect(() => {
    testKey;
    untrack(() => { clearTest(); testUrl = ""; testOpen = false; });
  });
  onMount(() => {
    // A focus change can follow an external edit, including for a stopped tunnel with no P1 baseline.
    const invalidate = () => clearTest(true);
    const visible = () => { if (document.visibilityState === "visible") invalidate(); };
    window.addEventListener("focus", invalidate);
    document.addEventListener("visibilitychange", visible);
    return () => {
      tester.cancel();
      window.removeEventListener("focus", invalidate);
      document.removeEventListener("visibilitychange", visible);
    };
  });
  async function showTest() {
    testOpen = true;
    await tick();
    testInput?.focus();
  }
  async function test(e: SubmitEvent) {
    e.preventDefault();
    clearTest();
    if (!testUrl.trim()) return;
    testing = true;
    await tester.test(testUrl.trim());
  }
  const matchedRow = $derived(match && sameIngressRules(match.rules, routes) ? match.rule_index : null);
  const matchedRule = $derived(match?.rules[match.rule_index]);
  const headingId = $props.id();
  const testDescriptionId = `${headingId}-test-description`;
  const testInputId = `${headingId}-test-input`;

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

  // Underlane never starts or stops these services, so the tips say what the dot means for traffic.
  function reachTip(service: string, r: Reach): string | undefined {
    const addr = shortService(service);
    if (r === "up") return `Live. Traffic to this hostname reaches ${addr}.`;
    if (r === "ready") return `Something is listening on ${addr}, but the tunnel is stopped. Start it to route traffic here. Underlane doesn't manage this service.`;
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

  const dot: Record<Reach, string> = {
    up: "bg-ok",
    ready: "bg-off",
    down: "bg-bad",
    idle: "shadow-[inset_0_0_0_1.5px_var(--off)]", // a hollow ring: probed nothing yet (StatusDot's idle look)
    none: "invisible", // keeps the column width so services stay aligned
  };
  const link = `cursor-pointer select-none rounded-xs hover:underline ${kbRing}`;

  function hostOf(m: (typeof routes)[number]): string | null {
    if (tunnel.kind === "quick") return s.public_url?.replace("https://", "") ?? null;
    return m.hostname;
  }
</script>

<section class="mt-8" aria-labelledby={headingId}>
  <div class="mb-3 ms-3.5 flex items-baseline justify-between gap-3">
    <h2 id={headingId} class="text-sm font-medium">Hostnames</h2>
    {#if summary}
      <span class="text-xs {live && upCount < probed.length ? 'text-bad-text' : 'text-muted-foreground'}" role="status">{summary}</span>
    {/if}
  </div>

  {#if routes.length === 0}
    <p class="mx-0.5 text-xs text-muted-foreground">
      {#if tunnel.kind === "token"}Hostnames for token tunnels are configured in the Cloudflare dashboard. They appear here once the tunnel connects.{:else}This tunnel has no hostname rules.{/if}
    </p>
  {:else}
    <!-- routes-grid / routes-row (app.css): rows are subgrids, so every service sits in one right-hand column. -->
    <ul class="routes-grid rounded-xl bg-card px-4">
      {#each routes as m, i (i)}
        {@const host = hostOf(m)}
        {@const r = reachOf(m.service)}
        {@const tip = reachTip(m.service, r)}
        {@const linkable = live && host && !host.includes("*")}
        <li class="group/route routes-row min-h-11 items-center py-2.5 not-first:border-t not-first:border-border {matchedRow === i ? 'bg-tint/8' : ''}">
          <span class="flex min-w-0 items-center gap-1">
            {#if linkable}
              <!-- The path is a regex in cloudflared, so the link opens the hostname alone. -->
              <Tip text="Open https://{host}">
                {#snippet children({ props })}
                  <a {...mergeProps(props, { onclick: (e: MouseEvent) => open(e, `https://${host}`) })} href="https://{host}" class="truncate text-foreground {link}">
                    {host}{#if m.path}<span class="text-muted-foreground">{m.path}</span>{/if}
                  </a>
                {/snippet}
              </Tip>
              <CopyButton
                value="https://{host}"
                what="https://{host}"
                size="icon-sm"
                class="opacity-0 transition-opacity duration-(--dur) group-hover/route:opacity-100 focus-visible:opacity-100"
              />
            {:else}
              <!-- Truncated, so the native title reveals the whole string (02 §2 Tip exception). -->
              <span class="truncate select-text {host ? 'text-foreground' : 'text-muted-foreground'}" title={host ? `${host}${m.path ?? ""}` : undefined}>
                {#if host}{host}{:else if tunnel.kind === "quick"}Assigned when connected{:else}Any other hostname{/if}{#if m.path}<span class="text-muted-foreground">{m.path}</span>{/if}
              </span>
            {/if}
          </span>
          <span class="flex items-center gap-1.75 justify-self-end text-callout whitespace-nowrap text-muted-foreground">
            <Tip text={tip ?? ""} disabled={!tip}>
              {#snippet children({ props })}
                <!-- The dot itself is the trigger (no extra button); ::before widens the 7px hover target. -->
                <span
                  {...{ ...props, tabindex: undefined }}
                  role={tip ? "img" : undefined}
                  aria-label={tip}
                  class="relative size-1.75 flex-none rounded-full transition-[background-color,box-shadow] duration-240 before:absolute before:-inset-1 {dot[r]}"
                >
                  <!-- Requests through an inspected route flash the dot, like a network activity light. -->
                  {#key store.pulses[tunnel.id]}
                    {#if m.inspected && store.pulses[tunnel.id]}<span class="absolute inset-0 animate-route-ping rounded-full bg-tint motion-reduce:hidden"></span>{/if}
                  {/key}
                </span>
              {/snippet}
            </Tip>
            <!-- The service carries the same explanation, so hovering it, or Tab to its link, shows it too (as before). -->
            <Tip text={tip ?? ""} disabled={!tip}>
              {#snippet children({ props })}
                {#if /^https?:\/\//.test(m.service)}
                  <a {...mergeProps(props, { onclick: (e: MouseEvent) => open(e, m.service) })} href={m.service} class="font-mono {link}">{shortService(m.service)}</a>
                {:else}
                  <span {...{ ...props, tabindex: undefined }} class={serviceLabel(m.service) === shortService(m.service) ? "font-mono select-text" : undefined}>{serviceLabel(m.service)}</span>
                {/if}
              {/snippet}
            </Tip>
          </span>
          {#if m.note && tunnel.inspect}<span class="col-span-full mt-0.5 text-xs text-muted-foreground">{m.note}</span>{/if}
          {#if matchedRow === i}<span class="col-span-full mt-0.5 flex items-center gap-1 text-xs font-medium text-tint"><Check class="size-3" aria-hidden="true" /> Matched rule {i + 1} · Config on disk</span>{/if}
        </li>
      {/each}
    </ul>
  {/if}

  {#if testTarget}
    {#if !testOpen}
      <Button variant="ghost" class="mt-2" onclick={showTest}>Test a URL…</Button>
    {:else}
      <form class="mt-3 space-y-2" onsubmit={test} aria-busy={testing}>
        <label for={testInputId} class="ms-0.5 text-xs font-medium">Test a URL · Config on disk</label>
        <div class="flex items-center gap-2">
          <Input id={testInputId} bind:ref={testInput} bind:value={testUrl} oninput={() => clearTest()}
            aria-describedby={testDescriptionId} aria-invalid={testError ? true : undefined}
            placeholder="https://app.example.com/path" class="font-mono text-callout"
            autocorrect="off" autocapitalize="off" spellcheck={false} writingsuggestions={false} />
          <Button type="submit" disabled={testing || !testUrl.trim()}>{testing ? "Testing…" : "Test"}</Button>
          {#if testing}<Button variant="outline" onclick={() => clearTest()}>Cancel</Button>{/if}
        </div>
        <p id={testDescriptionId} class="ms-0.5 text-xs text-muted-foreground">Matches the latest file offline. No request is sent to the URL.</p>
        {#if testError}<p role="alert" class="ms-0.5 text-xs text-bad-text">{testError}</p>{/if}
        {#if expired}<p role="status" class="ms-0.5 text-xs text-muted-foreground">The config may have changed. Test the URL again.</p>{/if}
        {#if match && matchedRule}
          <div role="status" class="rounded-lg bg-card px-3 py-2 text-xs">
            <p class="flex items-center gap-1 font-medium"><Check class="size-3" aria-hidden="true" /> Matched rule {match.rule_index + 1} · Config on disk</p>
            {#if matchedRow === null}
              <p class="mt-1 text-muted-foreground">The file's rules differ from the displayed hostnames.</p>
              <p class="mt-1 break-all"><span>{matchedRule.hostname ?? "Any other hostname"}{matchedRule.path ?? ""}</span> → <span class="font-mono">{serviceLabel(matchedRule.service)}</span></p>
            {/if}
          </div>
          {#if match.raw}
            <details class="ms-0.5 text-xs text-muted-foreground">
              <summary class="cursor-pointer rounded-xs {kbRing}">Diagnostics</summary>
              <pre class="mt-2 max-h-40 overflow-auto whitespace-pre-wrap break-all select-text">{match.raw}</pre>
            </details>
          {/if}
        {/if}
      </form>
    {/if}
  {:else}
    <p class="mt-3 ms-0.5 text-xs text-muted-foreground">
      {#if tunnel.kind === "quick"}URL testing requires a local config with ingress rules. Quick tunnels use one local service.
      {:else}URL testing requires a local config with ingress rules. Token tunnels are managed in the Cloudflare dashboard.{/if}
    </p>
  {/if}
</section>
