<script lang="ts">
  import Check from "@lucide/svelte/icons/check";
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import CircleCheck from "@lucide/svelte/icons/circle-check";
  import Ellipsis from "@lucide/svelte/icons/ellipsis";
  import FilePenLine from "@lucide/svelte/icons/file-pen-line";
  import FolderOpen from "@lucide/svelte/icons/folder-open";
  import Info from "@lucide/svelte/icons/info";
  import Play from "@lucide/svelte/icons/play";
  import RotateCw from "@lucide/svelte/icons/rotate-cw";
  import Square from "@lucide/svelte/icons/square";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import { DropdownMenu } from "bits-ui";
  import { onMount, type Snippet, tick, untrack } from "svelte";
  import { slide } from "svelte/transition";
  import Combobox from "$lib/components/app/Combobox.svelte";
  import EditableName from "$lib/components/app/EditableName.svelte";
  import IconAction from "$lib/components/app/IconAction.svelte";
  import SettingsGroup from "$lib/components/app/SettingsGroup.svelte";
  import SettingsRow from "$lib/components/app/SettingsRow.svelte";
  import StatusDot from "$lib/components/app/StatusDot.svelte";
  import Tip from "$lib/components/app/Tip.svelte";
  import {
    type AlertVariant as AlertAlertVariant,
    Description as AlertDescription,
    Root as AlertRoot,
    Title as AlertTitle,
  } from "$lib/components/ui/alert/index.js";
  import {
    Action as AlertDialogAction,
    Cancel as AlertDialogCancel,
    Content as AlertDialogContent,
    Description as AlertDialogDescription,
    Footer as AlertDialogFooter,
    Header as AlertDialogHeader,
    Root as AlertDialogRoot,
    Title as AlertDialogTitle,
    Trigger as AlertDialogTrigger,
  } from "$lib/components/ui/alert-dialog/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Checkbox } from "$lib/components/ui/checkbox/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import {
    kbRing,
    menuContent,
    menuIndicator,
    menuItem,
  } from "$lib/components/ui/mac";
  import {
    Content as SelectContent,
    Item as SelectItem,
    Root as SelectRoot,
    Trigger as SelectTrigger,
  } from "$lib/components/ui/select/index.js";
  import { Switch } from "$lib/components/ui/switch/index.js";
  import { configChecker } from "$lib/config-change";
  import { failurePresentation } from "$lib/failure";
  import { returnFocusTo } from "$lib/focus";
  import { notify } from "$lib/toast";
  import {
    activeTunnelOwner,
    sharedTunnelProjects,
  } from "$lib/tunnel-conflict";
  import { cn } from "$lib/utils";
  import { api, errorText } from "../lib/api";
  import { formatUptime, isOn, statusHint } from "../lib/format";
  import { store } from "../lib/store.svelte";
  import { motion } from "../lib/theme";
  import type {
    ConfigChange,
    SeparateTunnelPreview,
    SeparateTunnelProgress,
    Status,
    Tunnel,
    TunnelSetupProgress,
    Validation,
  } from "../lib/types";
  import FailureNotice from "./FailureNotice.svelte";
  import Routes from "./Routes.svelte";
  import { track } from "./reactivity";

  let {
    tunnel,
    onremove,
  }: {
    tunnel: Tunnel;
    onremove: (id: string, deleteConfig: boolean) => Promise<void>;
  } = $props();
  let busy = $state(false);
  let restarting = $state(false);
  let failureMessage = $state<string | null>(null);
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
    if (!removeOpen) {
      deleteConfig = false;
    }
  });
  let removeTrigger = $state<HTMLElement | null>(null);
  let originDraft = $state("");
  let tokenDraft = $state("");
  let now = $state(Date.now());

  $effect(() => {
    const i = setInterval(() => {
      now = Date.now();
    }, 1000);
    return () => clearInterval(i);
  });

  const s = $derived(tunnel.state);
  const failure = $derived(
    s.failure || s.status === "crashed"
      ? failurePresentation(s.failure, s.detail, s.hint)
      : null
  );
  const on = $derived(isOn(tunnel));
  const sharedProjects = $derived(sharedTunnelProjects(tunnel, store.tunnels));
  const tunnelOwner = $derived(activeTunnelOwner(tunnel, store.tunnels));
  const operationBusy = $derived(busy || !!tunnel.separating_tunnel);
  const conflictName = $derived(
    tunnelOwner?.name ?? s.tunnel_conflict?.project_name
  );
  const statusTooltip = $derived(
    [
      statusHint[s.status],
      s.connections > 0
        ? `${s.connections} connection${s.connections === 1 ? "" : "s"} to Cloudflare for redundancy.`
        : "",
    ]
      .filter(Boolean)
      .join(" ")
  );
  const headerMeta = $derived(
    [
      s.status === "connected" && s.started_at
        ? formatUptime(s.started_at, now)
        : "",
      s.restarts > 0
        ? `${s.restarts} restart${s.restarts === 1 ? "" : "s"}`
        : "",
    ]
      .filter(Boolean)
      .join(" · ")
  );
  $effect(() => {
    track([tunnel.id, s.run, s.config_revision, s.status]);
    validation = null;
    validationRequest += 1;
  });
  const configTarget = $derived(
    tunnel.kind === "config" &&
      tunnel.config_path &&
      on &&
      s.status !== "stopped" &&
      s.config_revision > 0
      ? {
          config_revision: s.config_revision,
          id: tunnel.id,
          run: s.run,
          source_path: tunnel.config_path,
        }
      : null
  );
  const configKey = $derived(JSON.stringify(configTarget));
  let mounted = false;
  const configCheck = configChecker(
    () => configTarget,
    api.checkConfigChange,
    (result) => {
      configChange = result;
      configError = null;
    },
    (e) => {
      configError = `Couldn't check the config file: ${errorText(e)}`;
    }
  );

  $effect(() => {
    track(configKey);
    configChange = null;
    configError = null;
    configCheck.cancel();
    untrack(() => {
      configCheck.check();
    });
  });

  onMount(() => {
    mounted = true;
    setupSubscription = listen<TunnelSetupProgress>(
      "project-setup-progress",
      ({ payload }) => {
        if (
          mounted &&
          payload.id === tunnel.id &&
          payload.request_id === setupRequest
        ) {
          setupStage = payload.stage;
        }
      }
    );
    // Preserve the rejection for setup() without an unhandled rejection before its first click.
    setupSubscription.catch(() => {
      /* Teardown must finish even if the subscription failed. */
    });
    separateSubscription = listen<SeparateTunnelProgress>(
      "project-separate-progress",
      ({ payload }) => {
        if (
          mounted &&
          payload.id === tunnel.id &&
          payload.request_id === separateRequest
        ) {
          separateStage = payload.stage;
        }
      }
    );
    separateSubscription.catch(() => {
      /* Teardown must finish even if the subscription failed. */
    });
    const check = () => {
      configCheck.check();
      // Draft setup is completed in an external editor. Reload stopped configs on return so
      // their Start button and hostname list reflect the file without requiring a relaunch.
      if (tunnel.kind === "config" && !on) {
        store.refresh();
      }
    };
    const visible = () => {
      if (document.visibilityState === "visible") {
        check();
      }
    };
    window.addEventListener("focus", check);
    document.addEventListener("visibilitychange", visible);
    return () => {
      mounted = false;
      if (setupRequest) {
        api.cancelProjectSetup(tunnel.id, setupRequest).catch(() => {
          /* Teardown must finish even if the subscription failed. */
        });
      }
      setupSubscription
        ?.then((unsubscribe) => unsubscribe())
        .catch(() => {
          /* Teardown must finish even if the subscription failed. */
        });
      separateSubscription
        ?.then((unsubscribe) => unsubscribe())
        .catch(() => {
          /* Teardown must finish even if the subscription failed. */
        });
      window.removeEventListener("focus", check);
      document.removeEventListener("visibilitychange", visible);
      configCheck.cancel();
    };
  });
  const kindText = {
    config: "Project from a config file",
    quick: "Quick tunnel on trycloudflare.com",
    token: "Remotely managed tunnel (token)",
  };
  const setupMessages: Record<TunnelSetupProgress["stage"], string> = {
    checking: "Checking the project config and Cloudflare login…",
    creating: "Creating your Cloudflare tunnel…",
    linking: "Linking the tunnel ID and credentials to this project…",
    login:
      "Complete Cloudflare login in your browser. Setup will continue when you return.",
  };
  const separateMessages: Record<SeparateTunnelProgress["stage"], string> = {
    checking: "Checking the config, credentials and hostname ownership…",
    creating: "Creating a separate Cloudflare tunnel…",
    dns: "Moving this project's hostname DNS records to the new tunnel…",
    linking: "Updating this project to use the new tunnel…",
    starting: "Starting this project with its separate tunnel…",
  };
  const noticeVariant: Record<Status, AlertAlertVariant> = {
    connected: "success",
    crashed: "destructive",
    degraded: "warning",
    starting: "default",
    stopped: "default",
  };

  async function run(fn: () => Promise<unknown>) {
    busy = true;
    failureMessage = null;
    validation = null;
    validationRequest += 1;
    try {
      await fn();
    } catch (e) {
      failureMessage = errorText(e);
    } finally {
      busy = false;
    }
  }

  async function setup() {
    if (busy || setupRequest || !tunnel.setup_needed) {
      return;
    }
    const target = tunnel.id;
    const request = crypto.randomUUID();
    setupRequest = request;
    setupStage = "checking";
    setupError = null;
    failureMessage = null;
    busy = true;
    try {
      await setupSubscription;
      if (!mounted) {
        return;
      }
      await api.setupProjectTunnel(target, request);
      if (!mounted || setupRequest !== request) {
        return;
      }
      await store.refresh();
      if (store.error) {
        throw new Error(
          `The tunnel was set up, but the project could not refresh: ${store.error}`
        );
      }
      if (mounted) {
        notify.success(
          "Tunnel set up. Configure hostnames, then start the project."
        );
      }
    } catch (e) {
      if (mounted && setupRequest === request) {
        setupError = errorText(e);
      }
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
    if (!setupRequest || cancellingSetup) {
      return;
    }
    cancellingSetup = true;
    try {
      await api.cancelProjectSetup(tunnel.id, setupRequest);
    } catch (e) {
      if (mounted) {
        setupError = errorText(e);
        cancellingSetup = false;
      }
    }
  }

  async function previewSeparate() {
    if (operationBusy || separateRequest) {
      return;
    }
    await run(async () => {
      separatePreview = await api.previewSeparateTunnel(tunnel.id);
      if (mounted) {
        separateOpen = true;
      }
    });
  }

  async function createSeparate() {
    if (busy || !separatePreview || separateRequest) {
      return;
    }
    const target = tunnel.id;
    const { name } = tunnel;
    const { revision } = separatePreview;
    const request = crypto.randomUUID();
    separateOpen = false;
    separateRequest = request;
    separateStage = "checking";
    busy = true;
    failureMessage = null;
    try {
      await separateSubscription;
      const result = await api.createSeparateTunnel(target, revision, request);
      await store.refresh();
      if (result.start_error) {
        throw new Error(
          `The separate tunnel and DNS are ready, but the project could not start: ${result.start_error}`
        );
      }
      notify.success(`${name}: separate tunnel configured. Starting project…`);
    } catch (e) {
      if (mounted) {
        failureMessage = errorText(e);
      }
      notify.error(
        e,
        `${name}: separate tunnel setup failed. Retry Create separate tunnel to finish.`
      );
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
    failureMessage = null;
    try {
      await api.updateTunnel(tunnel.id, p);
      await store.refresh();
    } catch (e) {
      failureMessage = errorText(e);
    }
  }

  async function validate() {
    await run(async () => {
      const request = validationRequest;
      const result = await api.validate(tunnel.id);
      if (mounted && request === validationRequest) {
        if (result.ok) {
          notify.success("The config is valid.");
        } else {
          validation = result;
        }
      }
    });
  }

  async function restart() {
    restarting = true;
    await run(() => api.restart(tunnel.id));
    restarting = false;
    if (mounted) {
      configCheck.check();
    }
  }

  // Names, URLs and tokens aren't prose: no autocorrect or capitalisation, and no spelling or
  // completion bubble, whose dismissal would eat the first Escape (measured in the real app).
  const plainText = {
    autocapitalize: "off",
    autocorrect: "off",
    spellcheck: false,
    writingsuggestions: false,
  } as const;

  // Source fields commit on Enter, then clear (the placeholder shows the saved value).
  function commitOnEnter(
    e: KeyboardEvent,
    value: string,
    save: (v: string) => void
  ) {
    if (e.key === "Enter" && value.trim()) {
      save(value.trim());
    }
  }

  // Confirmed in the AlertDialog. On success the parent selects and focuses the next tunnel
  // (this detail unmounts), so the dialog must not hand focus back to Remove; on failure it does.
  let removing = false;
  async function remove() {
    const deleteFile =
      tunnel.kind === "config" && !!tunnel.config_path && deleteConfig;
    removing = true;
    removeOpen = false;
    await run(() => onremove(tunnel.id, deleteFile));
    if (failureMessage) {
      removing = false;
      removeTrigger?.focus({ preventScroll: true });
    }
  }
  const backToRemove = returnFocusTo(() => removeTrigger);

  const sharedTunnelTitle = $derived.by(() => {
    if (tunnel.separate_tunnel_pending) {
      return "Separate tunnel setup is unfinished";
    }
    if (tunnelOwner) {
      return `Cannot start while ${tunnelOwner.name} is running`;
    }
    if (s.tunnel_conflict) {
      return `Not started: ${conflictName} uses this tunnel`;
    }
    return `Shares a Cloudflare tunnel with ${sharedProjects.map((project) => project.name).join(", ")}`;
  });

  const setupTitle = $derived.by(() => {
    if (setupRequest) {
      if (setupStage === "login") {
        return "Log in to Cloudflare";
      }
      return "Setting up tunnel";
    }
    return "Project setup needed";
  });

  const setupDescription = $derived.by(() => {
    if (cancellingSetup) {
      return "Cancelling setup…";
    }
    if (setupStage) {
      return setupMessages[setupStage];
    }
    if (on) {
      return "Stop this project, then set up its Cloudflare tunnel.";
    }
    return "Create a Cloudflare tunnel and link its credentials automatically. You'll be asked to log in if needed.";
  });
</script>
{#snippet setupNotice()}
  <div class="mb-3.5" role="status">
    <AlertRoot>
      {#snippet icon()}
        <Info />
      {/snippet}
      <AlertTitle>{setupTitle}</AlertTitle>
      <AlertDescription>{setupDescription}</AlertDescription>
      {#snippet action()}
        {#if setupRequest}
          <Button
            disabled={cancellingSetup || setupStage === "linking"}
            onclick={cancelSetup}
            variant="outline"
            >Cancel setup</Button
          >
        {:else}
          <Button
            disabled={operationBusy || on || !store.overview?.cloudflared}
            onclick={setup}
            >Set up tunnel</Button
          >
        {/if}
      {/snippet}
    </AlertRoot>
  </div>
{/snippet}

{#snippet detailActions()}
  <div class="mt-5.5 flex justify-end">
    <AlertDialogRoot bind:open={removeOpen}>
      <AlertDialogTrigger bind:ref={removeTrigger}>
        {#snippet child({
          props,
        })}
          <Button
            {...props}
            class="text-destructive hover:text-destructive aria-expanded:text-destructive"
            disabled={operationBusy}
            variant="ghost"
            ><Trash2 class="size-3.5" />
            Remove</Button
          >
        {/snippet}
      </AlertDialogTrigger>
      <!-- Escape/Cancel return focus to Remove explicitly (02 R7); after a removal Tunnels focuses the next tunnel. -->
      <AlertDialogContent
        onCloseAutoFocus={(e) =>
          removing ? e.preventDefault() : backToRemove(e)}
      >
        <AlertDialogHeader>
          <AlertDialogTitle>Remove “{tunnel.name}”?</AlertDialogTitle>
          {#if tunnel.kind === "config"}
            <AlertDialogDescription>
              {deleteConfig
                ? "The config file will be permanently deleted."
                : "The config file stays on disk."}
            </AlertDialogDescription>
          {:else if tunnel.kind === "token"}
            <AlertDialogDescription
              >Its token is deleted from the keychain.</AlertDialogDescription
            >
          {/if}
        </AlertDialogHeader>
        {#if tunnel.kind === "config" && tunnel.config_path}
          <div class="flex items-start gap-2.5">
            <Checkbox
              class="mt-0.5"
              id={deleteConfigId}
              bind:checked={deleteConfig}
            />
            <label class="min-w-0 text-sm" for={deleteConfigId}>
              Also delete the config file
              <span
                class="mt-1 block break-all font-mono text-xs text-muted-foreground"
                >{tunnel.config_path}</span
              >
            </label>
          </div>
        {/if}
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          <AlertDialogAction
            class="text-destructive hover:text-destructive hover:bg-destructive/10"
            onclick={remove}
            variant="ghost"
            >Remove</AlertDialogAction
          >
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialogRoot>
  </div>
{/snippet}

{#snippet sourceSettings()}
  <SettingsGroup
    footer={tunnel.kind === "token" ? undefined : sourceNote}
    title="Source"
  >
    <SettingsRow label="Type"
      ><span class="text-muted-foreground"
        >{kindText[tunnel.kind]}</span
      ></SettingsRow
    >
    {#if s.cloudflare_tunnel_id ?? tunnel.cloudflare_tunnel_id}
      <SettingsRow label="Tunnel ID"
        ><span
          class="break-all font-mono text-xs text-muted-foreground select-text"
          >{s.cloudflare_tunnel_id ?? tunnel.cloudflare_tunnel_id}</span
        ></SettingsRow
      >
    {/if}
    {#if tunnel.kind === "config" && tunnel.config_path}
      {@const configPath = tunnel.config_path}
      <SettingsRow label="Config file">
        <!-- Truncated, so the native title reveals the whole path (02 §2 Tip exception). -->
        <span
          class="max-w-[55%] truncate font-mono text-callout text-muted-foreground select-text"
          title={tunnel.config_path}
          >{tunnel.config_path}</span
        >
        <IconAction
          label="Open in Editor"
          onclick={() =>
            api
              .openConfigInEditor(tunnel.id)
              .catch((e) => notify.error(e, "Couldn't open the config file"))}
          ><FilePenLine /></IconAction
        >
        <IconAction
          label="Show in Finder"
          onclick={() =>
            revealItemInDir(configPath).catch((e) =>
              notify.error(e, "Couldn't show the config file")
            )}
          ><FolderOpen /></IconAction
        >
      </SettingsRow>
    {/if}
    {#if tunnel.kind === "quick"}
      <SettingsRow
        help="Press Return to save. A running tunnel restarts."
        label="Local address"
      >
        {#snippet control({
          id,
          describedBy,
        })}
          <Input
            {id}
            {...plainText}
            aria-describedby={describedBy}
            class="w-55 font-mono text-callout"
            onkeydown={(e) =>
              commitOnEnter(e, originDraft, (originUrl) => {
                patch({ originUrl });
                originDraft = "";
              })}
            placeholder={tunnel.origin_url ?? ""}
            bind:value={originDraft}
          />
        {/snippet}
      </SettingsRow>
    {/if}
    {#if tunnel.kind === "token"}
      <SettingsRow
        help={tunnel.has_token ? "Stored in the keychain." : "Missing."}
        label="Token"
      >
        {#snippet control({
          id,
          describedBy,
        })}
          <Input
            {id}
            {...plainText}
            aria-describedby={describedBy}
            class="w-55 font-mono text-callout"
            onkeydown={(e) =>
              commitOnEnter(e, tokenDraft, (token) => {
                patch({ token });
                tokenDraft = "";
              })}
            placeholder="Paste a new token"
            type="password"
            bind:value={tokenDraft}
          />
        {/snippet}
      </SettingsRow>
    {/if}
  </SettingsGroup>
{/snippet}

{#snippet behaviorSettings()}
  <SettingsGroup title="Behaviour">
    <SettingsRow
      help={tunnel.kind === "token"
        ? "Not available for token tunnels: their hostnames live in the Cloudflare dashboard."
        : `Routes traffic through Underlane's local proxy and stores request and response bodies (up to ${store.overview?.settings.max_body_kb ?? 1024} KB each). Bodies may contain secrets.`}
      label="Inspect requests"
    >
      {#snippet control({
        id,
        describedBy,
      })}
        <Switch
          aria-describedby={describedBy}
          disabled={tunnel.kind === "token" || operationBusy}
          {id}
          bind:checked={() => tunnel.inspect, (v) => patch({ inspect: v })}
        />
      {/snippet}
    </SettingsRow>
    <SettingsRow
      help="Start this tunnel whenever Underlane launches."
      label="Start automatically"
    >
      {#snippet control({
        id,
        describedBy,
      })}
        <Switch
          aria-describedby={describedBy}
          disabled={operationBusy || tunnel.setup_needed}
          {id}
          bind:checked={() => tunnel.autostart, (v) => patch({ autostart: v })}
        />
      {/snippet}
    </SettingsRow>
    <SettingsRow
      help="Debug logs every request's headers (redacted before storage)."
      label="Log level"
    >
      {#snippet control({
        id,
        describedBy,
      })}
        <SelectRoot
          onValueChange={(loglevel) => patch({ loglevel })}
          type="single"
          value={tunnel.loglevel}
        >
          <SelectTrigger aria-describedby={describedBy} class="w-40" {id}
            >{tunnel.loglevel}</SelectTrigger
          >
          <SelectContent>
            {#each ["debug", "info", "warn", "error"] as l (l)}
              <SelectItem label={l} value={l} />
            {/each}
          </SelectContent>
        </SelectRoot>
      {/snippet}
    </SettingsRow>
    <SettingsRow label="Group">
      {#snippet control({
        id,
        labelId,
      })}
        <!-- Named by the row label plus its own text, so it reads "Group, <name>". -->
        <div class="flex items-center gap-2.5">
          {#if tunnel.group_id !== null}
            <Button
              aria-label="Remove from group"
              onclick={async () => {
                await patch({ group: null });
                await tick();
                document.getElementById(id)?.focus({ preventScroll: true });
              }}
              variant="link"
              >Remove</Button
            >
          {/if}
          <Combobox
            aria-labelledby="{labelId} {id}"
            class="w-40"
            {id}
            items={(store.overview?.groups ?? []).map((p) => p.name)}
            onpick={(group) => patch({ group })}
            placeholder="Move to group…"
            value={tunnel.group_name}
          />
        </div>
      {/snippet}
    </SettingsRow>
  </SettingsGroup>
{/snippet}

{#snippet connectionNotice()}
  {#if tunnel.setup_needed || setupRequest}
    {@render setupNotice()}
  {:else if failure && !s.tunnel_conflict}
    {#key JSON.stringify([tunnel.id, s.run, s.status, failure])}
      <div class="mb-3.5" transition:slide={{ duration: motion(180) }}>
        <FailureNotice detail={s.detail} {failure} status={s.status} />
      </div>
    {/key}
  {:else if s.detail || s.hint}
    {#snippet stateBody()}
      {#if s.detail}
        <div class="whitespace-pre-wrap select-text">{s.detail}</div>
      {/if}
      {#if s.hint}
        <div
          class="mt-1.5 whitespace-pre-wrap text-muted-foreground select-text"
        >
          {s.hint}
        </div>
      {/if}
    {/snippet}
    {@render notice(s.status, stateBody, "status")}
  {/if}
{/snippet}

{#snippet separateConfirmation()}
  <AlertDialogRoot bind:open={separateOpen}>
    <AlertDialogContent onCloseAutoFocus={backToSeparate}>
      <AlertDialogHeader>
        <AlertDialogTitle
          >{separatePreview?.resuming ? "Finish creating" : "Create"}
          a separate tunnel for “{tunnel.name}”?</AlertDialogTitle
        >
        <AlertDialogDescription
          >Creates a Cloudflare tunnel and a separate config, switches this
          project to it, moves the hostname DNS records below, then starts this
          project. The original config file is preserved and the other project's
          tunnel stays running.</AlertDialogDescription
        >
      </AlertDialogHeader>
      {#if separatePreview}
        <div class="space-y-3 text-sm">
          <div>
            <span class="text-muted-foreground">Config file</span>
            <p class="break-all font-mono text-xs select-text">
              {separatePreview.config_path}
            </p>
          </div>
          <div>
            <span class="text-muted-foreground">Current tunnel</span>
            <p class="break-all font-mono text-xs select-text">
              {separatePreview.old_tunnel_id}
            </p>
          </div>
          <div>
            <span class="text-muted-foreground">Hostnames to move</span>
            {#if separatePreview.hostnames.length}
              <ul
                class="mt-1 max-h-40 overflow-auto space-y-1 font-mono text-xs select-text"
              >
                {#each separatePreview.hostnames as hostname}
                  <li>{hostname}</li>
                {/each}
              </ul>
            {:else}
              <p>No hostname DNS records need updating.</p>
            {/if}
          </div>
          <p class="text-muted-foreground">
            This project will stop during setup. DNS changes can briefly
            interrupt requests to these hostnames. If setup fails, retry this
            action to finish using the same new tunnel.
          </p>
        </div>
      {/if}
      <AlertDialogFooter>
        <AlertDialogCancel>Cancel</AlertDialogCancel>
        <Button
          disabled={operationBusy || !separatePreview}
          onclick={createSeparate}
          >{separatePreview?.resuming
            ? "Finish setup and start"
            : "Create separate tunnel"}</Button
        >
      </AlertDialogFooter>
    </AlertDialogContent>
  </AlertDialogRoot>
{/snippet}

{#snippet sharedTunnelNotice()}
  {#if separateRequest || tunnel.separating_tunnel}
    <div class="mb-3.5" role="status">
      <AlertRoot>
        {#snippet icon()}
          <RotateCw />
        {/snippet}
        <AlertTitle>Creating separate tunnel</AlertTitle>
        <AlertDescription
          >{separateMessages[separateStage ?? "checking"]}</AlertDescription
        >
      </AlertRoot>
    </div>
  {:else if sharedProjects.length ||
    s.tunnel_conflict ||
    tunnel.separate_tunnel_pending}
    <div class="mb-3.5" role="status">
      <AlertRoot variant="warning">
        {#snippet icon()}
          <TriangleAlert />
        {/snippet}
        <AlertTitle>{sharedTunnelTitle}</AlertTitle>
        <AlertDescription
          >{tunnel.separate_tunnel_pending
            ? "Retry Create separate tunnel to finish updating DNS using the saved tunnel before starting this project."
            : "Run one project at a time, or create a separate tunnel for this project."}</AlertDescription
        >
        <div class="mt-2 flex flex-wrap gap-2">
          {#if tunnel.kind === "config"}
            <Button
              disabled={operationBusy || !store.overview?.cloudflared}
              onclick={previewSeparate}
              bind:ref={separateTrigger}
              >Create separate tunnel</Button
            >
          {/if}
          {#if tunnelOwner ?? sharedProjects[0]}
            {@const sibling = tunnelOwner ?? sharedProjects[0]}
            <Button
              onclick={() => api.openDashboard(`tunnels:${sibling.id}`)}
              variant="outline"
              >View {sibling.name}</Button
            >
          {/if}
        </div>
      </AlertRoot>
    </div>
  {/if}
{/snippet}

{#snippet detailHeader()}
  <header
    class="flex h-(--toolbar-h) shrink-0 items-center gap-3 border-b-[0.5px] border-border pr-4 pl-5"
    data-tauri-drag-region
  >
    <div class="flex min-w-0 max-w-1/2 items-center gap-2.5">
      <!-- StatusDot carries the accessible status; the tooltip adds detail. -->
      <Tip text={statusTooltip}>
        {#snippet children({
          props,
        })}
          <button
            type="button"
            {...props}
            aria-label={statusTooltip}
            class="flex size-5 shrink-0 items-center justify-center rounded-sm {kbRing}"
          >
            <StatusDot status={s.status} />
          </button>
        {/snippet}
      </Tip>
      <h1 class="sr-only">{tunnel.name}</h1>
      <EditableName
        class="text-lg leading-none font-semibold tracking-[-0.01em]"
        label={tunnel.kind === "config" ? "Project name" : "Tunnel name"}
        name={tunnel.name}
        onsave={async (name) => {
          await api.updateTunnel(tunnel.id, { name });
          await store.refresh();
        }}
      />
    </div>
    <span
      class="min-w-0 flex-1 truncate text-callout leading-none text-muted-foreground"
      data-tauri-drag-region
      title={headerMeta}
      >{headerMeta}</span
    >
    <div class="flex shrink-0 items-center gap-2">
      {#if on}
        <Button
          disabled={operationBusy}
          onclick={() => run(() => api.stop(tunnel.id))}
          variant="outline"
          ><Square class="size-2.5 fill-current" />
          Stop</Button
        >
      {:else}
        <Button
          disabled={operationBusy ||
            tunnel.separate_tunnel_pending ||
            !!tunnelOwner ||
            tunnel.setup_needed ||
            !store.overview?.cloudflared}
          onclick={() => run(() => api.start(tunnel.id))}
          variant="outline"
        >
          <Play class="size-2.5 fill-current" />
          Start
        </Button>
      {/if}
      <DropdownMenu.Root>
        <DropdownMenu.Trigger>
          {#snippet child({
            props,
          })}
            <Button
              {...props}
              aria-label="More tunnel actions"
              size="icon"
              variant="outline"
              ><Ellipsis class="size-3.5" /></Button
            >
          {/snippet}
        </DropdownMenu.Trigger>
        <DropdownMenu.Portal>
          <DropdownMenu.Content
            align="end"
            class={cn(menuContent, "min-w-32")}
            sideOffset={3}
          >
            <DropdownMenu.Item
              class={cn(menuItem, "ps-2")}
              disabled={operationBusy || !on}
              onSelect={restart}
            >
              <RotateCw class="size-3.5" />
              {restarting ? "Restarting…" : "Restart"}
            </DropdownMenu.Item>
            <DropdownMenu.Sub>
              <DropdownMenu.SubTrigger
                class={cn(menuItem, "ps-2")}
                disabled={operationBusy}
              >
                <FolderOpen class="size-3.5" />Move to group<ChevronRight
                  class="ml-auto size-3.5"
                />
              </DropdownMenu.SubTrigger>
              <DropdownMenu.Portal>
                <DropdownMenu.SubContent
                  aria-label="Move project to group"
                  class={cn(menuContent, "max-h-80")}
                  sideOffset={6}
                >
                  <DropdownMenu.RadioGroup
                    onValueChange={(id) => {
                      const group = store.overview?.groups.find(
                        (item) => item.id === id
                      );
                      if (id === "*" || group) {
                        patch({ group: group?.name ?? null });
                      }
                    }}
                    value={tunnel.group_id ?? "*"}
                  >
                    {#each store.overview?.groups ?? [] as group (group.id)}
                      <DropdownMenu.RadioItem class={menuItem} value={group.id}>
                        <span class={menuIndicator}
                          >{#if tunnel.group_id === group.id}
                            <Check />
                          {/if}</span
                        >{group.name}
                      </DropdownMenu.RadioItem>
                    {/each}
                    <DropdownMenu.RadioItem class={menuItem} value="*">
                      <span class={menuIndicator}
                        >{#if tunnel.group_id === null}
                          <Check />
                        {/if}</span
                      >No group
                    </DropdownMenu.RadioItem>
                  </DropdownMenu.RadioGroup>
                </DropdownMenu.SubContent>
              </DropdownMenu.Portal>
            </DropdownMenu.Sub>
            {#if tunnel.kind === "config"}
              <DropdownMenu.Item
                class={cn(menuItem, "ps-2")}
                disabled={operationBusy}
                onSelect={validate}
                ><Check class="size-3.5" />
                Validate</DropdownMenu.Item
              >
              <DropdownMenu.Item
                class={cn(menuItem, "ps-2")}
                disabled={operationBusy ||
                  tunnel.setup_needed ||
                  !store.overview?.cloudflared}
                onSelect={previewSeparate}
                >Create separate tunnel…</DropdownMenu.Item
              >
            {/if}
          </DropdownMenu.Content>
        </DropdownMenu.Portal>
      </DropdownMenu.Root>
    </div>
  </header>
{/snippet}

<!-- Persistent state stays inline, not a toast (02 §1.4). -->
{#snippet notice(
  status: Status,
  body: Snippet,
  role: "status" | "alert" = "status"
)}
  <div class="mb-3.5" transition:slide={{ duration: motion(180) }}>
    <AlertRoot {role} variant={noticeVariant[status]}>
      {#snippet icon()}
        {#if status === "connected"}
          <CircleCheck />
        {:else if status === "crashed"}
          <CircleAlert />
        {:else if status === "degraded"}
          <TriangleAlert />
        {:else if status === "starting"}
          <RotateCw />
        {:else}
          <Info />
        {/if}
      {/snippet}
      <AlertDescription class="text-foreground"
        >{@render body()}</AlertDescription
      >
    </AlertRoot>
  </div>
{/snippet}

{#snippet sourceNote()}
  {#if tunnel.kind === "config"}
    Starting and request inspection leave this file unchanged. Tunnel setup and
    hostname saves update it with a backup. Other edits are picked up the next
    time the tunnel starts.
  {:else}
    Quick tunnels get a new random hostname every time they start, and don't
    support server-sent events.
  {/if}
{/snippet}

<div class="flex min-h-0 flex-1 flex-col">
  {@render detailHeader()}
  {#if failureMessage}
    <div class="mx-auto w-full max-w-[880px] shrink-0 px-8 pt-4">
      {#snippet errorBody()}
        <div class="whitespace-pre-wrap select-text">{failureMessage}</div>
      {/snippet}
      {@render notice("crashed", errorBody, "alert")}
    </div>
  {/if}

  {@render separateConfirmation()}

  <div class="min-h-0 flex-1 overflow-y-auto">
    <div class="mx-auto max-w-[880px] px-8 pt-4 pb-10">
      {@render sharedTunnelNotice()}

      {@render connectionNotice()}
      {#if setupError}
        <div class="mb-3.5">
          <AlertRoot variant="destructive">
            {#snippet icon()}
              <CircleAlert />
            {/snippet}
            <AlertDescription class="whitespace-pre-wrap text-foreground"
              >{setupError}</AlertDescription
            >
          </AlertRoot>
        </div>
      {/if}
      {#if configTarget && configChange?.status === "changed"}
        <div class="mb-3.5" transition:slide={{ duration: motion(180) }}>
          <AlertRoot role="status" variant="warning">
            {#snippet icon()}
              <FilePenLine />
            {/snippet}
            {#snippet action()}
              <Button
                disabled={operationBusy}
                onclick={restart}
                variant="outline"
                >{restarting ? "Restarting…" : "Restart"}</Button
              >
            {/snippet}
            <AlertTitle>Config changed on disk</AlertTitle>
            <AlertDescription>Restart to apply your changes.</AlertDescription>
          </AlertRoot>
        </div>
      {/if}
      {#if configTarget &&
        (configError || configChange?.status === "unreadable")}
        {#snippet configErrorBody()}
          <div class="whitespace-pre-wrap select-text">
            {configError ??
              configChange?.message ??
              "Couldn't read the config file. The running config is still loaded."}
          </div>
        {/snippet}
        {@render notice("degraded", configErrorBody, "alert")}
      {/if}
      {#if validation && !validation.ok}
        {#key JSON.stringify(validation)}
          <div class="mb-3.5" transition:slide={{ duration: motion(180) }}>
            <FailureNotice
              failure={failurePresentation(
                validation.failure,
                validation.raw,
                validation.hint
              )}
            />
          </div>
        {/key}
      {/if}

      <Routes {tunnel} />

      {@render behaviorSettings()}

      {@render sourceSettings()}

      {@render detailActions()}
    </div>
  </div>
</div>
