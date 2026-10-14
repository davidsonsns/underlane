<script lang="ts">
  import { onMount, tick, untrack, type Snippet } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { DropdownMenu } from "bits-ui";
  import { slide } from "svelte/transition";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import Check from "@lucide/svelte/icons/check";
  import CircleCheck from "@lucide/svelte/icons/circle-check";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import Info from "@lucide/svelte/icons/info";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import FolderOpen from "@lucide/svelte/icons/folder-open";
  import FilePenLine from "@lucide/svelte/icons/file-pen-line";
  import List from "@lucide/svelte/icons/list";
  import Play from "@lucide/svelte/icons/play";
  import RotateCw from "@lucide/svelte/icons/rotate-cw";
  import ScrollText from "@lucide/svelte/icons/scroll-text";
  import Square from "@lucide/svelte/icons/square";
  import Ellipsis from "@lucide/svelte/icons/ellipsis";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import { store } from "../lib/store.svelte";
  import { api, errorText } from "../lib/api";
  import { motion } from "../lib/theme";
  import { formatUptime, isOn, statusHint } from "../lib/format";
  import type { ConfigChange, SeparateTunnelPreview, SeparateTunnelProgress, Status, Tunnel, TunnelSetupProgress, Validation } from "../lib/types";
  import { activeTunnelOwner, sharedTunnelProjects } from "$lib/tunnel-conflict";
  import { configChecker } from "$lib/config-change";
  import { failurePresentation } from "$lib/failure";
  import { notify } from "$lib/toast";
  import { returnFocusTo } from "$lib/focus";
  import StatusDot from "$lib/components/app/StatusDot.svelte";
  import Tip from "$lib/components/app/Tip.svelte";
  import IconAction from "$lib/components/app/IconAction.svelte";
  import EditableName from "$lib/components/app/EditableName.svelte";
  import Combobox from "$lib/components/app/Combobox.svelte";
  import SettingsGroup from "$lib/components/app/SettingsGroup.svelte";
  import SettingsRow from "$lib/components/app/SettingsRow.svelte";
  import { kbRing, menuContent, menuItem } from "$lib/components/ui/mac";
  import { cn } from "$lib/utils";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import { Switch } from "$lib/components/ui/switch/index.js";
  import { Checkbox } from "$lib/components/ui/checkbox/index.js";
  import * as Select from "$lib/components/ui/select/index.js";
  import * as AlertDialog from "$lib/components/ui/alert-dialog/index.js";
  import * as Alert from "$lib/components/ui/alert/index.js";
  import Routes from "./Routes.svelte";
  import FailureNotice from "./FailureNotice.svelte";

  let { tunnel, onremove }: { tunnel: Tunnel; onremove: (id: string, deleteConfig: boolean) => Promise<void> } = $props();
  let busy = $state(false);
  let restarting = $state(false);
  let error = $state<string | null>(null);
  let setupError = $state<string | null>(null);
  let setupRequest = $state<string | null>(null);
  let setupStage = $state<TunnelSetupProgress["stage"] | null>(null);
  let cancellingSetup = $state(false);
  let setupSubscription: Promise<UnlistenFn> | null = null;
  let separateSubscription: Promise<UnlistenFn> | null = null;
  let separateOpen = $state(false);
  let separatePreview = $state<SeparateTunnelPreview | null>(null);
  let separateRequest = $state<string | null>(null);
  let separateStage = $state<SeparateTunnelProgress["stage"] | null>(null);
  let separateTrigger = $state<HTMLElement | null>(null);
  let configChange = $state<ConfigChange | null>(null);
  let configError = $state<string | null>(null);
  let validation = $state<Validation | null>(null);
  let validationRequest = 0;
  let removeOpen = $state(false);
  let deleteConfig = $state(false);
  const componentId = $props.id();
  const deleteConfigId = `${componentId}-delete-config`;
  $effect(() => {
    if (!removeOpen) deleteConfig = false;
  });
  let removeTrigger = $state<HTMLElement | null>(null);
  let originDraft = $state("");
  let tokenDraft = $state("");
  let now = $state(Date.now());

  $effect(() => {
    const i = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(i);
  });

  const s = $derived(tunnel.state);
  const failure = $derived(s.failure || s.status === "crashed"
    ? failurePresentation(s.failure, s.detail, s.hint)
    : null);
  const on = $derived(isOn(tunnel));
  const sharedProjects = $derived(sharedTunnelProjects(tunnel, store.tunnels));
  const tunnelOwner = $derived(activeTunnelOwner(tunnel, store.tunnels));
  const operationBusy = $derived(busy || !!tunnel.separating_tunnel);
  const conflictName = $derived(tunnelOwner?.name ?? s.tunnel_conflict?.project_name);
  const statusTooltip = $derived([
    statusHint[s.status],
    s.connections > 0 ? `${s.connections} connection${s.connections === 1 ? "" : "s"} to Cloudflare for redundancy.` : "",
  ].filter(Boolean).join(" "));
  const headerMeta = $derived([
    s.status === "connected" && s.started_at ? formatUptime(s.started_at, now) : "",
    s.restarts > 0 ? `${s.restarts} restart${s.restarts === 1 ? "" : "s"}` : "",
  ].filter(Boolean).join(" · "));
  $effect(() => {
    [tunnel.id, s.run, s.config_revision, s.status];
    validation = null;
    validationRequest += 1;
  });
  const configTarget = $derived(tunnel.kind === "config" && tunnel.config_path && on && s.status !== "stopped" && s.config_revision > 0
    ? { id: tunnel.id, source_path: tunnel.config_path, run: s.run, config_revision: s.config_revision }
    : null);
  const configKey = $derived(JSON.stringify(configTarget));
  let mounted = false;
  const configCheck = configChecker(
    () => configTarget,
    api.checkConfigChange,
    (result) => { configChange = result; configError = null; },
    (e) => { configError = `Couldn't check the config file: ${errorText(e)}`; },
  );

  $effect(() => {
    configKey;
    configChange = null;
    configError = null;
    configCheck.cancel();
    untrack(() => { void configCheck.check(); });
  });

  onMount(() => {
    mounted = true;
    setupSubscription = listen<TunnelSetupProgress>("project-setup-progress", ({ payload }) => {
      if (mounted && payload.id === tunnel.id && payload.request_id === setupRequest) setupStage = payload.stage;
    });
    // Preserve the rejection for setup() without an unhandled rejection before its first click.
    void setupSubscription.catch(() => {});
    separateSubscription = listen<SeparateTunnelProgress>("project-separate-progress", ({ payload }) => {
      if (mounted && payload.id === tunnel.id && payload.request_id === separateRequest) separateStage = payload.stage;
    });
    void separateSubscription.catch(() => {});
    const check = () => {
      void configCheck.check();
      // Draft setup is completed in an external editor. Reload stopped configs on return so
      // their Start button and hostname list reflect the file without requiring a relaunch.
      if (tunnel.kind === "config" && !on) void store.refresh();
    };
    const visible = () => { if (document.visibilityState === "visible") check(); };
    window.addEventListener("focus", check);
    document.addEventListener("visibilitychange", visible);
    return () => {
      mounted = false;
      if (setupRequest) void api.cancelProjectSetup(tunnel.id, setupRequest).catch(() => {});
      void setupSubscription?.then((unsubscribe) => unsubscribe()).catch(() => {});
      void separateSubscription?.then((unsubscribe) => unsubscribe()).catch(() => {});
      window.removeEventListener("focus", check);
      document.removeEventListener("visibilitychange", visible);
      configCheck.cancel();
    };
  });
  const kindText = { config: "Project from a config file", token: "Remotely managed tunnel (token)", quick: "Quick tunnel on trycloudflare.com" };
  const setupMessages: Record<TunnelSetupProgress["stage"], string> = {
    checking: "Checking the project config and Cloudflare login…",
    login: "Complete Cloudflare login in your browser. Setup will continue when you return.",
    creating: "Creating your Cloudflare tunnel…",
    linking: "Linking the tunnel ID and credentials to this project…",
  };
  const separateMessages: Record<SeparateTunnelProgress["stage"], string> = {
    checking: "Checking the config, credentials and hostname ownership…",
    creating: "Creating a separate Cloudflare tunnel…",
    dns: "Moving this project's hostname DNS records to the new tunnel…",
    linking: "Updating this project to use the new tunnel…",
    starting: "Starting this project with its separate tunnel…",
  };
  const noticeVariant: Record<Status, Alert.AlertVariant> = {
    connected: "success",
    starting: "default",
    degraded: "warning",
    crashed: "destructive",
    stopped: "default",
  };

  async function run(fn: () => Promise<unknown>) {
    busy = true;
    error = null;
    validation = null;
    validationRequest += 1;
    try {
      await fn();
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = false;
    }
  }

  async function setup() {
    if (busy || setupRequest || !tunnel.setup_needed) return;
    const target = tunnel.id;
    const request = crypto.randomUUID();
    setupRequest = request;
    setupStage = "checking";
    setupError = null;
    error = null;
    busy = true;
    try {
      await setupSubscription;
      if (!mounted) return;
      await api.setupProjectTunnel(target, request);
      if (!mounted || setupRequest !== request) return;
      await store.refresh();
      if (store.error) throw new Error(`The tunnel was set up, but the project could not refresh: ${store.error}`);
      if (mounted) notify.success("Tunnel set up. Configure hostnames, then start the project.");
    } catch (e) {
      if (mounted && setupRequest === request) setupError = errorText(e);
    } finally {
      if (mounted && setupRequest === request) {
        setupRequest = null;
        setupStage = null;
        cancellingSetup = false;
        busy = false;
      }
    }
  }

  async function cancelSetup() {
    if (!setupRequest || cancellingSetup) return;
    cancellingSetup = true;
    try {
      await api.cancelProjectSetup(tunnel.id, setupRequest);
    } catch (e) {
      if (mounted) { setupError = errorText(e); cancellingSetup = false; }
    }
  }

  async function previewSeparate() {
    if (operationBusy || separateRequest) return;
    await run(async () => {
      separatePreview = await api.previewSeparateTunnel(tunnel.id);
      if (mounted) separateOpen = true;
    });
  }

  async function createSeparate() {
    if (busy || !separatePreview || separateRequest) return;
    const target = tunnel.id;
    const name = tunnel.name;
    const revision = separatePreview.revision;
    const request = crypto.randomUUID();
    separateOpen = false;
    separateRequest = request;
    separateStage = "checking";
    busy = true;
    error = null;
    try {
      await separateSubscription;
      const result = await api.createSeparateTunnel(target, revision, request);
      await store.refresh();
      if (result.start_error) throw new Error(`The separate tunnel and DNS are ready, but the project could not start: ${result.start_error}`);
      notify.success(`${name}: separate tunnel configured. Starting project…`);
    } catch (e) {
      if (mounted) error = errorText(e);
      notify.error(e, `${name}: separate tunnel setup failed. Retry Create separate tunnel to finish.`);
      await store.refresh();
    } finally {
      if (mounted && separateRequest === request) {
        separateRequest = null;
        separateStage = null;
        busy = false;
      }
    }
  }

  const backToSeparate = returnFocusTo(() => separateTrigger);

  // Settings edits skip `busy`: disabling every control for each field change made the whole panel flash.
  async function patch(p: Parameters<typeof api.updateTunnel>[1]) {
    error = null;
    try {
      await api.updateTunnel(tunnel.id, p);
      await store.refresh();
    } catch (e) {
      error = errorText(e);
    }
  }

  async function validate() {
    await run(async () => {
      const request = validationRequest;
      const result = await api.validate(tunnel.id);
      if (mounted && request === validationRequest) {
        if (result.ok) notify.success("The config is valid.");
        else validation = result;
      }
    });
  }

  async function restart() {
    restarting = true;
    await run(() => api.restart(tunnel.id));
    restarting = false;
    if (mounted) void configCheck.check();
  }

  // Names, URLs and tokens aren't prose: no autocorrect or capitalisation, and no spelling or
  // completion bubble, whose dismissal would eat the first Escape (measured in the real app).
  const plainText = { autocorrect: "off", autocapitalize: "off", spellcheck: false, writingsuggestions: false } as const;

  // Source fields commit on Enter, then clear (the placeholder shows the saved value).
  function commitOnEnter(e: KeyboardEvent, value: string, save: (v: string) => void) {
    if (e.key === "Enter" && value.trim()) save(value.trim());
  }

  // Confirmed in the AlertDialog. On success the parent selects and focuses the next tunnel
  // (this detail unmounts), so the dialog must not hand focus back to Remove; on failure it does.
  let removing = false;
  async function remove() {
    const deleteFile = tunnel.kind === "config" && !!tunnel.config_path && deleteConfig;
    removing = true;
    removeOpen = false;
    await run(() => onremove(tunnel.id, deleteFile));
    if (error) {
      removing = false;
      removeTrigger?.focus({ preventScroll: true });
    }
  }
  const backToRemove = returnFocusTo(() => removeTrigger);
</script>

<!-- Persistent state stays inline, not a toast (02 §1.4). -->
{#snippet notice(status: Status, body: Snippet, role: "status" | "alert" = "status")}
  <div
    class="mb-3.5"
    transition:slide={{ duration: motion(180) }}
  >
    <Alert.Root variant={noticeVariant[status]} {role}>
      {#snippet icon()}
        {#if status === "connected"}<CircleCheck />
        {:else if status === "crashed"}<CircleAlert />
        {:else if status === "degraded"}<TriangleAlert />
        {:else if status === "starting"}<RotateCw />
        {:else}<Info />{/if}
      {/snippet}
      <Alert.Description class="text-foreground">{@render body()}</Alert.Description>
    </Alert.Root>
  </div>
{/snippet}

<header class="mb-3.5 flex min-w-0 items-center gap-3">
  <div class="flex min-w-0 max-w-1/2 items-center gap-2.5">
    <!-- StatusDot carries the accessible status; the tooltip adds detail. -->
    <Tip text={statusTooltip}>
      {#snippet children({ props })}<button {...props} type="button" aria-label={statusTooltip} class="flex size-5 shrink-0 items-center justify-center rounded-sm {kbRing}"><StatusDot status={s.status} /></button>{/snippet}
    </Tip>
    <EditableName name={tunnel.name} label={tunnel.kind === "config" ? "Project name" : "Tunnel name"} class="text-xl leading-none font-semibold tracking-[-0.01em]" onsave={async (name) => {
      await api.updateTunnel(tunnel.id, { name });
      await store.refresh();
    }} />
  </div>
  <span class="min-w-0 flex-1 truncate text-callout leading-none text-muted-foreground" title={headerMeta}>{headerMeta}</span>
  <div class="flex shrink-0 items-center gap-2">
    {#if on}
      <Button variant="outline" disabled={operationBusy} onclick={() => run(() => api.stop(tunnel.id))}><Square class="size-2.5 fill-current" /> Stop</Button>
    {:else}
      <Button variant="outline" disabled={operationBusy || tunnel.separate_tunnel_pending || !!tunnelOwner || tunnel.setup_needed || !store.overview?.cloudflared} onclick={() => run(() => api.start(tunnel.id))}>
        <Play class="size-2.5 fill-current" /> Start
      </Button>
    {/if}
    <DropdownMenu.Root>
      <DropdownMenu.Trigger>
        {#snippet child({ props })}
          <Button {...props} variant="outline" size="icon" aria-label="More tunnel actions"><Ellipsis class="size-3.5" /></Button>
        {/snippet}
      </DropdownMenu.Trigger>
      <DropdownMenu.Portal>
        <DropdownMenu.Content align="end" sideOffset={3} class={cn(menuContent, "min-w-32")}>
          <DropdownMenu.Item class={cn(menuItem, "ps-2")} disabled={operationBusy || !on} onSelect={restart}>
            <RotateCw class="size-3.5" /> {restarting ? "Restarting…" : "Restart"}
          </DropdownMenu.Item>
          {#if tunnel.kind === "config"}
            <DropdownMenu.Item class={cn(menuItem, "ps-2")} disabled={operationBusy} onSelect={validate}><Check class="size-3.5" /> Validate</DropdownMenu.Item>
            <DropdownMenu.Item class={cn(menuItem, "ps-2")} disabled={operationBusy || tunnel.setup_needed || !store.overview?.cloudflared} onSelect={previewSeparate}>Create separate tunnel…</DropdownMenu.Item>
          {/if}
        </DropdownMenu.Content>
      </DropdownMenu.Portal>
    </DropdownMenu.Root>
  </div>
</header>

{#if separateRequest || tunnel.separating_tunnel}
  <div class="mb-3.5" role="status">
    <Alert.Root>
      {#snippet icon()}<RotateCw />{/snippet}
      <Alert.Title>Creating separate tunnel</Alert.Title>
      <Alert.Description>{separateMessages[separateStage ?? "checking"]}</Alert.Description>
    </Alert.Root>
  </div>
{:else if sharedProjects.length || s.tunnel_conflict || tunnel.separate_tunnel_pending}
  <div class="mb-3.5" role="status">
    <Alert.Root variant="warning">
      {#snippet icon()}<TriangleAlert />{/snippet}
      <Alert.Title>{tunnel.separate_tunnel_pending ? "Separate tunnel setup is unfinished" : tunnelOwner ? `Cannot start while ${tunnelOwner.name} is running` : s.tunnel_conflict ? `Not started: ${conflictName} uses this tunnel` : `Shares a Cloudflare tunnel with ${sharedProjects.map((project) => project.name).join(", ")}`}</Alert.Title>
      <Alert.Description>{tunnel.separate_tunnel_pending ? "Retry Create separate tunnel to finish updating DNS using the saved tunnel before starting this project." : "Run one project at a time, or create a separate tunnel for this project."}</Alert.Description>
      <div class="mt-2 flex flex-wrap gap-2">
        {#if tunnel.kind === "config"}
          <Button bind:ref={separateTrigger} disabled={operationBusy || !store.overview?.cloudflared} onclick={previewSeparate}>Create separate tunnel</Button>
        {/if}
        {#if tunnelOwner ?? sharedProjects[0]}
          {@const sibling = tunnelOwner ?? sharedProjects[0]}
          <Button variant="outline" onclick={() => api.openDashboard(`tunnels:${sibling.id}`)}>View {sibling.name}</Button>
        {/if}
      </div>
    </Alert.Root>
  </div>
{/if}

<AlertDialog.Root bind:open={separateOpen}>
  <AlertDialog.Content onCloseAutoFocus={backToSeparate}>
    <AlertDialog.Header>
      <AlertDialog.Title>{separatePreview?.resuming ? "Finish creating" : "Create"} a separate tunnel for “{tunnel.name}”?</AlertDialog.Title>
      <AlertDialog.Description>Creates a Cloudflare tunnel and a separate config, switches this project to it, moves the hostname DNS records below, then starts this project. The original config file is preserved and the other project's tunnel stays running.</AlertDialog.Description>
    </AlertDialog.Header>
    {#if separatePreview}
      <div class="space-y-3 text-sm">
        <div><span class="text-muted-foreground">Config file</span><p class="break-all font-mono text-xs select-text">{separatePreview.config_path}</p></div>
        <div><span class="text-muted-foreground">Current tunnel</span><p class="break-all font-mono text-xs select-text">{separatePreview.old_tunnel_id}</p></div>
        <div><span class="text-muted-foreground">Hostnames to move</span>
          {#if separatePreview.hostnames.length}
            <ul class="mt-1 max-h-40 overflow-auto space-y-1 font-mono text-xs select-text">{#each separatePreview.hostnames as hostname}<li>{hostname}</li>{/each}</ul>
          {:else}<p>No hostname DNS records need updating.</p>{/if}
        </div>
        <p class="text-muted-foreground">This project will stop during setup. DNS changes can briefly interrupt requests to these hostnames. If setup fails, retry this action to finish using the same new tunnel.</p>
      </div>
    {/if}
    <AlertDialog.Footer>
      <AlertDialog.Cancel>Cancel</AlertDialog.Cancel>
      <Button disabled={operationBusy || !separatePreview} onclick={createSeparate}>{separatePreview?.resuming ? "Finish setup and start" : "Create separate tunnel"}</Button>
    </AlertDialog.Footer>
  </AlertDialog.Content>
</AlertDialog.Root>

{#if tunnel.setup_needed || setupRequest}
  <div class="mb-3.5" role="status">
    <Alert.Root>
      {#snippet icon()}<Info />{/snippet}
      <Alert.Title>{setupRequest ? setupStage === "login" ? "Log in to Cloudflare" : "Setting up tunnel" : "Project setup needed"}</Alert.Title>
      <Alert.Description>{cancellingSetup ? "Cancelling setup…" : setupStage ? setupMessages[setupStage] : on ? "Stop this project, then set up its Cloudflare tunnel." : "Create a Cloudflare tunnel and link its credentials automatically. You'll be asked to log in if needed."}</Alert.Description>
      {#snippet action()}
        {#if setupRequest}
          <Button variant="outline" disabled={cancellingSetup || setupStage === "linking"} onclick={cancelSetup}>Cancel setup</Button>
        {:else}
          <Button disabled={operationBusy || on || !store.overview?.cloudflared} onclick={setup}>Set up tunnel</Button>
        {/if}
      {/snippet}
    </Alert.Root>
  </div>
{:else if failure && !s.tunnel_conflict}
  {#key JSON.stringify([tunnel.id, s.run, s.status, failure])}
    <div class="mb-3.5" transition:slide={{ duration: motion(180) }}>
      <FailureNotice {failure} status={s.status} detail={s.detail} />
    </div>
  {/key}
{:else if s.detail || s.hint}
  {#snippet stateBody()}
    {#if s.detail}<div class="whitespace-pre-wrap select-text">{s.detail}</div>{/if}
    {#if s.hint}<div class="mt-1.5 whitespace-pre-wrap text-muted-foreground select-text">{s.hint}</div>{/if}
  {/snippet}
  {@render notice(s.status, stateBody, "status")}
{/if}
{#if setupError}
  <div class="mb-3.5">
    <Alert.Root variant="destructive">
      {#snippet icon()}<CircleAlert />{/snippet}
      <Alert.Description class="whitespace-pre-wrap text-foreground">{setupError}</Alert.Description>
    </Alert.Root>
  </div>
{/if}
{#if error}
  {#snippet errorBody()}<div class="whitespace-pre-wrap select-text">{error}</div>{/snippet}
  {@render notice("crashed", errorBody, "alert")}
{/if}
{#if configTarget && configChange?.status === "changed"}
  <div class="mb-3.5" transition:slide={{ duration: motion(180) }}>
    <Alert.Root variant="warning" role="status">
      {#snippet icon()}<FilePenLine />{/snippet}
      {#snippet action()}<Button variant="outline" disabled={operationBusy} onclick={restart}>{restarting ? "Restarting…" : "Restart"}</Button>{/snippet}
      <Alert.Title>Config changed on disk</Alert.Title>
      <Alert.Description>Restart to apply your changes.</Alert.Description>
    </Alert.Root>
  </div>
{/if}
{#if configTarget && (configError || configChange?.status === "unreadable")}
  {#snippet configErrorBody()}<div class="whitespace-pre-wrap select-text">{configError ?? configChange?.message ?? "Couldn't read the config file. The running config is still loaded."}</div>{/snippet}
  {@render notice("degraded", configErrorBody, "alert")}
{/if}
{#if validation && !validation.ok}
  {#key JSON.stringify(validation)}
    <div class="mb-3.5" transition:slide={{ duration: motion(180) }}>
      <FailureNotice failure={failurePresentation(validation.failure, validation.raw, validation.hint)} />
    </div>
  {/key}
{/if}

<Routes {tunnel} />

<SettingsGroup title="Behaviour">
  <SettingsRow
    label="Inspect requests"
    help={tunnel.kind === "token"
      ? "Not available for token tunnels: their hostnames live in the Cloudflare dashboard."
      : `Routes traffic through Underlane's local proxy and stores request and response bodies (up to ${store.overview?.settings.max_body_kb ?? 1024} KB each). Bodies may contain secrets.`}
  >
    {#snippet control({ id, describedBy })}
      <Switch {id} aria-describedby={describedBy} disabled={tunnel.kind === "token" || operationBusy} bind:checked={() => tunnel.inspect, (v) => patch({ inspect: v })} />
    {/snippet}
  </SettingsRow>
  <SettingsRow label="Start automatically" help="Start this tunnel whenever Underlane launches.">
    {#snippet control({ id, describedBy })}
      <Switch {id} aria-describedby={describedBy} disabled={operationBusy || tunnel.setup_needed} bind:checked={() => tunnel.autostart, (v) => patch({ autostart: v })} />
    {/snippet}
  </SettingsRow>
  <SettingsRow label="Log level" help="Debug logs every request's headers (redacted before storage).">
    {#snippet control({ id, describedBy })}
      <Select.Root type="single" value={tunnel.loglevel} onValueChange={(loglevel) => patch({ loglevel })}>
        <Select.Trigger {id} aria-describedby={describedBy} class="w-40">{tunnel.loglevel}</Select.Trigger>
        <Select.Content>
          {#each ["debug", "info", "warn", "error"] as l (l)}<Select.Item value={l} label={l} />{/each}
        </Select.Content>
      </Select.Root>
    {/snippet}
  </SettingsRow>
  <SettingsRow label="Group">
    {#snippet control({ id, labelId })}
      <!-- Named by the row label plus its own text, so it reads "Group, <name>". -->
      <div class="flex items-center gap-2.5">
        {#if tunnel.group_id !== null}
          <Button variant="link" aria-label="Remove from group" onclick={async () => {
            await patch({ group: null });
            await tick();
            document.getElementById(id)?.focus({ preventScroll: true });
          }}>Remove</Button>
        {/if}
        <Combobox
          {id}
          aria-labelledby="{labelId} {id}"
          value={tunnel.group_name}
          items={(store.overview?.groups ?? []).map((p) => p.name)}
          class="w-40"
          placeholder="Move to group…"
          onpick={(group) => patch({ group })}
        />
      </div>
    {/snippet}
  </SettingsRow>
</SettingsGroup>

{#snippet sourceNote()}
  {#if tunnel.kind === "config"}
    Starting and request inspection leave this file unchanged. Tunnel setup and hostname saves update it with a backup. Other edits are picked up the next time the tunnel starts.
  {:else}
    Quick tunnels get a new random hostname every time they start, and don't support server-sent events.
  {/if}
{/snippet}

<SettingsGroup title="Source" footer={tunnel.kind === "token" ? undefined : sourceNote}>
  <SettingsRow label="Type"><span class="text-muted-foreground">{kindText[tunnel.kind]}</span></SettingsRow>
  {#if s.cloudflare_tunnel_id ?? tunnel.cloudflare_tunnel_id}
    <SettingsRow label="Tunnel ID"><span class="break-all font-mono text-xs text-muted-foreground select-text">{s.cloudflare_tunnel_id ?? tunnel.cloudflare_tunnel_id}</span></SettingsRow>
  {/if}
  {#if tunnel.kind === "config" && tunnel.config_path}
    <SettingsRow label="Config file">
      <!-- Truncated, so the native title reveals the whole path (02 §2 Tip exception). -->
      <span class="max-w-[55%] truncate font-mono text-callout text-muted-foreground select-text" title={tunnel.config_path}>{tunnel.config_path}</span>
      <IconAction label="Open in Editor" onclick={() => api.openConfigInEditor(tunnel.id).catch((e) => notify.error(e, "Couldn't open the config file"))}><FilePenLine /></IconAction>
      <IconAction label="Show in Finder" onclick={() => revealItemInDir(tunnel.config_path!).catch((e) => notify.error(e, "Couldn't show the config file"))}><FolderOpen /></IconAction>
    </SettingsRow>
  {/if}
  {#if tunnel.kind === "quick"}
    <SettingsRow label="Local address" help="Press Return to save. A running tunnel restarts.">
      {#snippet control({ id, describedBy })}
        <Input {id} {...plainText} aria-describedby={describedBy} class="w-55 font-mono text-callout" placeholder={tunnel.origin_url ?? ""} bind:value={originDraft}
          onkeydown={(e) => commitOnEnter(e, originDraft, (originUrl) => { patch({ originUrl }); originDraft = ""; })} />
      {/snippet}
    </SettingsRow>
  {/if}
  {#if tunnel.kind === "token"}
    <SettingsRow label="Token" help={tunnel.has_token ? "Stored in the keychain." : "Missing."}>
      {#snippet control({ id, describedBy })}
        <Input {id} {...plainText} aria-describedby={describedBy} class="w-55 font-mono text-callout" type="password" placeholder="Paste a new token" bind:value={tokenDraft}
          onkeydown={(e) => commitOnEnter(e, tokenDraft, (token) => { patch({ token }); tokenDraft = ""; })} />
      {/snippet}
    </SettingsRow>
  {/if}
</SettingsGroup>

<div class="mt-5.5 flex flex-wrap items-center gap-2">
  {#if tunnel.inspect}<Button variant="ghost" onclick={() => api.openDashboard(`requests:${tunnel.id}`)}><List class="size-3.5" /> View requests</Button>{/if}
  <Button variant="ghost" onclick={() => api.openDashboard(`logs:${tunnel.id}`)}><ScrollText class="size-3.5" /> View logs</Button>
  <span class="flex-1"></span>
  <AlertDialog.Root bind:open={removeOpen}>
    <AlertDialog.Trigger bind:ref={removeTrigger}>
      {#snippet child({ props })}
        <Button {...props} variant="ghost" disabled={operationBusy} class="text-destructive hover:text-destructive aria-expanded:text-destructive"><Trash2 class="size-3.5" /> Remove</Button>
      {/snippet}
    </AlertDialog.Trigger>
    <!-- Escape/Cancel return focus to Remove explicitly (02 R7); after a removal Tunnels focuses the next tunnel. -->
    <AlertDialog.Content onCloseAutoFocus={(e) => (removing ? e.preventDefault() : backToRemove(e))}>
      <AlertDialog.Header>
        <AlertDialog.Title>Remove “{tunnel.name}”?</AlertDialog.Title>
        {#if tunnel.kind === "config"}
          <AlertDialog.Description>
            {deleteConfig ? "The config file will be permanently deleted." : "The config file stays on disk."}
          </AlertDialog.Description>
        {:else if tunnel.kind === "token"}
          <AlertDialog.Description>Its token is deleted from the keychain.</AlertDialog.Description>
        {/if}
      </AlertDialog.Header>
      {#if tunnel.kind === "config" && tunnel.config_path}
        <div class="flex items-start gap-2.5">
          <Checkbox id={deleteConfigId} bind:checked={deleteConfig} class="mt-0.5" />
          <label for={deleteConfigId} class="min-w-0 text-sm">
            Also delete the config file
            <span class="mt-1 block break-all font-mono text-xs text-muted-foreground">{tunnel.config_path}</span>
          </label>
        </div>
      {/if}
      <AlertDialog.Footer>
        <AlertDialog.Cancel>Cancel</AlertDialog.Cancel>
        <AlertDialog.Action variant="ghost" class="text-destructive hover:text-destructive hover:bg-destructive/10" onclick={remove}>Remove</AlertDialog.Action>
      </AlertDialog.Footer>
    </AlertDialog.Content>
  </AlertDialog.Root>
</div>
