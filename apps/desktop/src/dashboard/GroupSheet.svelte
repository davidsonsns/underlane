<script lang="ts">
  import { onDestroy, untrack } from "svelte";
  import { api, errorText } from "$lib/api";
  import { Button } from "$lib/components/ui/button/index.js";
  import {
    Content as DialogContent,
    Description as DialogDescription,
    Footer as DialogFooter,
    Header as DialogHeader,
    Root as DialogRoot,
    Title as DialogTitle,
  } from "$lib/components/ui/dialog/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import { Label } from "$lib/components/ui/label/index.js";
  import { returnFocusTo } from "$lib/focus";
  import { groupNameError, normalizeGroupName } from "$lib/group-name";
  import { store } from "$lib/store.svelte";
  import { track } from "./reactivity";

  let {
    open = $bindable(false),
    group = null,
    returnFocus,
    onsaved,
  }: {
    open?: boolean;
    group?: { id: string; name: string } | null;
    returnFocus: () => HTMLElement | null | undefined;
    onsaved?: (id: string) => void;
  } = $props();

  let name = $state("");
  let saving = $state(false);
  let savedId = $state<string | null>(null);
  let failureMessage = $state<string | null>(null);
  let session = 0;
  const uid = $props.id();
  const normalized = $derived(normalizeGroupName(name));
  const validationError = $derived(groupNameError(normalized));
  const writeLabel = $derived(group ? "Save changes" : "Create group");
  const submitLabel = $derived(savedId ? "Retry refresh" : writeLabel);

  $effect.pre(() => {
    track(group?.id);
    if (open) {
      untrack(() => {
        session += 1;
        name = group?.name ?? "";
        saving = false;
        savedId = null;
        failureMessage = null;
      });
    } else {
      session += 1;
    }
  });
  onDestroy(() => {
    session += 1;
  });

  async function save() {
    if (!open || saving) {
      return;
    }
    failureMessage = savedId ? null : validationError;
    if (failureMessage) {
      return;
    }
    const current = session;
    const target = group;
    saving = true;
    let completedId = savedId;
    try {
      if (!completedId) {
        if (target) {
          await api.renameGroup(target.id, normalized);
          completedId = target.id;
        } else {
          completedId = await api.createGroup(normalized);
        }
        if (session === current) {
          savedId = completedId;
        }
      }
      await store.refresh();
      if (store.error) {
        throw new Error(store.error);
      }
    } catch (e) {
      if (session === current) {
        failureMessage = savedId
          ? `Group saved. ${errorText(e)}`
          : errorText(e);
      }
      return;
    } finally {
      if (session === current) {
        saving = false;
      }
    }
    if (session === current && completedId) {
      open = false;
      onsaved?.(completedId);
    }
  }
</script>

<DialogRoot
  bind:open={
    () => open,
    (value) => {
    if (!saving) {
      open = value;
    }
  }
  }
>
  <DialogContent
    class="w-[min(420px,calc(100vw-48px))]"
    onCloseAutoFocus={returnFocusTo(returnFocus)}
    onEscapeKeydown={(e) => {
      if (saving) {
        e.preventDefault();
      }
    }}
    onInteractOutside={(e) => {
      if (saving) {
        e.preventDefault();
      }
    }}
  >
    <DialogHeader>
      <DialogTitle>{group ? "Rename group" : "New group"}</DialogTitle>
      <DialogDescription>Keep related projects together.</DialogDescription>
    </DialogHeader>
    <form
      aria-busy={saving}
      class="flex flex-col gap-4"
      onsubmit={(e) => {
        e.preventDefault();
        save();
      }}
    >
      <div class="flex flex-col gap-1.5">
        <Label for={`${uid}-name`}>Group name</Label>
        <Input
          aria-describedby={`${uid}-hint`}
          aria-invalid={!!failureMessage}
          autocapitalize="off"
          {...{ autocorrect: "off" }}
          disabled={saving || savedId !== null}
          id={`${uid}-name`}
          oninput={() => {
            failureMessage = null;
          }}
          placeholder="Work"
          spellcheck={false}
          writingsuggestions="false"
          bind:value={name}
        />
        <p class="text-xs text-muted-foreground" id={`${uid}-hint`}>
          Up to 64 characters.
        </p>
      </div>
      {#if failureMessage}
        <p
          class="select-text text-callout whitespace-pre-wrap text-bad-text"
          role="alert"
        >
          {failureMessage}
        </p>
      {/if}
      <DialogFooter class="border-t-[0.5px] border-border pt-4">
        <Button
          disabled={saving}
          onclick={() => {
            open = false;
          }}
          variant="ghost"
          >Cancel</Button
        >
        <Button
          disabled={saving || (savedId === null && !!validationError)}
          type="submit"
          >{saving ? "Saving…" : submitLabel}</Button
        >
      </DialogFooter>
    </form>
  </DialogContent>
</DialogRoot>
