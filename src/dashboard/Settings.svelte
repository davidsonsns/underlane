<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import Check from "@lucide/svelte/icons/check";
  import FolderOpen from "@lucide/svelte/icons/folder-open";
  import { store } from "../lib/store.svelte";
  import { api } from "../lib/api";
  import { notify } from "$lib/toast";
  import { motion } from "../lib/theme";
  import type { LoginItem, Settings } from "../lib/types";
  import Toolbar from "$lib/components/app/Toolbar.svelte";
  import StatePanel from "$lib/components/app/StatePanel.svelte";
  import SettingsGroup from "$lib/components/app/SettingsGroup.svelte";
  import SettingsRow from "$lib/components/app/SettingsRow.svelte";
  import Segmented from "$lib/components/app/Segmented.svelte";
  import CopyButton from "$lib/components/app/CopyButton.svelte";
  import IconAction from "$lib/components/app/IconAction.svelte";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import { Switch } from "$lib/components/ui/switch/index.js";
  import * as InputGroup from "$lib/components/ui/input-group/index.js";
  import * as Select from "$lib/components/ui/select/index.js";

  let draft = $state<Settings | null>(null);
  let login = $state<LoginItem | null>(null);
  let detecting = $state(false);
  let redactText = $state("");
  let redactQueryText = $state("");

  // data-field → the draft key it edits; a focused field keeps its typed value (see below).
  const KEEP: Record<string, keyof Settings> = { path: "cloudflared_path", ttl: "log_ttl_days", "max-db": "max_db_mb", "max-body": "max_body_kb" };

  // Follow changes made elsewhere (another window, the tray) as well as the initial load.
  $effect(() => {
    const s = store.overview?.settings;
    if (!s) return;
    const snap = structuredClone($state.snapshot(s));
    untrack(() => {
      // Don't clobber a field the user is typing in.
      const focused = document.activeElement?.getAttribute("data-field");
      const key = focused && KEEP[focused];
      if (draft && key) (snap as Record<string, unknown>)[key] = draft[key];
      draft = snap;
      if (focused !== "redact") redactText = snap.redact_headers.join(", ");
      if (focused !== "redact-query") redactQueryText = (snap.redact_query ?? []).join(", ");
    });
  });

  onMount(async () => {
    login = await api.loginItem();
  });

  async function save(patch: Partial<Settings>) {
    if (!draft) return;
    Object.assign(draft, patch);
    try {
      await api.saveSettings($state.snapshot(draft));
    } catch (e) {
      notify.error(e, "Couldn't save settings");
    }
  }

  // Saving the theme makes Rust flip the NSWindow appearance on the main thread, which freezes
  // the webview's animations (see commands.rs), so let the segment thumb finish sliding first.
  let themeTimer: ReturnType<typeof setTimeout> | undefined;
  function setTheme(theme: Settings["theme"]) {
    clearTimeout(themeTimer);
    themeTimer = setTimeout(() => save({ theme }), motion(220));
  }

  // Detection usually lands in a few ms, so the button keeps its label and look (no
  // "Detecting…" swap or disabled dim that would resize the row and flash).
  async function detect() {
    if (detecting) return;
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
    if (login) login = { ...login, enabled: v };
    try {
      login = await api.setLaunchAtLogin(v);
    } catch (e) {
      login = before;
      notify.error(e, "Couldn't change Launch at login");
    }
  }

  const num = (e: Event) => Number((e.target as HTMLInputElement).value);
  const list = (s: string) => s.split(",").map((x) => x.trim()).filter(Boolean);
  const themes = [
    { value: "system", label: "System" },
    { value: "light", label: "Light" },
    { value: "dark", label: "Dark" },
  ] as const;
</script>

{#snippet path(text: string | undefined)}
  <!-- Truncated, so the native title reveals the whole path (02 §2 Tip exception). -->
  <span class="max-w-70 truncate font-mono text-callout text-muted-foreground select-text" title={text}>{text}</span>
{/snippet}

<div class="flex h-full flex-col">
  <Toolbar title="Settings" />
  {#if !draft}
    <StatePanel kind="loading" title="Loading settings…" />
  {:else}
    <!-- Full-width scroller (scrollbar at the edge), content capped at 760px and centred. -->
    <div class="overflow-y-auto px-[max(24px,calc((100%-760px)/2))] pt-2 pb-8">
      <SettingsGroup title="cloudflared">
        <SettingsRow label="Status">
          {#if store.overview?.cloudflared}
            <span class="inline-flex items-center gap-1 text-ok-text"><Check class="size-3.5" /> Version {store.overview.cloudflared.version ?? "unknown"}</span>
            {@render path(store.overview.cloudflared.path)}
          {:else}
            <span class="text-bad-text">Not found</span>
          {/if}
        </SettingsRow>
        {#if !store.overview?.cloudflared}
          <SettingsRow label="Install it with Homebrew, then choose Detect.">
            <code class="rounded-sm bg-(--code-bg) px-2 py-0.5 font-mono text-callout select-text">brew install cloudflared</code>
            <CopyButton value="brew install cloudflared" what="command" />
          </SettingsRow>
        {/if}
        <SettingsRow label="Path" help="Leave empty to look on PATH and in Homebrew's folders.">
          {#snippet control({ id, describedBy })}
            <Input {id} aria-describedby={describedBy} data-field="path" class="w-65 font-mono text-callout" placeholder="/opt/homebrew/bin/cloudflared" bind:value={draft!.cloudflared_path} />
          {/snippet}
          <Button variant="outline" aria-busy={detecting || undefined} onclick={detect}>Detect</Button>
        </SettingsRow>
      </SettingsGroup>

      <SettingsGroup title="General">
        <SettingsRow label="Launch at login" help={login?.message ?? "Opens Burrow in the menu bar when you log in. Tunnels marked “Start automatically” come up with it."}>
          {#snippet control({ id, describedBy })}
            <Switch {id} aria-describedby={describedBy} disabled={!login || login.status === "unsupported"} bind:checked={() => login?.enabled ?? false, setLogin} />
          {/snippet}
        </SettingsRow>
        {#if store.overview?.last_launch}
          <SettingsRow label="This launch">
            <span class="text-muted-foreground">{store.overview.last_launch.reason === "login_item" ? "Opened by macOS at login" : "Opened manually"} at {new Date(store.overview.last_launch.at).toLocaleTimeString()}</span>
          </SettingsRow>
        {/if}
        <SettingsRow label="Appearance">
          {#snippet control({ id, labelId, describedBy })}
            <Segmented {id} label="Appearance" aria-labelledby={labelId} aria-describedby={describedBy} items={themes} bind:value={draft!.theme} onchange={setTheme} />
          {/snippet}
        </SettingsRow>
        <SettingsRow label="Default log level" help="Used for newly added tunnels.">
          {#snippet control({ id, describedBy })}
            <Select.Root type="single" value={draft!.default_loglevel} onValueChange={(default_loglevel) => save({ default_loglevel })}>
              <Select.Trigger {id} aria-describedby={describedBy} class="w-55">{draft!.default_loglevel}</Select.Trigger>
              <Select.Content>
                {#each ["debug", "info", "warn", "error"] as l (l)}<Select.Item value={l} label={l} />{/each}
              </Select.Content>
            </Select.Root>
          {/snippet}
        </SettingsRow>
      </SettingsGroup>

      <SettingsGroup title="Storage">
        <SettingsRow label="Keep logs and requests for" help="Older entries are deleted automatically.">
          {#snippet control({ id, describedBy })}
            <InputGroup.Root class="w-28">
              <InputGroup.Input {id} aria-describedby={describedBy} data-field="ttl" type="number" min="1" max="365" class="text-right" bind:value={draft!.log_ttl_days} onchange={(e) => save({ log_ttl_days: num(e) })} />
              <InputGroup.Addon align="inline-end">days</InputGroup.Addon>
            </InputGroup.Root>
          {/snippet}
        </SettingsRow>
        <SettingsRow label="Maximum database size" help="The oldest entries go first when this is reached.">
          {#snippet control({ id, describedBy })}
            <InputGroup.Root class="w-28">
              <InputGroup.Input {id} aria-describedby={describedBy} data-field="max-db" type="number" min="16" step="16" class="text-right" bind:value={draft!.max_db_mb} onchange={(e) => save({ max_db_mb: num(e) })} />
              <InputGroup.Addon align="inline-end">MB</InputGroup.Addon>
            </InputGroup.Root>
          {/snippet}
        </SettingsRow>
        <SettingsRow label="Data folder">
          {@render path(store.overview?.data_dir)}
          <IconAction label="Show in Finder" onclick={() => store.overview && revealItemInDir(store.overview.data_dir + "/burrow.db").catch((e) => notify.error(e, "Couldn't show the data folder"))}><FolderOpen /></IconAction>
        </SettingsRow>
      </SettingsGroup>

      <SettingsGroup title="Request inspection">
        <SettingsRow label="Body capture limit" help="Bodies larger than this are stored truncated. 0 keeps headers only.">
          {#snippet control({ id, describedBy })}
            <InputGroup.Root class="w-28">
              <InputGroup.Input {id} aria-describedby={describedBy} data-field="max-body" type="number" min="0" max="16384" class="text-right" bind:value={draft!.max_body_kb} onchange={(e) => save({ max_body_kb: num(e) })} />
              <InputGroup.Addon align="inline-end">KB</InputGroup.Addon>
            </InputGroup.Root>
          {/snippet}
        </SettingsRow>
        <SettingsRow column label="Redact these headers too" help="Authorization, Cookie, Set-Cookie, X-Api-Key and other credential headers are always redacted, as are token-like query parameters. Bodies are stored as sent.">
          {#snippet control({ id, describedBy })}
            <Input {id} aria-describedby={describedBy} data-field="redact" class="font-mono text-callout" placeholder="X-Internal-Key, X-Session" bind:value={redactText} onchange={() => save({ redact_headers: list(redactText) })} />
          {/snippet}
        </SettingsRow>
        <SettingsRow column label="Redact these query parameters too" help="token, access_token, api_key, key, code, sig, signature, password and secret are always redacted in captured URLs and logs.">
          {#snippet control({ id, describedBy })}
            <Input {id} aria-describedby={describedBy} data-field="redact-query" class="font-mono text-callout" placeholder="session, auth" bind:value={redactQueryText} onchange={() => save({ redact_query: list(redactQueryText) })} />
          {/snippet}
        </SettingsRow>
        {#if (store.overview?.dropped_captures ?? 0) > 0}
          <SettingsRow label="Dropped under load">
            <span class="text-muted-foreground">{store.overview?.dropped_captures} log lines or requests weren't stored because the writer fell behind.</span>
          </SettingsRow>
        {/if}
      </SettingsGroup>
    </div>
  {/if}
</div>
