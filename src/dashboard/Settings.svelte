<script lang="ts">
  import Check from "@lucide/svelte/icons/check";
  import FolderOpen from "@lucide/svelte/icons/folder-open";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import { onMount, untrack } from "svelte";
  import CopyButton from "$lib/components/app/CopyButton.svelte";
  import IconAction from "$lib/components/app/IconAction.svelte";
  import SettingsGroup from "$lib/components/app/SettingsGroup.svelte";
  import SettingsRow from "$lib/components/app/SettingsRow.svelte";
  import StatePanel from "$lib/components/app/StatePanel.svelte";
  import Toolbar from "$lib/components/app/Toolbar.svelte";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import {
    Addon as InputGroupAddon,
    Input as InputGroupInput,
    Root as InputGroupRoot,
  } from "$lib/components/ui/input-group/index.js";
  import {
    Content as SelectContent,
    Item as SelectItem,
    Root as SelectRoot,
    Trigger as SelectTrigger,
  } from "$lib/components/ui/select/index.js";
  import { Switch } from "$lib/components/ui/switch/index.js";
  import { notify } from "$lib/toast";
  import { api } from "../lib/api";
  import { store } from "../lib/store.svelte";
  import type { LoginItem, Settings } from "../lib/types";

  let draft = $state<Settings | null>(null);
  let login = $state<LoginItem | null>(null);
  let detecting = $state(false);
  let redactText = $state("");
  let redactQueryText = $state("");

  // data-field → the draft key it edits; a focused field keeps its typed value (see below).
  const KEEP: Record<string, keyof Settings> = {
    "max-body": "max_body_kb",
    "max-db": "max_db_mb",
    path: "cloudflared_path",
    ttl: "log_ttl_days",
  };

  // Follow changes made elsewhere (another window, the tray) as well as the initial load.
  $effect(() => {
    const s = store.overview?.settings;
    if (!s) {
      return;
    }
    const snap = structuredClone($state.snapshot(s));
    untrack(function reconcileDraft() {
      // Don't clobber a field the user is typing in.
      const focused = document.activeElement?.getAttribute("data-field");
      const key = focused && KEEP[focused];
      if (draft && key) {
        (snap as Record<string, unknown>)[key] = draft[key];
      }
      draft = snap;
      if (focused !== "redact") {
        redactText = snap.redact_headers.join(", ");
      }
      if (focused !== "redact-query") {
        redactQueryText = (snap.redact_query ?? []).join(", ");
      }
    });
  });

  onMount(async () => {
    login = await api.loginItem();
  });

  async function save(patch: Partial<Settings>) {
    if (!draft) {
      return;
    }
    Object.assign(draft, patch);
    try {
      await api.saveSettings($state.snapshot(draft));
    } catch (e) {
      notify.error(e, "Couldn't save settings");
    }
  }

  function setTheme(theme: string) {
    if (theme === "system" || theme === "light" || theme === "dark") {
      save({ theme });
    }
  }

  // Detection usually lands in a few ms, so the button keeps its label and look (no
  // "Detecting…" swap or disabled dim that would resize the row and flash).
  async function detect() {
    if (detecting) {
      return;
    }
    detecting = true;
    try {
      await save({ cloudflared_path: draft?.cloudflared_path?.trim() || null });
      await api.detectCloudflared();
      await store.refresh();
    } catch (e) {
      notify.error(e, "Couldn't detect cloudflared");
    } finally {
      detecting = false;
    }
  }

  // Optimistic, then whatever macOS reports (it can refuse or ask for approval).
  async function setLogin(v: boolean) {
    const before = login;
    if (login) {
      login = { ...login, enabled: v };
    }
    try {
      login = await api.setLaunchAtLogin(v);
    } catch (e) {
      login = before;
      notify.error(e, "Couldn't change Launch at login");
    }
  }

  const num = (e: Event) => Number((e.target as HTMLInputElement).value);
  const list = (s: string) =>
    s
      .split(",")
      .map((x) => x.trim())
      .filter(Boolean);
  const themes = [
    { label: "System", value: "system" },
    { label: "Light", value: "light" },
    { label: "Dark", value: "dark" },
  ] as const;
</script>

{#snippet inspectionSettings(
  currentDraft: Settings
)}
  <SettingsGroup title="Request inspection">
    <SettingsRow
      help="Bodies larger than this are stored truncated. 0 keeps headers only."
      label="Body capture limit"
    >
      {#snippet control({
        id,
        describedBy,
      })}
        <InputGroupRoot class="w-28">
          <InputGroupInput
            aria-describedby={describedBy}
            class="text-right"
            data-field="max-body"
            {id}
            max="16384"
            min="0"
            onchange={(e) => save({ max_body_kb: num(e) })}
            type="number"
            bind:value={currentDraft.max_body_kb}
          />
          <InputGroupAddon align="inline-end">KB</InputGroupAddon>
        </InputGroupRoot>
      {/snippet}
    </SettingsRow>
    <SettingsRow
      column
      help="Authorization, Cookie, Set-Cookie, X-Api-Key and other credential headers are always redacted, as are token-like query parameters. Bodies are stored as sent."
      label="Redact these headers too"
    >
      {#snippet control({
        id,
        describedBy,
      })}
        <Input
          aria-describedby={describedBy}
          class="font-mono text-callout"
          data-field="redact"
          {id}
          onchange={() => save({ redact_headers: list(redactText) })}
          placeholder="X-Internal-Key, X-Session"
          bind:value={redactText}
        />
      {/snippet}
    </SettingsRow>
    <SettingsRow
      column
      help="token, access_token, api_key, key, code, sig, signature, password and secret are always redacted in captured URLs and logs."
      label="Redact these query parameters too"
    >
      {#snippet control({
        id,
        describedBy,
      })}
        <Input
          aria-describedby={describedBy}
          class="font-mono text-callout"
          data-field="redact-query"
          {id}
          onchange={() => save({ redact_query: list(redactQueryText) })}
          placeholder="session, auth"
          bind:value={redactQueryText}
        />
      {/snippet}
    </SettingsRow>
    {#if (store.overview?.dropped_captures ?? 0) > 0}
      <SettingsRow label="Dropped under load">
        <span class="text-muted-foreground"
          >{store.overview?.dropped_captures}
          log lines or requests weren't stored because the writer fell
          behind.</span
        >
      </SettingsRow>
    {/if}
  </SettingsGroup>
{/snippet}

{#snippet storageSettings(
  currentDraft: Settings
)}
  <SettingsGroup title="Storage">
    <SettingsRow
      help="Older entries are deleted automatically."
      label="Keep logs and requests for"
    >
      {#snippet control({
        id,
        describedBy,
      })}
        <InputGroupRoot class="w-28">
          <InputGroupInput
            aria-describedby={describedBy}
            class="text-right"
            data-field="ttl"
            {id}
            max="365"
            min="1"
            onchange={(e) => save({ log_ttl_days: num(e) })}
            type="number"
            bind:value={currentDraft.log_ttl_days}
          />
          <InputGroupAddon align="inline-end">days</InputGroupAddon>
        </InputGroupRoot>
      {/snippet}
    </SettingsRow>
    <SettingsRow
      help="The oldest entries go first when this is reached."
      label="Maximum database size"
    >
      {#snippet control({
        id,
        describedBy,
      })}
        <InputGroupRoot class="w-28">
          <InputGroupInput
            aria-describedby={describedBy}
            class="text-right"
            data-field="max-db"
            {id}
            min="16"
            onchange={(e) => save({ max_db_mb: num(e) })}
            step="16"
            type="number"
            bind:value={currentDraft.max_db_mb}
          />
          <InputGroupAddon align="inline-end">MB</InputGroupAddon>
        </InputGroupRoot>
      {/snippet}
    </SettingsRow>
    <SettingsRow label="Data folder">
      {@render path(store.overview?.data_dir)}
      <IconAction
        label="Show in Finder"
        onclick={() =>
          store.overview &&
          revealItemInDir(`${store.overview.data_dir}/underlane.db`).catch(
            (e) => notify.error(e, "Couldn't show the data folder")
          )}
        ><FolderOpen /></IconAction
      >
    </SettingsRow>
  </SettingsGroup>
{/snippet}

{#snippet generalSettings(
  currentDraft: Settings
)}
  <SettingsGroup title="General">
    <SettingsRow
      help={login?.message ??
        "Opens Underlane in the menu bar when you log in. Tunnels marked “Start automatically” come up with it."}
      label="Launch at login"
    >
      {#snippet control({
        id,
        describedBy,
      })}
        <Switch
          aria-describedby={describedBy}
          disabled={!login || login.status === "unsupported"}
          {id}
          bind:checked={() => login?.enabled ?? false, setLogin}
        />
      {/snippet}
    </SettingsRow>
    {#if store.overview?.last_launch}
      <SettingsRow label="This launch">
        <span class="text-muted-foreground"
          >{store.overview.last_launch.reason === "login_item"
            ? "Opened by macOS at login"
            : "Opened manually"}
          at
          {new Date(store.overview.last_launch.at).toLocaleTimeString()}</span
        >
      </SettingsRow>
    {/if}
    <SettingsRow
      help="Choose how Underlane looks, or follow your system appearance."
      label="Interface theme"
    >
      {#snippet control({
        id,
        labelId,
        describedBy,
      })}
        <SelectRoot
          onValueChange={setTheme}
          type="single"
          value={currentDraft.theme}
        >
          <SelectTrigger
            aria-describedby={describedBy}
            aria-labelledby={labelId}
            class="w-40"
            {id}
            >{themes.find((t) => t.value === currentDraft.theme)
              ?.label}</SelectTrigger
          >
          <SelectContent>
            {#each themes as theme (theme.value)}
              <SelectItem label={theme.label} value={theme.value} />
            {/each}
          </SelectContent>
        </SelectRoot>
      {/snippet}
    </SettingsRow>
    <SettingsRow help="Used for newly added tunnels." label="Default log level">
      {#snippet control({
        id,
        describedBy,
      })}
        <SelectRoot
          onValueChange={(default_loglevel) => save({ default_loglevel })}
          type="single"
          value={currentDraft.default_loglevel}
        >
          <SelectTrigger aria-describedby={describedBy} class="w-40" {id}
            >{currentDraft.default_loglevel}</SelectTrigger
          >
          <SelectContent>
            {#each ["debug", "info", "warn", "error"] as l (l)}
              <SelectItem label={l} value={l} />
            {/each}
          </SelectContent>
        </SelectRoot>
      {/snippet}
    </SettingsRow>
  </SettingsGroup>
{/snippet}

{#snippet path(
  text: string | undefined
)}
  <!-- Truncated, so the native title reveals the whole path (02 §2 Tip exception). -->
  <span
    class="max-w-70 truncate font-mono text-callout text-muted-foreground select-text"
    title={text}
    >{text}</span
  >
{/snippet}

<div class="flex h-full flex-col">
  <Toolbar title="Settings" />
  {#if !draft}
    <StatePanel kind="loading" title="Loading settings…" />
  {:else}
    {@const currentDraft = draft}
    <!-- Full-width scroller (scrollbar at the edge), content capped at 720px and centred. -->
    <div class="overflow-y-auto px-[max(24px,calc((100%-720px)/2))] pt-1 pb-10">
      <SettingsGroup title="cloudflared">
        <SettingsRow label="Status">
          {#if store.overview?.cloudflared}
            <span class="inline-flex items-center gap-1 text-ok-text"
              ><Check class="size-3.5" />
              Version {store.overview.cloudflared.version ?? "unknown"}</span
            >
            {@render path(store.overview.cloudflared.path)}
          {:else}
            <span class="text-bad-text">Not found</span>
          {/if}
        </SettingsRow>
        {#if !store.overview?.cloudflared}
          <SettingsRow label="Install it with Homebrew, then choose Detect.">
            <code
              class="rounded-sm bg-(--code-bg) px-2 py-0.5 font-mono text-callout select-text"
              >brew install cloudflared</code
            >
            <CopyButton value="brew install cloudflared" what="command" />
          </SettingsRow>
        {/if}
        <SettingsRow
          help="Leave empty to look on PATH and in Homebrew's folders."
          label="Path"
        >
          {#snippet control({
            id,
            describedBy,
          })}
            <Input
              aria-describedby={describedBy}
              class="w-65 font-mono text-callout"
              data-field="path"
              {id}
              placeholder="/opt/homebrew/bin/cloudflared"
              bind:value={currentDraft.cloudflared_path}
            />
          {/snippet}
          <Button
            aria-busy={detecting || undefined}
            onclick={detect}
            variant="outline"
            >Detect</Button
          >
        </SettingsRow>
      </SettingsGroup>

      {@render generalSettings(currentDraft)}

      {@render storageSettings(currentDraft)}

      {@render inspectionSettings(currentDraft)}
    </div>
  {/if}
</div>
