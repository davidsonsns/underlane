<script lang="ts">
  import ArrowRight from "@lucide/svelte/icons/arrow-right";
  import Check from "@lucide/svelte/icons/check";
  import CircleCheck from "@lucide/svelte/icons/circle-check";
  import Download from "@lucide/svelte/icons/download";
  import ExternalLink from "@lucide/svelte/icons/external-link";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import { open as openPicker } from "@tauri-apps/plugin-dialog";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { onMount, tick } from "svelte";
  import CopyButton from "$lib/components/app/CopyButton.svelte";
  import LaneGlyph from "$lib/components/app/LaneGlyph.svelte";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import { Label } from "$lib/components/ui/label/index.js";
  import {
    Content as SelectContent,
    Item as SelectItem,
    Root as SelectRoot,
    Trigger as SelectTrigger,
  } from "$lib/components/ui/select/index.js";
  import { notify } from "$lib/toast";
  import flowDark from "../assets/onboarding/flow-dark.png";
  import flowLight from "../assets/onboarding/flow-light.png";
  import { api, errorText } from "../lib/api";
  import { store } from "../lib/store.svelte";
  import type { Binary, Settings } from "../lib/types";

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
  let theme = $state<Settings["theme"]>("system");
  let savingTheme = $state(false);
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
  const docs =
    "https://developers.cloudflare.com/cloudflare-one/networks/connectors/cloudflare-tunnel/downloads/";
  const themes = [
    { label: "System", value: "system" },
    { label: "Light", value: "light" },
    { label: "Dark", value: "dark" },
  ] as const;

  $effect(() => {
    theme = store.overview?.settings.theme ?? "system";
  });

  async function setTheme(value: string) {
    const settings = store.overview?.settings;
    if (
      !settings ||
      savingTheme ||
      busy ||
      detecting ||
      value === settings.theme ||
      (value !== "system" && value !== "light" && value !== "dark")
    ) {
      return;
    }
    const previousTheme = settings.theme;
    theme = value;
    savingTheme = true;
    try {
      await api.saveSettings({ ...$state.snapshot(settings), theme: value });
    } catch (e) {
      theme = previousTheme;
      notify.error(e, "Couldn't change theme");
    } finally {
      savingTheme = false;
    }
  }

  async function detect() {
    if (detecting || busy || savingTheme || !alive) {
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
    if (busy || detecting || savingTheme) {
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
    if (busy || detecting || savingTheme) {
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

<svg
  aria-hidden="true"
  class="pointer-events-none absolute size-0"
  focusable="false"
>
  <defs>
    <filter color-interpolation-filters="sRGB" id={`${uid}-flow-ink`}>
      <!-- Separate the pale ink from the blue paper without redrawing fine details. -->
      <feColorMatrix
        type="matrix"
        values="0 0 0 0 1
                0 0 0 0 1
                0 0 0 0 1
                2.5 0 0 0 -0.5"
      />
    </filter>
  </defs>
</svg>

<main class="flex h-screen flex-col overflow-y-auto bg-background">
  <header class="flex h-[52px] shrink-0 items-center gap-4 px-5">
    <div class="h-full flex-1" data-tauri-drag-region></div>
    <SelectRoot
      disabled={savingTheme || busy || detecting || !store.overview}
      onValueChange={setTheme}
      type="single"
      value={theme}
    >
      <SelectTrigger aria-label="Interface theme" class="w-40">
        Theme: {themes.find((item) => item.value === theme)?.label}
      </SelectTrigger>
      <SelectContent align="end">
        {#each themes as item (item.value)}
          <SelectItem label={item.label} value={item.value} />
        {/each}
      </SelectContent>
    </SelectRoot>
  </header>
  <div
    class="onboarding-shell mx-auto flex min-h-[calc(100dvh-52px)] w-full max-w-[900px] shrink-0 flex-col px-8 pt-4 pb-8 sm:px-12"
  >
    <div class="mb-8 flex flex-wrap items-center justify-between gap-6">
      <div class="flex items-center gap-2 font-medium">
        <LaneGlyph size={18} />Underlane
      </div>
      <nav
        aria-label="Setup progress"
        class="flex flex-wrap gap-6 text-xs text-muted-foreground"
      >
        {#each ["Welcome", "Connector", "First tunnel"] as label, index (label)}
          <button
            aria-current={phase === index ? "step" : undefined}
            class={phase === index
              ? "flex items-center gap-1.5 text-primary"
              : "flex items-center gap-1.5 enabled:cursor-pointer enabled:hover:text-foreground"}
            disabled={index >= phase || busy || running || createdId !== null}
            onclick={() => go(index === 0 ? "welcome" : "connector")}
            type="button"
          >
            {#if phase > index}
              <Check class="size-3" />
            {/if}
            {label}
          </button>
        {/each}
      </nav>
    </div>

    {#if step === "welcome"}
      <header class="min-h-[104px]">
        <p class="mb-3 text-xs font-medium text-primary">
          Local work. A shareable link.
        </p>
        <h1
          class="mb-4 text-[28px] leading-tight font-medium tracking-tight outline-none"
          tabindex="-1"
          bind:this={heading}
        >
          Your localhost, one link away.
        </h1>
        <p class="text-callout text-muted-foreground">
          Share a local app, test webhooks, and manage tunnels from your menu
          bar.
        </p>
      </header>
      <div class="min-h-[248px] flex-1 pt-6">
        <img
          alt="A local app at localhost:3000 connects through Underlane and Cloudflare to a public, shareable link."
          class="onboarding-flow h-auto w-full dark:hidden"
          height="728"
          src={flowLight}
          width="2159"
        >
        <div class="onboarding-flow onboarding-flow-grid hidden dark:block">
          <img
            alt="A local app at localhost:3000 connects through Underlane and Cloudflare to a public, shareable link."
            class="h-auto w-full"
            height="728"
            src={flowDark}
            width="2159"
            style:filter={`url(#${uid}-flow-ink)`}
          >
        </div>
      </div>
      <footer class="flex min-h-12 items-center justify-end pt-4">
        <Button onclick={() => go("connector")} size="lg"
          >Get started<ArrowRight /></Button
        >
      </footer>
    {:else if step === "connector"}
      <header class="min-h-[104px]">
        <p class="mb-3 text-xs font-medium text-primary">One-time setup</p>
        <h1
          class="mb-4 text-[28px] leading-tight font-medium tracking-tight outline-none"
          tabindex="-1"
          bind:this={heading}
        >
          Get the tunnel connector.
        </h1>
        <p class="text-callout text-muted-foreground">
          Underlane uses cloudflared to connect your local service to
          Cloudflare.
        </p>
      </header>
      <div class="min-h-[248px] flex-1 pt-6">
        <div
          class="flex items-start gap-3 rounded-lg bg-muted p-4"
          role="status"
        >
          {#if detecting || busy}
            <LoaderCircle
              class="mt-0.5 size-5 shrink-0 animate-spin text-muted-foreground motion-reduce:animate-none"
            />
          {:else if ready}
            <CircleCheck class="mt-0.5 size-5 shrink-0 text-primary" />
          {:else}
            <Download class="mt-0.5 size-5 shrink-0 text-muted-foreground" />
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
            {#if binary?.path}
              <p
                class="mt-3 break-all font-mono text-xs leading-relaxed text-muted-foreground select-text"
              >
                {binary.path}
              </p>
            {/if}
          </div>
        </div>
        {#if !ready}
          <div class="mt-6 flex flex-wrap gap-3">
            <Button
              disabled={busy || detecting || savingTheme}
              onclick={install}
              size="lg"
              ><Download />Install for Underlane</Button
            >
            <Button
              disabled={busy || detecting || savingTheme}
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
        {#if detectionFailure}
          <p class="mt-5 text-callout text-destructive" role="alert">
            {detectionFailure}
          </p>
        {/if}
      </div>
      <footer
        class="flex min-h-12 flex-wrap items-center justify-between gap-5 pt-4"
      >
        <Button onclick={() => open(docs)} variant="link"
          >Installation guide<ExternalLink /></Button
        >
        {#if store.overview?.settings.cloudflared_path && !ready}
          <Button
            disabled={busy || detecting || savingTheme}
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
        {#if ready}
          <Button
            disabled={busy || detecting || savingTheme}
            onclick={() => go("choose")}
            size="lg"
            >Continue<ArrowRight /></Button
          >
        {/if}
      </footer>
    {:else if step === "choose"}
      <header class="min-h-[104px]">
        <p class="mb-3 text-xs font-medium text-primary">First tunnel</p>
        <h1
          class="mb-4 text-[28px] leading-tight font-medium tracking-tight outline-none"
          tabindex="-1"
          bind:this={heading}
        >
          How would you like to share?
        </h1>
        <p class="text-callout text-muted-foreground">
          Start with a temporary link, or connect Cloudflare for your own
          hostname.
        </p>
      </header>
      <div class="flex min-h-[248px] flex-1 flex-col gap-3 pt-6">
        <section
          class="flex flex-wrap items-center justify-between gap-4 border-b-[0.5px] border-border pb-6"
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
      <footer class="flex min-h-12 items-center pt-4">
        <Button disabled={busy} onclick={() => finish()} variant="link"
          >Open dashboard — set up a tunnel later</Button
        >
      </footer>
    {:else if step === "quick"}
      <header class="min-h-[104px]">
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
          <p class="text-callout text-muted-foreground">
            Your temporary tunnel is connected.
          </p>
        {:else}
          <p class="text-callout text-muted-foreground">
            Create a temporary public link to a service running on your Mac.
          </p>
        {/if}
      </header>
      <div class="min-h-[248px] flex-1 pt-6">
        {#if sharing && publicUrl}
          <div class="flex items-center gap-2 rounded-lg bg-muted p-4">
            <p
              class="min-w-0 flex-1 break-all font-mono text-callout select-text"
            >
              {publicUrl}
            </p>
            <CopyButton value={publicUrl} what="public URL" />
          </div>
        {:else}
          <form
            id={`${uid}-quick-form`}
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
              <p
                class="mt-6 flex items-center gap-2 text-callout"
                role="status"
              >
                <LoaderCircle
                  class="size-4 animate-spin motion-reduce:animate-none"
                />Connecting to Cloudflare…
              </p>
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
      </div>
      <footer
        class="flex min-h-12 flex-wrap items-center justify-end gap-3 pt-4"
      >
        {#if sharing && publicUrl}
          <Button
            disabled={busy}
            onclick={stopQuick}
            size="lg"
            variant="outline"
            >Stop tunnel</Button
          >
          <Button
            onclick={() => open(publicUrl ?? "")}
            size="lg"
            variant="outline"
            >Open link<ExternalLink /></Button
          >
          <Button disabled={busy} onclick={() => finish()} size="lg"
            >Open dashboard</Button
          >
        {:else if running && tunnel?.state.status !== "crashed"}
          <Button
            disabled={busy}
            onclick={stopQuick}
            size="lg"
            variant="outline"
            >Stop tunnel</Button
          >
          <Button
            disabled={busy}
            onclick={() => finish()}
            size="lg"
            variant="outline"
            >Open dashboard</Button
          >
        {:else}
          {#if createdId}
            <Button
              disabled={busy}
              onclick={() => finish()}
              size="lg"
              variant="outline"
              >Open dashboard</Button
            >
          {/if}
          <Button
            disabled={busy || !ready}
            form={`${uid}-quick-form`}
            size="lg"
            type="submit"
            >{startLabel}<ArrowRight /></Button
          >
        {/if}
      </footer>
    {:else}
      <header class="min-h-[104px]">
        <p class="mb-3 text-xs font-medium text-primary">Your own hostname</p>
        <h1
          class="mb-4 text-[28px] leading-tight font-medium tracking-tight outline-none"
          tabindex="-1"
          bind:this={heading}
        >
          {connectedAccount ? "Cloudflare connected." : "Connect Cloudflare."}
        </h1>
        {#if connectedAccount}
          <p class="text-callout text-muted-foreground">
            Create a project next, then configure its tunnel and hostname.
          </p>
        {:else}
          <p class="text-callout text-muted-foreground">
            Continue in your browser to authorize cloudflared for your
            Cloudflare account.
          </p>
        {/if}
      </header>
      <div class="min-h-[248px] flex-1 pt-6">
        {#if !connectedAccount}
          <p class="mb-6 text-xs text-muted-foreground">
            This saves an account certificate on your Mac and allows cloudflared
            to manage tunnels in that account. Your hostname and DNS still need
            to be configured.
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
          {/if}
        {/if}
      </div>
      <footer
        class="flex min-h-12 flex-wrap items-center justify-between gap-3 pt-4"
      >
        <Button disabled={busy} onclick={() => go("quick")} variant="link"
          >Use a temporary link instead</Button
        >
        {#if connectedAccount}
          <Button disabled={busy} onclick={() => finish("project")} size="lg"
            >Create a project<ArrowRight /></Button
          >
        {:else if loginRequest}
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
      </footer>
    {/if}

    {#if failure}
      <p class="mt-6 text-callout text-destructive select-text" role="alert">
        {failure}
      </p>
    {/if}
  </div>
</main>

<style>
  .onboarding-shell > footer {
    order: 1;
  }

  .onboarding-flow {
    mask-image:
      linear-gradient(
        to right,
        transparent,
        #000 24px,
        #000 calc(100% - 24px),
        transparent
      ),
      linear-gradient(
        to bottom,
        transparent,
        #000 24px,
        #000 calc(100% - 24px),
        transparent
      );
    mask-composite: intersect;
  }

  .onboarding-flow-grid {
    background-image:
      linear-gradient(to right, var(--border) 1px, transparent 1px),
      linear-gradient(to bottom, var(--border) 1px, transparent 1px);
    background-size: 12px 12px;
  }
</style>
