<script lang="ts">
  // One settings row: label + optional help on the left, the control on the right (or
  // below, with `column`). The control snippet gets the ids to wire up, so every input
  // has a programmatic label and description (02 F13):
  //   <SettingsRow label="Body capture limit" help="…">
  //     {#snippet control({ id, describedBy })}<Input {id} aria-describedby={describedBy} />{/snippet}
  //   </SettingsRow>
  // `labelId` is for controls that <label for> can't name (a Segmented group, a Switch's
  // aria-labelledby). Without a control the label is a plain span. The label hugs its text
  // (self-start), so a click in the row's blank space doesn't toggle a Switch. `children` renders after the control (a unit, a Detect button).
  import type { Snippet } from "svelte";
  import { cn } from "$lib/utils";

  interface Ids {
    describedBy: string | undefined;
    id: string;
    labelId: string;
  }
  let {
    label,
    help,
    column = false,
    class: className,
    control,
    children,
  }: {
    label: string;
    help?: string;
    column?: boolean;
    class?: string;
    control?: Snippet<[Ids]>;
    children?: Snippet;
  } = $props();

  const uid = $props.id();
  const ids = $derived<Ids>({
    describedBy: help ? `${uid}-help` : undefined,
    id: `${uid}-control`,
    labelId: `${uid}-label`,
  });
</script>

<div
  class={cn(
    "flex min-h-[58px] items-center gap-4 py-3.5",
    column && "flex-col items-stretch",
    className
  )}
>
  <div class="flex min-w-0 flex-1 flex-col">
    {#if control}
      <label class="self-start" for={ids.id} id={ids.labelId}>{label}</label>
    {:else}
      <span id={ids.labelId}>{label}</span>
    {/if}
    {#if help}
      <span
        class="mt-1 text-callout leading-[1.4] text-muted-foreground"
        id={ids.describedBy}
        >{help}</span
      >
    {/if}
  </div>
  {@render control?.(ids)}
  {@render children?.()}
</div>
