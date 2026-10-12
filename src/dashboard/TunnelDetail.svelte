<script lang="ts">
  import { tick, type Snippet } from "svelte";
  import { mergeProps } from "bits-ui";
  import { slide } from "svelte/transition";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import Check from "@lucide/svelte/icons/check";
  import FolderOpen from "@lucide/svelte/icons/folder-open";
  import List from "@lucide/svelte/icons/list";
  import Play from "@lucide/svelte/icons/play";
  import RotateCw from "@lucide/svelte/icons/rotate-cw";
  import ScrollText from "@lucide/svelte/icons/scroll-text";
  import Square from "@lucide/svelte/icons/square";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import { store } from "../lib/store.svelte";
  import { api, errorText } from "../lib/api";
  import { motion } from "../lib/theme";
  import { formatUptime, isOn, statusHint, statusLabel } from "../lib/format";
  import type { Status, Tunnel, Validation } from "../lib/types";
  import { notify } from "$lib/toast";
  import { returnFocusTo } from "$lib/focus";
  import StatusDot from "$lib/components/app/StatusDot.svelte";
  import Tip from "$lib/components/app/Tip.svelte";
  import IconAction from "$lib/components/app/IconAction.svelte";
  import Combobox from "$lib/components/app/Combobox.svelte";
  import SettingsGroup from "$lib/components/app/SettingsGroup.svelte";
  import SettingsRow from "$lib/components/app/SettingsRow.svelte";
  import { kbRing } from "$lib/components/ui/mac";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import { Switch } from "$lib/components/ui/switch/index.js";
  import * as Select from "$lib/components/ui/select/index.js";
  import * as AlertDialog from "$lib/components/ui/alert-dialog/index.js";
  import Routes from "./Routes.svelte";

  let { tunnel, onremove }: { tunnel: Tunnel; onremove: (id: string) => Promise<void> } = $props();
  let busy = $state(false);
  let error = $state<string | null>(null);
  let validation = $state<Validation | null>(null);
  let removeOpen = $state(false);
  let removeTrigger = $state<HTMLElement | null>(null);
  let nameButton = $state<HTMLElement | null>(null);
  let nameInput = $state<HTMLInputElement | null>(null);
  let nameDraft = $state("");
  let originDraft = $state("");
  let tokenDraft = $state("");
  let now = $state(Date.now());

  $effect(() => {
    const i = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(i);
  });

  const s = $derived(tunnel.state);
  const on = $derived(isOn(tunnel));
  const kindText = { config: "Named tunnel from a config file", token: "Remotely managed tunnel (token)", quick: "Quick tunnel on trycloudflare.com" };
  // Status words use the AA text shades of the status colours (01 §3.3).
  const statusText: Record<Status, string> = {
    connected: "text-ok-text",
    starting: "text-starting-text",
    degraded: "text-degraded-text",
    crashed: "text-bad-text",
    stopped: "text-muted-foreground",
  };
  const border: Record<Status, string> = {
    connected: "border-s-ok",
    starting: "border-s-starting",
    degraded: "border-s-degraded",
    crashed: "border-s-bad",
    stopped: "border-s-off",
  };

  async function run(fn: () => Promise<unknown>) {
    busy = true;
    error = null;
    try {
      await fn();
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = false;
    }
  }

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
      validation = await api.validate(tunnel.id);
    });
  }

  // Rename (02 F7). Enter commits once, Escape never saves, and a blur commits only while still
  // editing (so the Enter/Escape that closed the field can't save again). Focus goes back to
  // the name only after Enter or Escape; a blur leaves it where the user clicked or tabbed.
  // Only an edit saves: comparing with the name at edit start (not the live one) keeps a rename
  // made elsewhere meanwhile from being overwritten by an untouched field.
  let mode = $state<"idle" | "editing" | "done">("idle");
  let nameAtStart = "";
  const renameHintId = $props.id();

  async function startRename() {
    nameDraft = nameAtStart = tunnel.name;
    mode = "editing";
    await tick();
    nameInput?.focus();
    nameInput?.select();
  }

  function commit() {
    if (mode !== "editing") return;
    mode = "done";
    const name = nameDraft.trim();
    if (name && name !== nameAtStart) patch({ name });
  }

  async function close(refocus: boolean) {
    if (refocus) {
      await tick();
      nameButton?.focus();
    }
    mode = "idle";
  }

  function onNameKey(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      commit();
      close(true);
    } else if (e.key === "Escape") {
      e.preventDefault();
      mode = "done";
      close(true);
    }
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
    removing = true;
    removeOpen = false;
    await run(() => onremove(tunnel.id));
    if (error) {
      removing = false;
      removeTrigger?.focus({ preventScroll: true });
    }
  }
  const backToRemove = returnFocusTo(() => removeTrigger);
</script>

<!-- Persistent state stays inline, not a toast (02 §1.4). One-offs: the 3px status edge and the
     looser line height of today's notices. -->
{#snippet notice(status: Status, body: Snippet, role?: "status" | "alert")}
  <div
    class="mb-3.5 rounded-lg border-[0.5px] border-s-[3px] border-border bg-card px-3 py-2.5 text-callout leading-[1.45] {border[status]}"
    {role}
    transition:slide={{ duration: motion(180) }}
  >
    {@render body()}
  </div>
{/snippet}

<header class="mb-3.5 flex flex-col gap-1.5">
  <div class="flex items-center gap-2.5">
    <!-- The status word follows in text, so the dot's hint is hover-only (no tab stop). -->
    <Tip text={statusHint[s.status]}>
      {#snippet children({ props })}<span {...{ ...props, tabindex: undefined }} class="flex"><StatusDot status={s.status} size={10} /></span>{/snippet}
    </Tip>
    {#if mode === "editing"}
      <Input
        bind:ref={nameInput}
        bind:value={nameDraft}
        {...plainText}
        aria-label="Tunnel name"
        class="h-7.5 w-70 text-lg"
        onkeydown={onNameKey}
        onblur={() => {
          if (mode === "editing") {
            commit();
            close(false);
          }
        }}
      />
    {:else}
      <!-- A permanent "Rename" description: the Tip's aria-describedby exists only while it's open.
           -0.01em: the title's tighter tracking, a one-off. -->
      <Tip text="Rename">
        {#snippet children({ props })}
          <button
            {...mergeProps(props, { onclick: startRename, "aria-describedby": renameHintId })}
            bind:this={nameButton}
            class="cursor-text rounded-sm text-left text-xl font-semibold tracking-[-0.01em] {kbRing}">{tunnel.name}</button
          >
        {/snippet}
      </Tip>
      <span id={renameHintId} class="sr-only">Rename</span>
    {/if}
  </div>
  <div class="flex gap-3 ps-5 text-callout">
    <span class={statusText[s.status]}>{statusLabel[s.status]}</span>
    {#if s.status === "connected" && s.started_at}<span class="text-muted-foreground">for {formatUptime(s.started_at, now)}</span>{/if}
    {#if s.connections > 0}<span class="text-muted-foreground">{s.connections} edge connection{s.connections === 1 ? "" : "s"}</span>{/if}
    {#if s.restarts > 0}<span class="text-muted-foreground">{s.restarts} restart{s.restarts === 1 ? "" : "s"}</span>{/if}
  </div>
  <div class="mt-2 flex flex-wrap gap-2">
    {#if on}
      <Button variant="outline" disabled={busy} onclick={() => run(() => api.stop(tunnel.id))}><Square class="size-2.5 fill-current" /> Stop</Button>
      <Button variant="outline" disabled={busy} onclick={() => run(() => api.restart(tunnel.id))}><RotateCw class="size-3.5" /> {s.status === "crashed" ? "Try again now" : "Restart"}</Button>
    {:else}
      <Button disabled={busy || !store.overview?.cloudflared} onclick={() => run(() => api.start(tunnel.id))}>
        <Play class="size-2.5 fill-current" /> {s.status === "crashed" ? "Try again" : "Start"}
      </Button>
    {/if}
    {#if tunnel.kind === "config"}
      <Button variant="outline" disabled={busy} onclick={validate}><Check class="size-3.5" /> Validate</Button>
    {/if}
  </div>
</header>

{#if s.detail || s.hint}
  {#snippet stateBody()}
    {#if s.detail}<div class="whitespace-pre-wrap select-text">{s.detail}</div>{/if}
    {#if s.hint}<div class="mt-1.5 whitespace-pre-wrap text-muted-foreground select-text">{s.hint}</div>{/if}
  {/snippet}
  {@render notice(s.status, stateBody, "status")}
{/if}
{#if error}
  {#snippet errorBody()}<div class="whitespace-pre-wrap select-text">{error}</div>{/snippet}
  {@render notice("crashed", errorBody, "alert")}
{/if}
{#if validation}
  {#snippet validationBody()}
    {#if validation?.ok}
      <div>The config is valid.</div>
    {:else}
      <div class="whitespace-pre-wrap select-text">{validation?.raw}</div>
      {#if validation?.hint}<div class="mt-1.5 whitespace-pre-wrap text-muted-foreground">{validation.hint}</div>{/if}
    {/if}
  {/snippet}
  {@render notice(validation.ok ? "connected" : "crashed", validationBody)}
{/if}

<Routes {tunnel} />

<SettingsGroup title="Behaviour">
  <SettingsRow
    label="Inspect requests"
    help={tunnel.kind === "token"
      ? "Not available for token tunnels: their routes live in the Cloudflare dashboard."
      : `Routes traffic through Underlane's local proxy and stores request and response bodies (up to ${store.overview?.settings.max_body_kb ?? 1024} KB each). Bodies may contain secrets.`}
  >
    {#snippet control({ id, describedBy })}
      <Switch {id} aria-describedby={describedBy} disabled={tunnel.kind === "token" || busy} bind:checked={() => tunnel.inspect, (v) => patch({ inspect: v })} />
    {/snippet}
  </SettingsRow>
  <SettingsRow label="Start automatically" help="Start this tunnel whenever Underlane launches.">
    {#snippet control({ id, describedBy })}
      <Switch {id} aria-describedby={describedBy} disabled={busy} bind:checked={() => tunnel.autostart, (v) => patch({ autostart: v })} />
    {/snippet}
  </SettingsRow>
  <SettingsRow label="Log level" help="Debug logs every request's headers (redacted before storage).">
    {#snippet control({ id, describedBy })}
      <Select.Root type="single" value={tunnel.loglevel} onValueChange={(loglevel) => patch({ loglevel })}>
        <Select.Trigger {id} aria-describedby={describedBy} class="w-55">{tunnel.loglevel}</Select.Trigger>
        <Select.Content>
          {#each ["debug", "info", "warn", "error"] as l (l)}<Select.Item value={l} label={l} />{/each}
        </Select.Content>
      </Select.Root>
    {/snippet}
  </SettingsRow>
  <SettingsRow label="Project">
    {#snippet control({ id, labelId })}
      <!-- Named by the row label plus its own text, so it reads "Project, <name>". -->
      <Combobox
        {id}
        aria-labelledby="{labelId} {id}"
        value={tunnel.project_name}
        items={(store.overview?.projects ?? []).map((p) => p.name)}
        placeholder="Move to project…"
        onpick={(project) => patch({ project })}
      />
    {/snippet}
  </SettingsRow>
</SettingsGroup>

{#snippet sourceNote()}
  {#if tunnel.kind === "config"}
    Underlane reads this file and never changes it. Edits are picked up the next time the tunnel starts.
  {:else}
    Quick tunnels get a new random hostname every time they start, and don't support server-sent events.
  {/if}
{/snippet}

<SettingsGroup title="Source" footer={tunnel.kind === "token" ? undefined : sourceNote}>
  <SettingsRow label="Type"><span class="text-muted-foreground">{kindText[tunnel.kind]}</span></SettingsRow>
  {#if tunnel.kind === "config" && tunnel.config_path}
    <SettingsRow label="Config file">
      <!-- Truncated, so the native title reveals the whole path (02 §2 Tip exception). 55%: leaves
           the label and the Show in Finder button their room, a one-off. -->
      <span class="max-w-[55%] truncate font-mono text-callout text-muted-foreground select-text" title={tunnel.config_path}>{tunnel.config_path}</span>
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
        <Button {...props} variant="ghost" disabled={busy} class="text-destructive hover:text-destructive aria-expanded:text-destructive"><Trash2 class="size-3.5" /> Remove</Button>
      {/snippet}
    </AlertDialog.Trigger>
    <!-- Escape/Cancel return focus to Remove explicitly (02 R7); after a removal Tunnels focuses the next tunnel. -->
    <AlertDialog.Content onCloseAutoFocus={(e) => (removing ? e.preventDefault() : backToRemove(e))}>
      <AlertDialog.Header>
        <AlertDialog.Title>Remove “{tunnel.name}”?</AlertDialog.Title>
        {#if tunnel.kind === "config"}
          <AlertDialog.Description>The config file stays on disk.</AlertDialog.Description>
        {:else if tunnel.kind === "token"}
          <AlertDialog.Description>Its token is deleted from the keychain.</AlertDialog.Description>
        {/if}
      </AlertDialog.Header>
      <AlertDialog.Footer>
        <AlertDialog.Cancel>Cancel</AlertDialog.Cancel>
        <AlertDialog.Action variant="destructive" onclick={remove}>Remove</AlertDialog.Action>
      </AlertDialog.Footer>
    </AlertDialog.Content>
  </AlertDialog.Root>
</div>
