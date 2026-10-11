<script lang="ts">
  // Every ui/* and app/* component in each state, in the current theme (02 Phase 1 step 8).
  // Started from the theming spike's Demo.svelte (6c0e62d). Query params:
  //   ?theme=light|dark|system   ?open=sheet|alert|combobox   ?section=<id> (scrolls to it)
  // Sections carry data-section so Playwright can screenshot them one by one.
  import { onMount, tick } from "svelte";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import { Textarea } from "$lib/components/ui/textarea/index.js";
  import { Label } from "$lib/components/ui/label/index.js";
  import { Switch } from "$lib/components/ui/switch/index.js";
  import { Checkbox } from "$lib/components/ui/checkbox/index.js";
  import { Separator } from "$lib/components/ui/separator/index.js";
  import { Badge } from "$lib/components/ui/badge/index.js";
  import { Spinner } from "$lib/components/ui/spinner/index.js";
  import { Kbd, KbdGroup } from "$lib/components/ui/kbd/index.js";
  import { Toggle } from "$lib/components/ui/toggle/index.js";
  import { Toaster } from "$lib/components/ui/sonner/index.js";
  import * as Tabs from "$lib/components/ui/tabs/index.js";
  import * as ToggleGroup from "$lib/components/ui/toggle-group/index.js";
  import * as Tooltip from "$lib/components/ui/tooltip/index.js";
  import * as Dialog from "$lib/components/ui/dialog/index.js";
  import * as AlertDialog from "$lib/components/ui/alert-dialog/index.js";
  import * as InputGroup from "$lib/components/ui/input-group/index.js";
  import * as Alert from "$lib/components/ui/alert/index.js";
  import * as Empty from "$lib/components/ui/empty/index.js";
  import Tip from "$lib/components/app/Tip.svelte";
  import IconAction from "$lib/components/app/IconAction.svelte";
  import CopyButton from "$lib/components/app/CopyButton.svelte";
  import Segmented from "$lib/components/app/Segmented.svelte";
  import SettingsGroup from "$lib/components/app/SettingsGroup.svelte";
  import SettingsRow from "$lib/components/app/SettingsRow.svelte";
  import ArchGlyph from "$lib/components/app/ArchGlyph.svelte";
  import StatePanel from "$lib/components/app/StatePanel.svelte";
  import StatusDot from "$lib/components/app/StatusDot.svelte";
  import RouteLine from "$lib/components/app/RouteLine.svelte";
  import Toolbar from "$lib/components/app/Toolbar.svelte";
  import { notify } from "$lib/toast";
  import { returnFocusTo } from "$lib/focus";
  import Combobox from "$lib/components/app/Combobox.svelte";
  import { applyTheme } from "$lib/theme";
  import type { Status } from "$lib/types";
  import VirtuaLab from "./VirtuaLab.svelte";
  import RequestsLab from "./RequestsLab.svelte";
  import Play from "@lucide/svelte/icons/play";
  import Square from "@lucide/svelte/icons/square";
  import RotateCw from "@lucide/svelte/icons/rotate-cw";
  import ExternalLink from "@lucide/svelte/icons/external-link";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import Plus from "@lucide/svelte/icons/plus";
  import Search from "@lucide/svelte/icons/search";
  import FolderOpen from "@lucide/svelte/icons/folder";
  import Zap from "@lucide/svelte/icons/zap";
  import FileText from "@lucide/svelte/icons/file-text";
  import KeyRound from "@lucide/svelte/icons/key-round";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import WrapText from "@lucide/svelte/icons/wrap-text";
  import X from "@lucide/svelte/icons/x";

  type Theme = "system" | "light" | "dark";
  const params = new URLSearchParams(location.search);
  const p = params.get("theme");
  let theme = $state<Theme>(p === "light" || p === "dark" ? p : "system");
  function setTheme(t: Theme) {
    applyTheme(t);
    const u = new URL(location.href);
    u.searchParams.set("theme", t);
    history.replaceState(null, "", u);
  }

  // Overlays, openable from the URL for screenshots.
  let sheetOpen = $state(params.get("open") === "sheet");
  let alertOpen = $state(params.get("open") === "alert");
  let comboOpen = $state(params.get("open") === "combobox");
  let sheetTrigger = $state<HTMLElement | null>(null);
  let alertTrigger = $state<HTMLElement | null>(null);

  // Controls
  let appearance = $state<Theme>("system");
  let bodyView = $state<"pretty" | "raw" | "hex">("raw");
  let levels = $state<string[]>(["info", "warn", "error"]);
  let wrap = $state(true);
  let side = $state("resp");
  let addTab = $state("quick");
  let inspect = $state(true);
  let autostart = $state(false);
  let launch = $state(true);
  let agree = $state(true);
  let partial = $state(false);
  let loglevel = $state("info");
  let pulse = $state(0);
  let segmentChanges = $state(0);

  // Combobox ("Move to project…", 02 F6)
  let projects = $state(["E2E", "Work", "Side projects"]);
  let project = $state("Work");
  function pick(name: string) {
    if (!projects.includes(name)) projects.push(name);
    project = name;
  }

  const statuses: Status[] = ["connected", "starting", "degraded", "crashed", "stopped"];
  const longError = "cloudflared exited with status 1: failed to connect to the edge (dial tcp 198.41.192.7:7844: i/o timeout)";

  onMount(async () => {
    const s = params.get("section");
    if (s) {
      await tick();
      document.querySelector(`[data-section="${s}"]`)?.scrollIntoView({ block: "start" });
    }
  });
</script>

{#snippet section(id: string, title: string, note: string)}
  <div class="mb-2 flex items-baseline gap-3">
    <h2 class="text-sm font-semibold">{title}</h2>
    <span class="text-xs text-muted-foreground">{note}</span>
  </div>
{/snippet}

<Tooltip.Provider delayDuration={250} ignoreNonKeyboardFocus>
  <div class="flex h-screen flex-col bg-background text-foreground">
    <Toolbar title="UI lab" subtitle="Phase 1: ui/* and app/* in every state">
      <Segmented label="Theme" bind:value={theme} onchange={setTheme} items={[{ value: "system", label: "System" }, { value: "light", label: "Light" }, { value: "dark", label: "Dark" }]} />
    </Toolbar>

    <div class="flex-1 overflow-y-auto px-6 pt-2 pb-16" data-testid="scroller">
      <div class="mx-auto flex max-w-[860px] flex-col gap-8">
        <!-- Buttons ------------------------------------------------------------------ -->
        <section data-section="buttons" class="pt-4">
          {@render section("buttons", "Buttons", "22px push buttons (lg 24px), no hover; ghost is the toolbar/borderless button")}
          <div class="flex flex-wrap items-center gap-2">
            <Button data-testid="btn-default">Default</Button>
            <Button variant="outline" data-testid="btn-outline">Outline</Button>
            <Button variant="secondary">Secondary</Button>
            <Button variant="ghost">Ghost</Button>
            <Button variant="destructive">Destructive</Button>
            <Button variant="link">Link</Button>
            <Button disabled>Disabled</Button>
            <Button variant="outline" disabled>Disabled</Button>
          </div>
          <div class="mt-3 flex flex-wrap items-center gap-2">
            <Button size="lg">Large 24px</Button>
            <Button variant="outline" size="lg">Large</Button>
            <Button variant="outline" size="sm">Small</Button>
            <Button variant="outline" size="xs">XS</Button>
            <Button variant="outline"><Play class="fill-current" />Start</Button>
            <Button variant="outline"><Square class="fill-current" />Stop</Button>
            <Button variant="outline"><RotateCw />Restart</Button>
            <Button><Plus />Add tunnel</Button>
            <Button variant="outline" disabled><Spinner class="text-current" />Detecting…</Button>
          </div>
          <div class="mt-3 flex flex-wrap items-center gap-1" data-testid="icon-actions">
            <IconAction label="Open in browser" onclick={() => {}}><ExternalLink /></IconAction>
            <IconAction label="Restart" shortcut="⌘R" onclick={() => {}} data-testid="tip-trigger"><RotateCw /></IconAction>
            <IconAction label="Show in Finder" onclick={() => {}}><FolderOpen /></IconAction>
            <IconAction label="Remove" onclick={() => {}}><Trash2 /></IconAction>
            <CopyButton value="https://synthetic-quick-12.trycloudflare.com" what="public URL" />
            <IconAction label="Disabled" disabled><X /></IconAction>
            <Separator orientation="vertical" class="mx-2 h-4" />
            <CopyButton value="curl -X GET https://example.com/e2e" what="curl command" label="Copy as curl" />
            <Button variant="outline" size="icon" aria-label="Plain icon button"><Plus /></Button>
            <Button variant="outline" size="icon-sm" aria-label="Small icon button"><Plus /></Button>
          </div>
        </section>

        <!-- Fields ------------------------------------------------------------------- -->
        <section data-section="fields">
          {@render section("fields", "Fields", "22px NSTextField; the ring shows on any focus; placeholder is --text-placeholder (AA)")}
          <div class="grid grid-cols-[140px_1fr] items-center gap-x-3 gap-y-2.5">
            <Label for="f1" class="justify-self-end">Local address:</Label>
            <Input id="f1" class="w-64 font-mono" placeholder="http://localhost:3000" data-testid="input" />
            <Label for="f2" class="justify-self-end">With value:</Label>
            <Input id="f2" class="w-64" value="web" />
            <Label for="f3" class="justify-self-end">Disabled:</Label>
            <Input id="f3" class="w-64" disabled placeholder="Disabled" />
            <Label for="f4" class="justify-self-end">Invalid:</Label>
            <Input id="f4" class="w-64" aria-invalid="true" value="not a url" />
            <Label class="justify-self-end">Search:</Label>
            <InputGroup.Root class="w-64">
              <InputGroup.Addon><Search /></InputGroup.Addon>
              <InputGroup.Input placeholder="Filter path or method" data-testid="search" />
            </InputGroup.Root>
            <Label class="justify-self-end">Number + unit:</Label>
            <InputGroup.Root class="w-28">
              <InputGroup.Input type="number" value="1024" class="text-right" />
              <InputGroup.Addon align="inline-end">KB</InputGroup.Addon>
            </InputGroup.Root>
            <Label class="justify-self-end">With button:</Label>
            <InputGroup.Root class="w-64">
              <InputGroup.Input placeholder="Token" class="font-mono" />
              <InputGroup.Addon align="inline-end"><InputGroup.Button size="icon-xs" aria-label="Clear"><X /></InputGroup.Button></InputGroup.Addon>
            </InputGroup.Root>
            <Label for="sel" class="justify-self-end">Native select:</Label>
            <select id="sel" bind:value={loglevel} class="justify-self-start" data-testid="select">
              {#each ["debug", "info", "warn", "error"] as l (l)}<option value={l}>{l}</option>{/each}
            </select>
            <Label for="ta" class="self-start justify-self-end pt-1">Textarea:</Label>
            <Textarea id="ta" class="w-96 font-mono" placeholder="Paste a tunnel token (eyJh…)" />
          </div>
        </section>

        <!-- Switch / checkbox -------------------------------------------------------- -->
        <section data-section="switch">
          {@render section("switch", "Switch and checkbox", "30×18 switch, 200ms spring; 14px checkbox")}
          <div class="flex flex-wrap items-center gap-6">
            <label class="flex items-center gap-2"><Switch bind:checked={inspect} data-testid="switch" />Inspect requests</label>
            <label class="flex items-center gap-2"><Switch bind:checked={autostart} />Start automatically</label>
            <label class="flex items-center gap-2"><Switch checked disabled />Disabled on</label>
            <label class="flex items-center gap-2"><Switch disabled />Disabled off</label>
            <label class="flex items-center gap-2"><Switch size="sm" checked />Small</label>
            <label class="flex items-center gap-2"><Switch checked aria-busy="true" class="*:animate-pulse" />Busy</label>
          </div>
          <div class="mt-3 flex flex-wrap items-center gap-6">
            <label class="flex items-center gap-2"><Checkbox bind:checked={agree} data-testid="checkbox" />Checked</label>
            <label class="flex items-center gap-2"><Checkbox />Unchecked</label>
            <label class="flex items-center gap-2"><Checkbox bind:indeterminate={partial} checked={false} onclick={() => (partial = !partial)} />Mixed (click)</label>
            <label class="flex items-center gap-2"><Checkbox indeterminate />Indeterminate</label>
            <label class="flex items-center gap-2 text-faint"><Checkbox disabled checked />Disabled</label>
          </div>
        </section>

        <!-- Segmented / tabs / toggles ----------------------------------------------- -->
        <section data-section="segmented">
          {@render section("segmented", "Segmented, tabs, toggles", "one 22px height; arrows move focus, Space selects; clicking the selected segment keeps it")}
          <div class="flex flex-wrap items-center gap-4">
            <div data-testid="segmented">
              <Segmented label="Appearance" bind:value={appearance} onchange={() => segmentChanges++} items={[{ value: "system", label: "System" }, { value: "light", label: "Light" }, { value: "dark", label: "Dark" }]} />
            </div>
            <span class="text-xs text-muted-foreground" data-testid="segmented-state">value={appearance} changes={segmentChanges}</span>
            <Segmented label="Body view" bind:value={bodyView} items={[{ value: "pretty", label: "Pretty", disabled: true }, { value: "raw", label: "Raw" }, { value: "hex", label: "Hex" }]} />
          </div>
          <div class="mt-3 flex flex-wrap items-center gap-4">
            <ToggleGroup.Root type="multiple" bind:value={levels} aria-label="Levels">
              {#each ["debug", "info", "warn", "error"] as l (l)}<ToggleGroup.Item value={l}>{l}</ToggleGroup.Item>{/each}
            </ToggleGroup.Root>
            <Toggle bind:pressed={wrap} aria-label="Wrap lines"><WrapText />Wrap</Toggle>
            <Toggle variant="outline" aria-label="Bolt"><Zap /></Toggle>
          </div>
          <div class="mt-4 grid grid-cols-2 gap-6">
            <Tabs.Root bind:value={side}>
              <Tabs.List data-testid="tabs">
                <Tabs.Trigger value="req">Request</Tabs.Trigger>
                <Tabs.Trigger value="resp">Response</Tabs.Trigger>
              </Tabs.List>
              <Tabs.Content value="req" class="pt-2 text-callout text-muted-foreground">GET /e2e?source=run.mjs</Tabs.Content>
              <Tabs.Content value="resp" class="pt-2 text-callout text-muted-foreground">200 OK · 12 ms · 48 B</Tabs.Content>
            </Tabs.Root>
            <Tabs.Root bind:value={addTab}>
              <Tabs.List>
                <Tabs.Trigger value="quick"><Zap />Quick</Tabs.Trigger>
                <Tabs.Trigger value="token"><KeyRound />Token</Tabs.Trigger>
                <Tabs.Trigger value="import"><FileText />Import</Tabs.Trigger>
                <Tabs.Trigger value="x" disabled>Disabled</Tabs.Trigger>
              </Tabs.List>
            </Tabs.Root>
          </div>
        </section>

        <!-- Small things ------------------------------------------------------------- -->
        <section data-section="small">
          {@render section("small", "Badge, kbd, spinner, alert, empty", "01 §5.22–5.26: owner review before 2b")}
          <div class="flex flex-wrap items-center gap-2">
            <Badge>Decoded gzip</Badge>
            <Badge variant="ok">2</Badge>
            <Badge variant="warning">Truncated at 64 KB</Badge>
            <Badge variant="destructive">500</Badge>
            <Badge variant="outline">Server-sent events</Badge>
            <Separator orientation="vertical" class="mx-2 h-4" />
            <Kbd>⌘N</Kbd>
            <KbdGroup><Kbd>⇧⌘C</Kbd></KbdGroup>
            <Kbd>⌥⌘L</Kbd>
            <Separator orientation="vertical" class="mx-2 h-4" />
            <Spinner />
            <Spinner class="size-3" />
          </div>
          <div class="mt-3 grid grid-cols-2 gap-3">
            <Alert.Root variant="warning">
              <TriangleAlert />
              <Alert.Description>cloudflared isn't installed. Run <code class="font-mono text-[11px]">brew install cloudflared</code>, then open Settings.</Alert.Description>
            </Alert.Root>
            <Alert.Root variant="destructive">
              <Alert.Title>Couldn't save settings</Alert.Title>
              <Alert.Description>{longError}</Alert.Description>
            </Alert.Root>
            <Alert.Root>
              <Alert.Title>Default alert</Alert.Title>
              <Alert.Description>Quick tunnels get a new random hostname every time they start.</Alert.Description>
            </Alert.Root>
          </div>
          <div class="mt-3 grid grid-cols-2 gap-3 rounded-lg bg-card shadow-(--shadow)">
            <Empty.Root>
              <Empty.Media variant="icon"><Search /></Empty.Media>
              <Empty.Title>No requests match</Empty.Title>
              <Empty.Description>Try a different path or method, or clear the filter.</Empty.Description>
              <Empty.Content><Button variant="outline">Clear filter</Button></Empty.Content>
            </Empty.Root>
            <Empty.Root>
              <Empty.Media><ArchGlyph class="text-faint" /></Empty.Media>
              <Empty.Header>
                <Empty.Title>Empty with header</Empty.Title>
                <Empty.Description>Header groups the title and the description.</Empty.Description>
              </Empty.Header>
            </Empty.Root>
          </div>
        </section>

        <!-- Overlays ----------------------------------------------------------------- -->
        <section data-section="overlays">
          {@render section("overlays", "Dialog sheet, alert dialog, combobox, tooltip, toast", "sheets drop from under the 52px toolbar; Escape closes; focus returns to the trigger")}
          <div class="flex flex-wrap items-center gap-2">
            <Button bind:ref={sheetTrigger} onclick={() => (sheetOpen = true)} data-testid="open-sheet">Add tunnel…</Button>
            <AlertDialog.Root bind:open={alertOpen}>
              <AlertDialog.Trigger bind:ref={alertTrigger}>
                {#snippet child({ props })}<Button {...props} variant="destructive" data-testid="open-alert"><Trash2 />Remove…</Button>{/snippet}
              </AlertDialog.Trigger>
              <AlertDialog.Content onCloseAutoFocus={returnFocusTo(() => alertTrigger)} data-testid="alert">
                <AlertDialog.Header>
                  <AlertDialog.Title>Remove “web”?</AlertDialog.Title>
                  <AlertDialog.Description>The config file stays on disk.</AlertDialog.Description>
                </AlertDialog.Header>
                <AlertDialog.Footer>
                  <AlertDialog.Cancel>Cancel</AlertDialog.Cancel>
                  <AlertDialog.Action variant="destructive" onclick={() => notify.copied("nothing (lab)")}>Remove</AlertDialog.Action>
                </AlertDialog.Footer>
              </AlertDialog.Content>
            </AlertDialog.Root>

            <span data-testid="combobox-wrap" class="contents"><Combobox bind:open={comboOpen} value={project} items={projects} heading="Projects" placeholder="Move to project…" onpick={pick} /></span>

            <Tip text="Restart the tunnel" shortcut="⌘R">
              {#snippet children({ props })}<Button {...props} variant="outline" data-testid="tip-button">Hover for a tooltip</Button>{/snippet}
            </Tip>
          </div>
          <div class="mt-3 flex flex-wrap items-center gap-2">
            <Button variant="outline" onclick={() => notify.copied("synthetic-quick-12.trycloudflare.com")} data-testid="toast-copied">Toast: copied</Button>
            <Button variant="outline" onclick={() => notify.saved()}>Toast: saved</Button>
            <Button variant="outline" onclick={() => notify.error(new Error(longError), "Couldn't start web")} data-testid="toast-error">Toast: error</Button>
            <Button variant="outline" onclick={() => notify.dismiss()}>Dismiss all</Button>
          </div>
        </section>

        <!-- App composites ------------------------------------------------------------ -->
        <section data-section="composites">
          {@render section("composites", "Settings rows", "SettingsGroup + SettingsRow; every control is labelled (F13)")}
          <SettingsGroup title="General">
            {#snippet aside()}Saved automatically{/snippet}
            <SettingsRow label="Launch at login" help="Opens Burrow in the menu bar when you log in.">
              {#snippet control({ id, describedBy })}<Switch {id} bind:checked={launch} aria-describedby={describedBy} />{/snippet}
            </SettingsRow>
            <SettingsRow label="Appearance">
              {#snippet control({ id, labelId, describedBy })}<Segmented {id} label="Appearance" aria-labelledby={labelId} aria-describedby={describedBy} bind:value={appearance} items={[{ value: "system", label: "System" }, { value: "light", label: "Light" }, { value: "dark", label: "Dark" }]} />{/snippet}
            </SettingsRow>
            <SettingsRow label="Default log level" help="Used for newly added tunnels.">
              {#snippet control({ id, describedBy })}
                <select {id} aria-describedby={describedBy} bind:value={loglevel}>{#each ["debug", "info", "warn", "error"] as l (l)}<option value={l}>{l}</option>{/each}</select>
              {/snippet}
            </SettingsRow>
            <SettingsRow label="Body capture limit" help="Bodies larger than this are stored truncated. 0 keeps headers only.">
              {#snippet control({ id, describedBy })}
                <InputGroup.Root class="w-28">
                  <InputGroup.Input {id} aria-describedby={describedBy} type="number" value="1024" class="text-right" />
                  <InputGroup.Addon align="inline-end">KB</InputGroup.Addon>
                </InputGroup.Root>
              {/snippet}
            </SettingsRow>
            <SettingsRow label="Path" help="Leave empty to look on PATH and in Homebrew's folders.">
              {#snippet control({ id, describedBy })}<Input {id} aria-describedby={describedBy} class="w-64 font-mono" placeholder="/opt/homebrew/bin/cloudflared" />{/snippet}
              <Button variant="outline">Detect</Button>
            </SettingsRow>
            <SettingsRow label="Redact these headers too" help="Authorization, Cookie and other credential headers are always redacted." column>
              {#snippet control({ id, describedBy })}<Input {id} aria-describedby={describedBy} class="font-mono" placeholder="X-Internal-Key, X-Session" />{/snippet}
            </SettingsRow>
          </SettingsGroup>
          <SettingsGroup title="Source">
            {#snippet footer()}Quick tunnels get a new random hostname every time they start.{/snippet}
            <SettingsRow label="Type"><span class="text-muted-foreground">Quick tunnel on trycloudflare.com</span></SettingsRow>
          </SettingsGroup>
        </section>

        <section data-section="shared">
          {@render section("shared", "Shared components (ported to utilities)", "StatusDot, RouteLine, StatePanel, Toolbar, ArchGlyph: same look as before the port")}
          <div class="flex flex-wrap items-center gap-5">
            {#each statuses as s (s)}<span class="flex items-center gap-2"><StatusDot status={s} />{s}</span>{/each}
            <span class="flex items-center gap-2"><StatusDot status="connected" size={10} />10px</span>
          </div>
          <div class="mt-3 grid max-w-[340px] gap-1.5">
            <RouteLine hostname="synthetic-quick-15.trycloudflare.com" service="http://127.0.0.1:57990" compact {pulse} inspected />
            <RouteLine hostname="api.example.com" service="http://localhost:8080" compact />
            <RouteLine hostname={null} service="http_status:404" />
            <RouteLine hostname="docs.example.com" service="http://localhost:3000" {pulse} />
            <Button variant="outline" class="justify-self-start" onclick={() => pulse++}>Send a pulse</Button>
          </div>
          <div class="mt-3 grid grid-cols-3 gap-3 rounded-lg bg-card shadow-(--shadow)">
            <StatePanel kind="empty" title="No tunnels yet" message="Import a config.yml, paste a tunnel token, or share a local port with a quick tunnel.">
              <Button>Add a tunnel</Button>
            </StatePanel>
            <StatePanel kind="loading" title="Loading tunnels…" />
            <StatePanel kind="error" title="Couldn't load tunnels" message={longError}>
              <Button variant="outline">Try again</Button>
            </StatePanel>
          </div>
          <div class="mt-3 overflow-hidden rounded-lg bg-background shadow-(--shadow)">
            <Toolbar title="Tunnels" subtitle="2 of 3 running">
              <Button variant="outline"><RotateCw />Restart all</Button>
              <Button><Plus />Add tunnel</Button>
            </Toolbar>
          </div>
          <div class="mt-3 flex items-end gap-4 text-faint">
            <ArchGlyph />
            <ArchGlyph size={24} strokeWidth={3} />
            <ArchGlyph size={15} strokeWidth={4.2} class="text-foreground" />
          </div>
        </section>

        <!-- virtua ------------------------------------------------------------------- -->
        <section data-section="virtua">
          {@render section("virtua", "virtua VList", "3,000 variable-height rows and 500 fixed 28px rows; R-21/R-22 probes")}
          <VirtuaLab />
        </section>

        <!-- Requests list (03 R-49) --------------------------------------------------- -->
        <section data-section="requests">
          {@render section("requests", "Requests list", "the real RequestList at the 500 cap: listbox + VList, keys, by-id selection, anchoring under prepend + trim")}
          <RequestsLab />
        </section>
      </div>
    </div>
  </div>

  <Dialog.Root bind:open={sheetOpen}>
    <Dialog.Content onCloseAutoFocus={returnFocusTo(() => sheetTrigger)} data-testid="sheet">
      <form class="contents" onsubmit={(e) => { e.preventDefault(); sheetOpen = false; notify.copied("nothing (lab)"); }}>
        <Dialog.Header>
          <Dialog.Title>Add tunnel</Dialog.Title>
          <Dialog.Description>Share a local server at a random <span class="font-mono">trycloudflare.com</span> address.</Dialog.Description>
        </Dialog.Header>
        <Tabs.Root bind:value={addTab}>
          <Tabs.List>
            <Tabs.Trigger value="quick"><Zap />Quick</Tabs.Trigger>
            <Tabs.Trigger value="token"><KeyRound />Token</Tabs.Trigger>
            <Tabs.Trigger value="import"><FileText />Import</Tabs.Trigger>
          </Tabs.List>
        </Tabs.Root>
        <div class="grid grid-cols-[120px_1fr] items-center gap-x-3 gap-y-2.5">
          <Label for="addr" class="justify-self-end">Local address:</Label>
          <Input id="addr" class="font-mono" placeholder="localhost:3000" />
          <Label for="name" class="justify-self-end">Name:</Label>
          <Input id="name" placeholder="Optional" />
          <span></span>
          <label class="flex items-center gap-2"><Switch bind:checked={inspect} />Inspect requests</label>
        </div>
        <Dialog.Footer>
          <Button variant="outline" onclick={() => (sheetOpen = false)}>Cancel</Button>
          <Button type="submit">Create and start</Button>
        </Dialog.Footer>
      </form>
    </Dialog.Content>
  </Dialog.Root>

  <Toaster {theme} position="bottom-right" visibleToasts={3} />
</Tooltip.Provider>
