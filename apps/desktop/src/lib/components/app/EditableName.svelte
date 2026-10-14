<script lang="ts">
  import { mergeProps } from "bits-ui";
  import { errorText } from "$lib/api";
  import { kbRing } from "$lib/components/ui/mac";
  import { groupNameError, normalizeGroupName } from "$lib/group-name";
  import Tip from "./Tip.svelte";

  let {
    name,
    label,
    onsave,
    class: className = "",
  }: {
    name: string;
    label: string;
    onsave: (name: string) => Promise<void>;
    class?: string;
  } = $props();
  let editor = $state<HTMLDivElement | null>(null);
  let editing = $state(false);
  let saving = $state(false);
  let validationError = $state<string | null>(null);
  let nameAtStart = "";
  const hintId = $props.id();

  // Refreshes from other windows must not overwrite text while the user is editing it.
  $effect(() => {
    if (editor && !editing && !saving) {
      editor.textContent = name;
    }
  });

  function start() {
    if (editing || saving) {
      return;
    }
    nameAtStart = name;
    editing = true;
    validationError = null;
  }

  function validate() {
    start();
    validationError = groupNameError(
      normalizeGroupName(editor?.textContent ?? "")
    );
  }

  function blur() {
    const selection = window.getSelection();
    if (editor?.contains(selection?.anchorNode ?? null)) {
      selection?.removeAllRanges();
    }
    save();
  }

  function editorText(): string {
    return editor?.textContent ?? "";
  }

  function editableName(): string | null {
    if (!editing || saving) {
      return null;
    }
    const normalized = normalizeGroupName(editorText());
    validationError = groupNameError(normalized);
    if (validationError) {
      return null;
    }
    return normalized;
  }

  function setText(text: string) {
    if (editor) {
      editor.textContent = text;
    }
  }

  async function save() {
    const normalized = editableName();
    if (normalized === null) {
      return;
    }
    saving = true;
    try {
      if (normalized !== nameAtStart) {
        await onsave(normalized);
      }
      setText(normalized);
      editing = false;
    } catch (e) {
      validationError = errorText(e);
    } finally {
      saving = false;
    }
  }

  function cancel(e: KeyboardEvent) {
    if (saving) {
      return;
    }
    e.preventDefault();
    setText(name);
    editing = false;
    validationError = null;
  }

  function onKey(e: KeyboardEvent) {
    if (e.isComposing) {
      return;
    }
    if (e.key === "Enter") {
      e.preventDefault();
      save();
    }
    if (e.key === "Escape") {
      cancel(e);
    }
  }

  function pasteRange(): Range | null {
    const selection = window.getSelection();
    if (!selection?.rangeCount) {
      return null;
    }
    const range = selection.getRangeAt(0);
    return editor?.contains(range.commonAncestorContainer) ? range : null;
  }

  function insertText(range: Range, text: string) {
    const node = document.createTextNode(
      text.replace(/\p{White_Space}+/gu, " ")
    );
    range.deleteContents();
    range.insertNode(node);
    range.setStartAfter(node);
    range.collapse(true);
    const selection = window.getSelection();
    selection?.removeAllRanges();
    selection?.addRange(range);
  }

  function clipboardText(e: ClipboardEvent): string {
    return e.clipboardData?.getData("text/plain") ?? "";
  }

  function paste(e: ClipboardEvent) {
    e.preventDefault();
    if (saving || !editor) {
      return;
    }
    start();
    const range = pasteRange();
    if (!range) {
      return;
    }
    insertText(range, clipboardText(e));
    validate();
  }
</script>

<svelte:window
  onpointerdowncapture={(e) => {
    if (
      editor &&
      document.activeElement === editor &&
      !e.composedPath().includes(editor)
    ) {
      editor.blur();
    }
  }}
/>

<div class="min-w-0">
  <Tip disabled={editing} text={name}>
    {#snippet children({
      props,
    })}
      <!-- biome-ignore lint/a11y/useSemanticElements: This plaintext editor needs contenteditable selection and paste behavior. -->
      <div
        {...mergeProps(props, { onblur: blur, onfocus: start })}
        aria-busy={saving}
        aria-describedby={hintId}
        aria-invalid={!!validationError}
        aria-label={label}
        aria-multiline="false"
        aria-readonly={saving}
        contenteditable="plaintext-only"
        role="textbox"
        tabindex="0"
        bind:this={editor}
        {...{ autocorrect: "off" }}
        autocapitalize="off"
        class="min-w-0 cursor-text select-text truncate text-left focus:text-clip {kbRing} {className}"
        onbeforeinput={(e) => {
          if (saving) {
            e.preventDefault();
          }
        }}
        oninput={validate}
        onkeydown={onKey}
        onpaste={paste}
        spellcheck={false}
        writingsuggestions="false"
      ></div>
    {/snippet}
  </Tip>
  <span
    aria-live="polite"
    class={validationError ? "block text-xs text-bad-text" : "sr-only"}
    id={hintId}
  >
    {validationError ?? "Up to 64 characters. Enter to save, Escape to cancel."}
  </span>
</div>
