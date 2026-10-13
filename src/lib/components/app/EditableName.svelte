<script lang="ts">
  import { mergeProps } from "bits-ui";
  import { errorText } from "$lib/api";
  import { groupNameError, normalizeGroupName } from "$lib/group-name";
  import { kbRing } from "$lib/components/ui/mac";
  import Tip from "./Tip.svelte";

  let { name, label, onsave, class: className = "" }: {
    name: string;
    label: string;
    onsave: (name: string) => Promise<void>;
    class?: string;
  } = $props();
  let editor = $state<HTMLDivElement | null>(null);
  let editing = $state(false);
  let saving = $state(false);
  let error = $state<string | null>(null);
  let nameAtStart = "";
  const hintId = $props.id();

  // Refreshes from other windows must not overwrite text while the user is editing it.
  $effect(() => {
    if (editor && !editing && !saving) editor.textContent = name;
  });

  function start() {
    if (editing || saving) return;
    nameAtStart = name;
    editing = true;
    error = null;
  }

  function validate() {
    start();
    error = groupNameError(normalizeGroupName(editor?.textContent ?? ""));
  }

  function blur() {
    const selection = window.getSelection();
    if (editor?.contains(selection?.anchorNode ?? null)) selection?.removeAllRanges();
    void save();
  }

  async function save() {
    if (!editing || saving) return;
    const normalized = normalizeGroupName(editor?.textContent ?? "");
    error = groupNameError(normalized);
    if (error) return;
    saving = true;
    try {
      if (normalized !== nameAtStart) await onsave(normalized);
      if (editor) editor.textContent = normalized;
      editing = false;
    } catch (e) {
      error = errorText(e);
    } finally {
      saving = false;
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.isComposing) return;
    if (e.key === "Enter") {
      e.preventDefault();
      void save();
    } else if (e.key === "Escape" && !saving) {
      e.preventDefault();
      if (editor) editor.textContent = name;
      editing = false;
      error = null;
    }
  }

  function paste(e: ClipboardEvent) {
    e.preventDefault();
    if (saving || !editor) return;
    start();
    const selection = window.getSelection();
    if (!selection?.rangeCount) return;
    const range = selection.getRangeAt(0);
    if (!editor.contains(range.commonAncestorContainer)) return;
    const text = document.createTextNode((e.clipboardData?.getData("text/plain") ?? "").replace(/\p{White_Space}+/gu, " "));
    range.deleteContents();
    range.insertNode(text);
    range.setStartAfter(text);
    range.collapse(true);
    selection.removeAllRanges();
    selection.addRange(range);
    validate();
  }
</script>

<svelte:window onpointerdowncapture={(e) => {
  if (editor && document.activeElement === editor && !e.composedPath().includes(editor)) editor.blur();
}} />

<div class="min-w-0">
  <Tip text={name} disabled={editing}>
    {#snippet children({ props })}
      <div
        {...mergeProps(props, { onfocus: start, onblur: blur })}
        bind:this={editor}
        contenteditable="plaintext-only"
        role="textbox"
        tabindex="0"
        aria-label={label}
        aria-multiline="false"
        aria-describedby={hintId}
        aria-invalid={!!error}
        aria-readonly={saving}
        aria-busy={saving}
        {...{ autocorrect: "off" }}
        autocapitalize="off"
        spellcheck={false}
        writingsuggestions="false"
        class="min-w-0 cursor-text select-text truncate text-left focus:text-clip {kbRing} {className}"
        onbeforeinput={(e) => { if (saving) e.preventDefault(); }}
        oninput={validate}
        onkeydown={onKey}
        onpaste={paste}
      ></div>
    {/snippet}
  </Tip>
  <span id={hintId} class={error ? "block text-xs text-bad-text" : "sr-only"} aria-live="polite">
    {error ?? "Up to 64 characters. Enter to save, Escape to cancel."}
  </span>
</div>
