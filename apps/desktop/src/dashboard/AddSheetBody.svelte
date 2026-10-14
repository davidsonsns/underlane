<script lang="ts" module>
  export type AddTab = "project" | "quick";
</script>

<script lang="ts">
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import FolderPlus from "@lucide/svelte/icons/folder-plus";
  import Zap from "@lucide/svelte/icons/zap";
  import { open as openPicker } from "@tauri-apps/plugin-dialog";
  // The body remounts on every opening, so cancelled drafts never leak into the next New form.
  import { onMount } from "svelte";
  import Combobox from "$lib/components/app/Combobox.svelte";
  import {
    Description as AlertDescription,
    Root as AlertRoot,
  } from "$lib/components/ui/alert/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import {
    Footer as DialogFooter,
    Header as DialogHeader,
    Title as DialogTitle,
  } from "$lib/components/ui/dialog/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import { Label } from "$lib/components/ui/label/index.js";
  import {
    Content as SelectContent,
    Item as SelectItem,
    Root as SelectRoot,
    Trigger as SelectTrigger,
  } from "$lib/components/ui/select/index.js";
  import {
    Content as TabsContent,
    List as TabsList,
    Root as TabsRoot,
    Trigger as TabsTrigger,
  } from "$lib/components/ui/tabs/index.js";
  import { api, errorText } from "../lib/api";
  import { serviceLabel } from "../lib/format";
  import { projectFilename } from "../lib/project-filename";
  import { store } from "../lib/store.svelte";
  import type { Candidate, ProjectConfigPreview } from "../lib/types";

  let {
    initialTab = "project",
    busy = $bindable(false),
    onclose,
    onadded,
  }: {
    initialTab?: AddTab;
    busy?: boolean;
    onclose: () => void;
    onadded: (id: string) => void | Promise<void>;
  } = $props();

  // svelte-ignore state_referenced_locally
  let tab = $state<AddTab>(initialTab);
  let project = $state({ group: "", name: "" });
  let origin = $state("");
  let defaultDirectory = $state<string | null>(null);
  let directory = $state<string | null>(null);
  let directoryError = $state<string | null>(null);
  let chosenPath = $state<string | null>(null);
  let preview = $state<ProjectConfigPreview | null>(null);
  let configError = $state<string | null>(null);
  let failureMessage = $state<string | null>(null);
  let picking = $state(false);
  let candidates = $state<Candidate[]>([]);
  let configSource = $state("new");
  let scanError = $state<string | null>(null);
  // A successful write is never repeated if refreshing the dashboard fails afterward.
  let createdId = $state<string | null>(null);
  let alive = true;
  const locked = $derived(busy || picking || createdId !== null);
  const saveDirectory = $derived(directory ?? defaultDirectory);
  const plainText = {
    autocapitalize: "off",
    autocorrect: "off",
    spellcheck: false,
    writingsuggestions: false,
  } as const;
  const uid = $props.id();
  const id = (name: string) => `${uid}-${name}`;

  onMount(() => {
    api
      .scanDefault()
      .then((files) => {
        if (alive) {
          candidates = files.filter((file) => !file.imported);
        }
      })
      .catch((e) => {
        if (alive) {
          scanError = errorText(e);
        }
      });
    api
      .projectDirectory()
      .then((path) => {
        if (alive) {
          defaultDirectory = path;
        }
      })
      .catch((e) => {
        if (alive) {
          directoryError = errorText(e);
        }
      });
    return () => {
      alive = false;
    };
  });

  async function loadConfig(selectedPath?: string) {
    if (locked) {
      return;
    }
    picking = true;
    failureMessage = null;
    try {
      const path =
        selectedPath ??
        (await openPicker({
          directory: false,
          filters: [
            { extensions: ["yml", "yaml"], name: "cloudflared config" },
          ],
          multiple: false,
        }));
      if (!(alive && path) || Array.isArray(path)) {
        return;
      }
      chosenPath = path;
      preview = null;
      configError = null;
      try {
        const result = await api.previewProjectConfig(path);
        if (alive) {
          preview = result;
          chosenPath = result.path;
          if (!project.name.trim()) {
            project.name =
              candidates.find((file) => file.path === path)?.name ?? "";
          }
        }
      } catch (e) {
        if (alive) {
          configError = errorText(e);
        }
      }
    } catch (e) {
      if (alive) {
        failureMessage = errorText(e);
      }
    } finally {
      if (alive) {
        picking = false;
      }
    }
  }

  async function chooseDirectory() {
    if (locked) {
      return;
    }
    picking = true;
    failureMessage = null;
    try {
      const path = await openPicker({
        defaultPath: saveDirectory ?? undefined,
        directory: true,
        multiple: false,
      });
      if (alive && typeof path === "string") {
        directory = path;
      }
    } catch (e) {
      if (alive) {
        failureMessage = errorText(e);
      }
    } finally {
      if (alive) {
        picking = false;
      }
    }
  }

  function removeConfig() {
    configSource = "new";
    chosenPath = null;
    preview = null;
    configError = null;
    failureMessage = null;
  }

  async function selectConfig(source: string) {
    if (locked) {
      return;
    }
    if (source === "new") {
      removeConfig();
      return;
    }
    configSource = source;
    await loadConfig(source === "browse" ? undefined : source);
    if (alive) {
      configSource = chosenPath ?? "new";
    }
  }

  function shortPath(path: string) {
    const suffix = "/.cloudflared/underlane";
    const home = defaultDirectory?.endsWith(suffix)
      ? defaultDirectory.slice(0, -suffix.length)
      : null;
    return home && path.startsWith(`${home}/`)
      ? `~${path.slice(home.length)}`
      : path;
  }

  async function create() {
    if (busy || picking) {
      return;
    }
    const finish = onadded;
    busy = true;
    failureMessage = null;
    try {
      if (!createdId) {
        createdId =
          tab === "project"
            ? await api.createProject(
                project.name,
                project.group,
                preview?.path ?? null,
                directory
              )
            : await api.addQuick("", origin, "Quick tunnels", false);
      }
      if (alive) {
        await finish(createdId);
      }
    } catch (e) {
      if (alive) {
        failureMessage = errorText(e);
      }
    } finally {
      if (alive) {
        busy = false;
      }
    }
  }

  const projectSubmitLabel = $derived.by(() => {
    if (busy) {
      return "Creating…";
    }
    if (createdId) {
      return "Open project";
    }
    return "Create project";
  });

  const quickSubmitLabel = $derived.by(() => {
    if (busy) {
      return "Creating…";
    }
    if (createdId) {
      return "Open tunnel";
    }
    return "Create quick tunnel";
  });
</script>
{#snippet configPreview(
  preview: ProjectConfigPreview
)}
  {#if preview.routes.length}
    <table class="w-full table-fixed text-left text-callout">
      <thead>
        <tr class="text-xs font-normal text-muted-foreground">
          <th class="pb-2 font-normal">Hostname</th>
          <th class="pb-2 font-normal">Local endpoint</th>
        </tr>
      </thead>
      <tbody>
        {#each preview.routes as route, i (i)}
          <tr class="border-t-[0.5px] border-border">
            <td class="break-all py-2 pe-3">
              {route.hostname ?? "Any other hostname"}
              {#if route.path}
                <span class="text-muted-foreground">{route.path}</span>
              {/if}
            </td>
            <td class="break-all py-2 font-mono text-muted-foreground">
              {serviceLabel(route.service)}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {:else}
    <p class="text-callout text-muted-foreground">
      This config has no hostname rules.
    </p>
  {/if}
{/snippet}

{#snippet projectConfig()}
  <div class="flex min-w-0 flex-col gap-2">
    <Label for={id("config")}>Configuration</Label>
    <SelectRoot
      disabled={locked}
      onValueChange={selectConfig}
      type="single"
      value={configSource}
    >
      <SelectTrigger class="w-full" id={id("config")}>
        {chosenPath ? chosenPath.split("/").at(-1) : "Create a new config"}
      </SelectTrigger>
      <SelectContent>
        <SelectItem label="Create a new config" value="new" />
        {#each candidates as file (file.path)}
          <SelectItem
            label={file.path.split("/.cloudflared/").at(-1) ?? file.path}
            value={file.path}
          />
        {/each}
        {#if chosenPath && !candidates.some((file) => file.path === chosenPath)}
          <SelectItem
            label={chosenPath.split("/").at(-1) ?? chosenPath}
            value={chosenPath}
          />
        {/if}
        <SelectItem label="Browse files…" value="browse" />
      </SelectContent>
    </SelectRoot>
    {@render failure(scanError)}
    {#if chosenPath}
      <p
        class="break-all font-mono text-code text-muted-foreground select-text"
        title={chosenPath}
      >
        {shortPath(chosenPath)}
      </p>
      {#if picking}
        <p class="text-callout text-muted-foreground" role="status">
          Reading config…
        </p>
      {/if}
      {@render failure(configError)}
      {#if preview}
        <p class="text-callout text-muted-foreground">
          This project will use the selected file.
        </p>
        {@render configPreview(preview)}
      {/if}
    {:else}
      <div class="mt-1.5 flex min-w-0 items-center gap-3">
        <div class="min-w-0 flex-1">
          <p class="font-medium">{projectFilename(project.name)}</p>
          <p
            class="break-all font-mono text-code text-muted-foreground select-text"
            title={saveDirectory ?? undefined}
          >
            {saveDirectory
              ? `${shortPath(saveDirectory)}/`
              : "Finding your config folder…"}
          </p>
        </div>
        <Button disabled={locked} onclick={chooseDirectory} variant="ghost"
          >Change…</Button
        >
      </div>
      {#if !saveDirectory}
        {@render failure(directoryError)}
      {/if}
    {/if}
  </div>
{/snippet}

{#snippet projectForm()}
  <TabsContent tabindex={-1} value="project">
    <form
      class="flex min-w-0 flex-col gap-4"
      onsubmit={(e) => {
        e.preventDefault();
        create();
      }}
    >
      <div class="grid grid-cols-2 gap-3">
        <div class="flex min-w-0 flex-col gap-1.5">
          <Label for={id("name")}>Name</Label>
          <Input
            id={id("name")}
            {...plainText}
            disabled={locked}
            placeholder="my-app"
            required
            bind:value={project.name}
          />
        </div>
        <div class="flex min-w-0 flex-col gap-1.5">
          <Label for={id("group")} id={id("group-label")}>Group</Label>
          <div class="flex min-w-0 items-center gap-1">
            <Combobox
              aria-labelledby="{id("group-label")} {id("group")}"
              class="min-w-0 w-full"
              disabled={locked}
              id={id("group")}
              items={(store.overview?.groups ?? []).map((g) => g.name)}
              onpick={(name) => {
                project.group = name;
              }}
              placeholder="Choose or create a group…"
              value={project.group}
            />
            {#if project.group}
              <Button
                aria-label="Remove group"
                disabled={locked}
                onclick={() => {
                  project.group = "";
                }}
                size="sm"
                variant="ghost"
                >Clear</Button
              >
            {/if}
          </div>
        </div>
      </div>
      {@render projectConfig()}
      {@render failure(failureMessage)}
      <DialogFooter class="mt-1 border-t-[0.5px] border-border pt-4">
        <Button disabled={busy} onclick={onclose} variant="ghost"
          >Cancel</Button
        >
        <Button
          disabled={busy ||
            picking ||
            (!createdId &&
              (!project.name.trim() ||
                (chosenPath ? !preview : !saveDirectory)))}
          type="submit"
          >{projectSubmitLabel}</Button
        >
      </DialogFooter>
    </form>
  </TabsContent>
{/snippet}

{#snippet failure(
  message: string | null
)}
  {#if message}
    <AlertRoot variant="destructive">
      {#snippet icon()}
        <CircleAlert />
      {/snippet}
      <AlertDescription class="whitespace-pre-wrap text-foreground"
        >{message}</AlertDescription
      >
    </AlertRoot>
  {/if}
{/snippet}

<DialogHeader><DialogTitle>New</DialogTitle></DialogHeader>

<TabsRoot
  class="min-w-0"
  bind:value={
    () => tab,
    (v) => {
    tab = v as AddTab;
    failureMessage = null;
  }
  }
>
  <TabsList class="self-center">
    <TabsTrigger disabled={locked} value="project"
      ><FolderPlus />Project</TabsTrigger
    >
    <TabsTrigger disabled={locked} value="quick"
      ><Zap />Quick tunnel</TabsTrigger
    >
  </TabsList>

  {@render projectForm()}

  <TabsContent tabindex={-1} value="quick">
    <form
      class="flex flex-col gap-3"
      onsubmit={(e) => {
        e.preventDefault();
        create();
      }}
    >
      <div class="flex flex-col gap-1.5">
        <Label for={id("origin")}>Local endpoint</Label>
        <Input
          id={id("origin")}
          {...plainText}
          class="font-mono"
          disabled={locked}
          placeholder="localhost:3000"
          required
          bind:value={origin}
        />
      </div>
      <p class="text-callout text-muted-foreground">
        Get a temporary public URL for your local app. No Cloudflare account
        needed.
      </p>
      {@render failure(failureMessage)}
      <DialogFooter class="mt-1 border-t-[0.5px] border-border pt-4">
        <Button disabled={busy} onclick={onclose} variant="ghost"
          >Cancel</Button
        >
        <Button
          disabled={busy || picking || !(createdId || origin.trim())}
          type="submit"
          >{quickSubmitLabel}</Button
        >
      </DialogFooter>
    </form>
  </TabsContent>
</TabsRoot>
