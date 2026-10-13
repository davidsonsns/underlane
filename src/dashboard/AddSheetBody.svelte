<script lang="ts" module>
  export type AddTab = "project" | "quick";
</script>

<script lang="ts">
  // The body remounts on every opening, so cancelled drafts never leak into the next New form.
  import { onMount } from "svelte";
  import { open as openPicker } from "@tauri-apps/plugin-dialog";
  import FileText from "@lucide/svelte/icons/file-text";
  import Folder from "@lucide/svelte/icons/folder";
  import FolderPlus from "@lucide/svelte/icons/folder-plus";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import Zap from "@lucide/svelte/icons/zap";
  import { api, errorText } from "../lib/api";
  import { store } from "../lib/store.svelte";
  import { serviceLabel } from "../lib/format";
  import { projectFilename } from "../lib/project-filename";
  import type { ProjectConfigPreview } from "../lib/types";
  import Combobox from "$lib/components/app/Combobox.svelte";
  import * as Dialog from "$lib/components/ui/dialog/index.js";
  import * as Alert from "$lib/components/ui/alert/index.js";
  import * as Tabs from "$lib/components/ui/tabs/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import { Label } from "$lib/components/ui/label/index.js";

  let {
    initialTab = "project",
    busy = $bindable(false),
    onclose,
    onadded,
  }: { initialTab?: AddTab; busy?: boolean; onclose: () => void; onadded: (id: string) => void | Promise<void> } = $props();

  // svelte-ignore state_referenced_locally
  let tab = $state<AddTab>(initialTab);
  let project = $state({ name: "", group: "" });
  let origin = $state("");
  let defaultDirectory = $state<string | null>(null);
  let directory = $state<string | null>(null);
  let directoryError = $state<string | null>(null);
  let chosenPath = $state<string | null>(null);
  let preview = $state<ProjectConfigPreview | null>(null);
  let configError = $state<string | null>(null);
  let error = $state<string | null>(null);
  let picking = $state(false);
  // A successful write is never repeated if refreshing the dashboard fails afterward.
  let createdId = $state<string | null>(null);
  let alive = true;
  const locked = $derived(busy || picking || createdId !== null);
  const saveDirectory = $derived(directory ?? defaultDirectory);
  const destination = $derived(saveDirectory ? `${saveDirectory}/${projectFilename(project.name)}` : null);
  const plainText = { autocorrect: "off", autocapitalize: "off", spellcheck: false, writingsuggestions: false } as const;
  const uid = $props.id();
  const id = (name: string) => `${uid}-${name}`;

  onMount(() => {
    api.projectDirectory().then((path) => { if (alive) defaultDirectory = path; })
      .catch((e) => { if (alive) directoryError = errorText(e); });
    return () => { alive = false; };
  });

  async function loadConfig() {
    if (locked) return;
    picking = true;
    error = null;
    try {
      const path = await openPicker({ multiple: false, directory: false, filters: [{ name: "cloudflared config", extensions: ["yml", "yaml"] }] });
      if (!alive || !path || Array.isArray(path)) return;
      chosenPath = path;
      preview = null;
      configError = null;
      try {
        const result = await api.previewProjectConfig(path);
        if (alive) { preview = result; chosenPath = result.path; }
      } catch (e) {
        if (alive) configError = errorText(e);
      }
    } catch (e) {
      if (alive) error = errorText(e);
    } finally {
      if (alive) picking = false;
    }
  }

  async function chooseDirectory() {
    if (locked) return;
    picking = true;
    error = null;
    try {
      const path = await openPicker({ multiple: false, directory: true, defaultPath: saveDirectory ?? undefined });
      if (alive && typeof path === "string") directory = path;
    } catch (e) {
      if (alive) error = errorText(e);
    } finally {
      if (alive) picking = false;
    }
  }

  function removeConfig() {
    chosenPath = null;
    preview = null;
    configError = null;
    error = null;
  }

  async function create() {
    if (busy || picking) return;
    const finish = onadded;
    busy = true;
    error = null;
    try {
      if (!createdId) {
        createdId = tab === "project"
          ? await api.createProject(project.name, project.group, preview?.path ?? null, directory)
          : await api.addQuick("", origin, "Quick tunnels", false);
      }
      if (alive) await finish(createdId);
    } catch (e) {
      if (alive) error = errorText(e);
    } finally {
      if (alive) busy = false;
    }
  }
</script>

{#snippet failure(message: string | null)}
  {#if message}
    <Alert.Root variant="destructive">
      {#snippet icon()}<CircleAlert />{/snippet}
      <Alert.Description class="whitespace-pre-wrap text-foreground">{message}</Alert.Description>
    </Alert.Root>
  {/if}
{/snippet}

<Dialog.Header><Dialog.Title>New</Dialog.Title></Dialog.Header>

<Tabs.Root class="min-w-0" bind:value={() => tab, (v) => { tab = v as AddTab; error = null; }}>
  <Tabs.List class="self-center">
    <Tabs.Trigger value="project" disabled={locked}><FolderPlus />Project</Tabs.Trigger>
    <Tabs.Trigger value="quick" disabled={locked}><Zap />Quick tunnel</Tabs.Trigger>
  </Tabs.List>

  <Tabs.Content value="project" tabindex={-1}>
    <form class="flex min-w-0 flex-col gap-4" onsubmit={(e) => { e.preventDefault(); create(); }}>
      <div class="grid grid-cols-2 gap-3">
        <div class="flex min-w-0 flex-col gap-1.5">
          <Label for={id("name")}>Name</Label>
          <Input id={id("name")} {...plainText} placeholder="my-app" required disabled={locked} bind:value={project.name} />
        </div>
        <div class="flex min-w-0 flex-col gap-1.5">
          <Label id={id("group-label")} for={id("group")}>Group</Label>
          <div class="flex min-w-0 items-center gap-1">
            <Combobox id={id("group")} aria-labelledby="{id('group-label')} {id('group')}" value={project.group} items={(store.overview?.groups ?? []).map((g) => g.name)} class="min-w-0 w-full" placeholder="Choose or create a group…" disabled={locked} onpick={(name) => (project.group = name)} />
            {#if project.group}<Button variant="ghost" size="sm" disabled={locked} aria-label="Remove group" onclick={() => (project.group = "")}>Clear</Button>{/if}
          </div>
        </div>
      </div>
      <div class="flex min-w-0 flex-col gap-2">
        <p class="font-medium">Config <span class="font-normal text-muted-foreground">· optional</span></p>
        {#if chosenPath}
          <div class="flex min-w-0 items-start gap-2">
            <span class="min-w-0 flex-1 break-all font-mono text-code text-muted-foreground select-text">{chosenPath}</span>
            <Button variant="ghost" disabled={locked} onclick={removeConfig}>Remove</Button>
          </div>
          {#if picking}<p class="text-callout text-muted-foreground" role="status">Reading config…</p>{/if}
          {@render failure(configError)}
          {#if preview}
            <p class="text-callout text-muted-foreground">This project will use the selected file.</p>
            {#if preview.routes.length}
              <table class="w-full table-fixed text-left text-callout">
                <thead><tr class="text-xs font-normal text-muted-foreground"><th class="pb-2 font-normal">Hostname</th><th class="pb-2 font-normal">Local endpoint</th></tr></thead>
                <tbody>
                  {#each preview.routes as route, i (i)}
                    <tr class="border-t-[0.5px] border-border"><td class="break-all py-2 pe-3">{route.hostname ?? "Any other hostname"}{#if route.path}<span class="text-muted-foreground">{route.path}</span>{/if}</td><td class="break-all py-2 font-mono text-muted-foreground">{serviceLabel(route.service)}</td></tr>
                  {/each}
                </tbody>
              </table>
            {:else}<p class="text-callout text-muted-foreground">This config has no hostname rules.</p>{/if}
          {/if}
        {:else}
          <Button variant="outline" class="self-start" disabled={locked} onclick={loadConfig}><FileText class="size-3.5" /> Load config…</Button>
          <div class="mt-2 border-t-[0.5px] border-border pt-3">
            <p class="text-callout text-muted-foreground">A new config will be created at</p>
            <div class="mt-1.5 flex min-w-0 items-start gap-2">
              <span class="min-w-0 flex-1 break-all font-mono text-code text-muted-foreground select-text">{destination ?? "Finding your config folder…"}</span>
              <Button variant="outline" disabled={locked} onclick={chooseDirectory}><Folder class="size-3.5" /> Change…</Button>
            </div>
          </div>
          {#if !saveDirectory}{@render failure(directoryError)}{/if}
        {/if}
      </div>
      {@render failure(error)}
      <Dialog.Footer class="mt-1 border-t-[0.5px] border-border pt-4">
        <Button variant="ghost" disabled={busy} onclick={onclose}>Cancel</Button>
        <Button type="submit" disabled={busy || picking || (!createdId && (!project.name.trim() || (chosenPath ? !preview : !saveDirectory)))}>{busy ? "Creating…" : createdId ? "Open project" : "Create project"}</Button>
      </Dialog.Footer>
    </form>
  </Tabs.Content>

  <Tabs.Content value="quick" tabindex={-1}>
    <form class="flex flex-col gap-3" onsubmit={(e) => { e.preventDefault(); create(); }}>
      <div class="flex flex-col gap-1.5">
        <Label for={id("origin")}>Local endpoint</Label>
        <Input id={id("origin")} {...plainText} class="font-mono" placeholder="localhost:3000" bind:value={origin} required disabled={locked} />
      </div>
      <p class="text-callout text-muted-foreground">Get a temporary public URL for your local app. No Cloudflare account needed.</p>
      {@render failure(error)}
      <Dialog.Footer class="mt-1 border-t-[0.5px] border-border pt-4">
        <Button variant="ghost" disabled={busy} onclick={onclose}>Cancel</Button>
        <Button type="submit" disabled={busy || picking || (!createdId && !origin.trim())}>{busy ? "Creating…" : createdId ? "Open tunnel" : "Create quick tunnel"}</Button>
      </Dialog.Footer>
    </form>
  </Tabs.Content>
</Tabs.Root>
