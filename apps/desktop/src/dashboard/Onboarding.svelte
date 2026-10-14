<script lang="ts">
  import ArrowRight from "@lucide/svelte/icons/arrow-right";
  import Check from "@lucide/svelte/icons/check";
  import CircleCheck from "@lucide/svelte/icons/circle-check";
  import Cloud from "@lucide/svelte/icons/cloud";
  import Download from "@lucide/svelte/icons/download";
  import ExternalLink from "@lucide/svelte/icons/external-link";
  import Laptop from "@lucide/svelte/icons/laptop";
  import Link from "@lucide/svelte/icons/link";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import { open as openPicker } from "@tauri-apps/plugin-dialog";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { onMount, tick } from "svelte";
  import CopyButton from "$lib/components/app/CopyButton.svelte";
  import LaneGlyph from "$lib/components/app/LaneGlyph.svelte";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import { Label } from "$lib/components/ui/label/index.js";
  import { api, errorText } from "../lib/api";
  import { store } from "../lib/store.svelte";
  import type { Binary } from "../lib/types";

  let {
    existingAuthorization,
    oncomplete,
  }: {
    existingAuthorization: boolean;
    oncomplete: (mode: "project" | null, id?: string) => Promise<void>;
  } = $props();

  type Step = "welcome" | "connector" | "choose" | "quick" | "account";
  let step = $state<Step>("welcome");
  let binary = $state<Binary | null>(null);
  let detecting = $state(false);
  let busy = $state(false);
  let failure = $state<string | null>(null);
  let detectionFailure = $state<string | null>(null);
  let origin = $state("http://localhost:3000");
  let createdId = $state<string | null>(null);
  let loginRequest = $state<string | null>(null);
  let connectedAccount = $state(false);
  let heading = $state<HTMLHeadingElement | null>(null);
  let alive = true;
  const uid = $props.id();
  const ready = $derived(binary !== null && binary.version !== null);
  const tunnel = $derived(store.tunnels.find((item) => item.id === createdId));
  const running = $derived(tunnel?.state.supervised ?? false);
  const publicUrl = $derived(tunnel?.state.public_url);
  const sharing = $derived(tunnel?.state.status === "connected" && !!publicUrl);
  const phase = $derived.by(() => {
    if (step === "welcome") {
      return 0;
    }
    return step === "connector" ? 1 : 2;
  });
  const connectorLabel = $derived.by(() => {
    if (detecting) {
      return "Looking for cloudflared…";
    }
    if (busy) {
      return "Preparing cloudflared…";
    }
    return ready ? "cloudflared is ready" : "cloudflared wasn’t found";
  });
  const startLabel = $derived.by(() => {
    if (busy) {
      return "Starting…";
    }
    return createdId ? "Try again" : "Start temporary tunnel";
  });
  function back() {
    if (step === "connector") {
      return go("welcome");
    }
    return go(step === "choose" ? "connector" : "choose");
  }
  const docs =
    "https://developers.cloudflare.com/cloudflare-one/networks/connectors/cloudflare-tunnel/downloads/";

  async function detect() {
    if (detecting || busy || !alive) {
      return;
    }
    detecting = true;
    detectionFailure = null;
    try {
      const result = await api.detectCloudflared();
      if (alive) {
        binary = result;
      }
    } catch (e) {
      if (alive) {
        detectionFailure = errorText(e);
      }
    } finally {
      if (alive) {
        detecting = false;
      }
    }
  }

  onMount(() => {
    detect();
    return () => {
      alive = false;
      if (loginRequest) {
        api.cancelOnboardingLogin(loginRequest).catch(console.error);
      }
    };
  });

  async function go(next: Step) {
    failure = null;
    step = next;
    await tick();
    heading?.focus();
  }

  async function install() {
    if (busy || detecting) {
      return;
    }
    busy = true;
    failure = null;
    try {
      const result = await api.installCloudflared();
      if (alive) {
        binary = result;
        detectionFailure = null;
      }
    } catch (e) {
      if (alive) {
        failure = errorText(e);
      }
    } finally {
      if (alive) {
        busy = false;
      }
    }
  }

  async function chooseBinary() {
    if (busy || detecting) {
      return;
    }
    busy = true;
    failure = null;
    try {
      const path = await openPicker({
        directory: false,
        multiple: false,
        title: "Choose cloudflared",
      });
      if (!alive || typeof path !== "string") {
        return;
      }
      const result = await api.selectCloudflared(path);
      if (alive) {
        binary = result;
        detectionFailure = null;
        if (!result) {
          failure =
            "This executable could not run as cloudflared. Choose another copy or install it for Underlane.";
        }
      }
    } catch (e) {
      if (alive) {
        failure = errorText(e);
      }
    } finally {
      if (alive) {
        busy = false;
      }
    }
  }

  async function open(url: string) {
    try {
      await openUrl(url);
    } catch (e) {
      failure = errorText(e);
    }
  }

  async function startQuick() {
    if (busy || !ready) {
      return;
    }
    busy = true;
    failure = null;
    try {
      if (!createdId) {
        createdId = await api.addQuick("", origin, "Quick tunnels", false);
      }
      await api.start(createdId);
      await store.refresh();
      if (store.error) {
        throw new Error(store.error);
      }
    } catch (e) {
      if (alive) {
        failure = errorText(e);
      }
    } finally {
      if (alive) {
        busy = false;
      }
    }
  }

  async function stopQuick() {
    if (!createdId || busy) {
      return;
    }
    busy = true;
    failure = null;
    try {
      await api.stop(createdId);
      await store.refresh();
    } catch (e) {
      failure = errorText(e);
    } finally {
      busy = false;
    }
  }

  async function connect() {
    if (busy || !ready) {
      return;
    }
    const request = crypto.randomUUID();
    loginRequest = request;
    busy = true;
    failure = null;
    try {
      await api.connectCloudflare(request);
      if (alive && loginRequest === request) {
        connectedAccount = true;
      }
    } catch (e) {
      if (alive && loginRequest === request) {
        failure = errorText(e);
      }
    } finally {
      if (alive && loginRequest === request) {
        loginRequest = null;
        busy = false;
      }
    }
  }

  async function cancelLogin() {
    const request = loginRequest;
    if (!request) {
      return;
    }
    // Invalidate the UI request before awaiting cancellation; success can arrive meanwhile.
    loginRequest = null;
    try {
      await api.cancelOnboardingLogin(request);
      if (alive) {
        busy = false;
        failure = "Authorization cancelled. Try again or use a temporary link.";
      }
    } catch (e) {
      if (alive) {
        busy = false;
        failure = errorText(e);
      }
    }
  }

  async function finish(mode: "project" | null = null) {
    if (busy) {
      return;
    }
    busy = true;
    failure = null;
    try {
      await oncomplete(mode, createdId ?? undefined);
    } catch (e) {
      if (alive) {
        failure = errorText(e);
        busy = false;
      }
    }
  }
</script>

<svelte:window
  onfocus={() => {
    if (step === "connector" && !ready) {
      detect();
    }
  }}
/>

<main class="flex h-screen flex-col overflow-y-auto bg-background">
  <header
    class="flex h-[52px] shrink-0 items-center justify-center border-b-[0.5px] border-border"
    data-tauri-drag-region
  >
    <div class="pointer-events-none flex items-center gap-2 font-medium">
      <LaneGlyph size={18} />Underlane
    </div>
  </header>
  <div class="m-auto w-full max-w-[620px] px-8 py-10 sm:px-12">
    <nav
      aria-label="Setup progress"
      class="mb-9 flex flex-wrap gap-6 text-xs text-muted-foreground"
    >
      {#each ["Welcome", "Connector", "First tunnel"] as label, index (label)}
        <span
          aria-current={phase === index ? "step" : undefined}
          class={phase === index
            ? "flex items-center gap-1.5 text-primary"
            : "flex items-center gap-1.5"}
        >
          {#if phase > index}
            <Check class="size-3" />
          {/if}
          {label}
        </span>
      {/each}
    </nav>

    {#if step === "welcome"}
      <p class="mb-3 text-xs font-medium text-primary">
        Local work. A shareable link.
      </p>
      <h1
        class="mb-4 text-[32px] leading-tight font-medium tracking-tight outline-none"
        tabindex="-1"
        bind:this={heading}
      >
        Your localhost,<br>one link away.
      </h1>
      <p class="max-w-[430px] text-callout text-muted-foreground">
        Share a local app, test webhooks, and manage tunnels from your menu bar.
      </p>
      <div
        aria-label="Local service connects through Cloudflare to a public URL"
        class="my-8 flex items-center justify-around gap-2 rounded-lg bg-muted px-3 py-7 text-xs"
        role="img"
      >
        <div class="flex flex-col items-center gap-3">
          <Laptop class="size-5 text-muted-foreground" />localhost:3000
        </div>
        <ArrowRight
          aria-hidden="true"
          class="size-4 shrink-0 text-muted-foreground"
        />
        <div class="flex flex-col items-center gap-3">
          <Cloud class="size-5 text-muted-foreground" />Cloudflare
        </div>
        <ArrowRight
          aria-hidden="true"
          class="size-4 shrink-0 text-muted-foreground"
        />
        <div class="flex flex-col items-center gap-3">
          <Link class="size-5 text-muted-foreground" />Public URL
        </div>
      </div>
      <Button onclick={() => go("connector")} size="lg"
        >Get started<ArrowRight /></Button
      >
    {:else if step === "connector"}
      <p class="mb-3 text-xs font-medium text-primary">One-time setup</p>
      <h1
        class="mb-4 text-[28px] leading-tight font-medium tracking-tight outline-none"
        tabindex="-1"
        bind:this={heading}
      >
        Get the tunnel connector.
      </h1>
      <p class="text-callout text-muted-foreground">
        Underlane uses cloudflared to connect your local service to Cloudflare.
      </p>
      <div
        class="my-6 flex items-center gap-3 rounded-lg bg-muted p-4"
        role="status"
      >
        {#if detecting || busy}
          <LoaderCircle
            class="size-5 animate-spin text-muted-foreground motion-reduce:animate-none"
          />
        {:else if ready}
          <CircleCheck class="size-5 text-primary" />
        {:else}
          <Download class="size-5 text-muted-foreground" />
        {/if}
        <div class="min-w-0">
          <p class="font-medium">
            {connectorLabel}
          </p>
          <p class="mt-1 text-xs text-muted-foreground">
            {ready
              ? `Version ${binary?.version}`
              : "Install it here or choose an existing executable."}
          </p>
        </div>
      </div>
      {#if ready}
        <details class="mb-6 text-xs text-muted-foreground">
          <summary class="w-fit rounded-sm">Executable details</summary>
          <p class="mt-2 break-all font-mono select-text">{binary?.path}</p>
        </details>
        <Button
          disabled={busy || detecting}
          onclick={() => go("choose")}
          size="lg"
          >Continue<ArrowRight /></Button
        >
      {:else}
        <div class="flex flex-wrap gap-3">
          <Button disabled={busy || detecting} onclick={install} size="lg"
            ><Download />Install for Underlane</Button
          >
          <Button
            disabled={busy || detecting}
            onclick={chooseBinary}
            size="lg"
            variant="outline"
            >Choose existing binary…</Button
          >
        </div>
        <p class="mt-4 text-xs text-muted-foreground">
          Downloads the official cloudflared release into Underlane’s data
          folder. No administrator password or system service.
        </p>
      {/if}
      <div class="mt-6 flex flex-wrap gap-5">
        <Button disabled={busy || detecting} onclick={detect} variant="link"
          >Check again</Button
        >
        {#if store.overview?.settings.cloudflared_path && !ready}
          <Button
            disabled={busy || detecting}
            onclick={async () => {
              busy = true;
              try {
                binary = await api.selectCloudflared(null);
                detectionFailure = null;
              } catch (e) {
                failure = errorText(e);
              } finally {
                busy = false;
              }
            }}
            variant="link"
            >Search automatically</Button
          >
        {/if}
        <Button onclick={() => open(docs)} variant="link"
          >Installation guide<ExternalLink /></Button
        >
      </div>
      {#if detectionFailure}
        <p class="mt-5 text-callout text-destructive" role="alert">
          {detectionFailure}
        </p>
      {/if}
    {:else if step === "choose"}
      <h1
        class="mb-4 text-[28px] leading-tight font-medium tracking-tight outline-none"
        tabindex="-1"
        bind:this={heading}
      >
        How would you like to share?
      </h1>
      <p class="mb-3 text-callout text-muted-foreground">
        Start with a temporary link, or connect Cloudflare for your own
        hostname.
      </p>
      <div class="flex flex-col gap-3">
        <section
          class="flex flex-wrap items-center justify-between gap-4 border-b-[0.5px] border-border py-6"
        >
          <div>
            <h2 class="mb-1 font-medium">Temporary link</h2>
            <p class="text-callout text-muted-foreground">
              Share a demo or test a webhook.<br>Random URL · no account needed
            </p>
          </div>
          <Button onclick={() => go("quick")} size="lg"
            >Use temporary link</Button
          >
        </section>
        <section class="flex flex-wrap items-center justify-between gap-4 py-6">
          <div>
            <h2 class="mb-1 font-medium">Your own hostname</h2>
            <p class="text-callout text-muted-foreground">
              Keep a consistent address.<br>Cloudflare account + domain setup
            </p>
          </div>
          <Button onclick={() => go("account")} size="lg" variant="outline"
            >Connect Cloudflare</Button
          >
        </section>
      </div>
      <Button disabled={busy} onclick={() => finish()} variant="link"
        >Open dashboard — set up a tunnel later</Button
      >
    {:else if step === "quick"}
      <p class="mb-3 text-xs font-medium text-primary">
        No Cloudflare account needed
      </p>
      <h1
        class="mb-4 text-[28px] leading-tight font-medium tracking-tight outline-none"
        tabindex="-1"
        bind:this={heading}
      >
        {sharing ? "Your link is ready." : "Share your local app."}
      </h1>
      {#if sharing && publicUrl}
        <p class="mb-5 text-callout text-muted-foreground">
          Your temporary tunnel is connected.
        </p>
        <div class="flex items-center gap-2 rounded-lg bg-muted p-4">
          <p
            class="min-w-0 flex-1 break-all font-mono text-callout select-text"
          >
            {publicUrl}
          </p>
          <CopyButton value={publicUrl} what="public URL" />
        </div>
        <div class="mt-6 flex flex-wrap gap-3">
          <Button disabled={busy} onclick={() => finish()} size="lg"
            >Open dashboard</Button
          ><Button
            onclick={() => open(publicUrl ?? "")}
            size="lg"
            variant="outline"
            >Open link<ExternalLink /></Button
          ><Button
            disabled={busy}
            onclick={stopQuick}
            size="lg"
            variant="outline"
            >Stop tunnel</Button
          >
        </div>
      {:else}
        <form
          class="mt-6"
          onsubmit={(event) => {
            event.preventDefault();
            startQuick();
          }}
        >
          <Label for={`${uid}-origin`}>Local service URL</Label>
          <Input
            class="mt-2"
            disabled={busy || createdId !== null}
            id={`${uid}-origin`}
            placeholder="http://localhost:3000"
            required
            spellcheck={false}
            bind:value={origin}
          />
          <p class="mt-3 text-xs text-muted-foreground">
            Starts a public link to this service. Anyone with the link can
            access it. The random URL changes each time you start the tunnel.
          </p>
          {#if running && tunnel?.state.status !== "crashed"}
            <p class="mt-6 flex items-center gap-2 text-callout" role="status">
              <LoaderCircle
                class="size-4 animate-spin motion-reduce:animate-none"
              />Connecting to Cloudflare…
            </p>
            <div class="mt-4 flex flex-wrap gap-3">
              <Button disabled={busy} onclick={stopQuick} variant="outline"
                >Stop tunnel</Button
              ><Button
                disabled={busy}
                onclick={() => finish()}
                variant="outline"
                >Open dashboard</Button
              >
            </div>
          {:else}
            <Button
              class="mt-6"
              disabled={busy || !ready}
              size="lg"
              type="submit"
              >{startLabel}<ArrowRight /></Button
            >
          {/if}
        </form>
        {#if tunnel?.state.status === "crashed"}
          <p class="mt-4 text-callout text-destructive" role="alert">
            {tunnel.state.failure?.cause ??
              tunnel.state.detail ??
              "The tunnel could not connect. Try again or review it in the dashboard."}
          </p>
        {/if}
      {/if}
      <p class="mt-5 text-xs text-muted-foreground">
        Temporary tunnels are for testing and development. The link stops
        working when you stop the tunnel or quit Underlane.
      </p>
    {:else}
      <p class="mb-3 text-xs font-medium text-primary">Your own hostname</p>
      <h1
        class="mb-4 text-[28px] leading-tight font-medium tracking-tight outline-none"
        tabindex="-1"
        bind:this={heading}
      >
        {connectedAccount ? "Cloudflare connected." : "Connect Cloudflare."}
      </h1>
      {#if connectedAccount}
        <p class="mb-6 text-callout text-muted-foreground">
          Create a project next, then configure its tunnel and hostname.
        </p>
        <Button disabled={busy} onclick={() => finish("project")} size="lg"
          >Create a project<ArrowRight /></Button
        >
      {:else}
        <p class="mb-4 text-callout text-muted-foreground">
          Continue in your browser to authorize cloudflared for your Cloudflare
          account.
        </p>
        <p class="mb-6 text-xs text-muted-foreground">
          This saves an account certificate on your Mac and allows cloudflared
          to manage tunnels in that account. Your hostname and DNS still need to
          be configured.
        </p>
        {#if existingAuthorization}
          <p class="mb-5 text-xs text-muted-foreground">
            Existing authorization found. We’ll verify it before continuing.
          </p>
        {/if}
        {#if loginRequest}
          <p class="mb-5 flex items-center gap-2 text-callout" role="status">
            <LoaderCircle
              class="size-4 animate-spin motion-reduce:animate-none"
            />Finish authorization in your browser.
          </p>
          <Button onclick={cancelLogin} size="lg" variant="outline"
            >Cancel authorization</Button
          >
        {:else}
          <Button disabled={busy || !ready} onclick={connect} size="lg"
            >{existingAuthorization
              ? "Verify existing authorization"
              : "Connect in browser"}<ExternalLink /></Button
          >
        {/if}
      {/if}
      <div class="mt-6">
        <Button disabled={busy} onclick={() => go("quick")} variant="link"
          >Use a temporary link instead</Button
        >
      </div>
    {/if}

    {#if failure}
      <p class="mt-6 text-callout text-destructive select-text" role="alert">
        {failure}
      </p>
    {/if}
    {#if step !== "welcome"}
      <div class="mt-9 border-t-[0.5px] border-border pt-5">
        <Button
          disabled={busy || running || createdId !== null}
          onclick={back}
          variant="ghost"
          >Back</Button
        >
        {#if createdId && !sharing}
          <Button disabled={busy} onclick={() => finish()} variant="ghost"
            >Open dashboard</Button
          >
        {/if}
      </div>
    {/if}
  </div>
</main>
