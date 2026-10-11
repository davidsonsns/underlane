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

  type Ids = { id: string; labelId: string; describedBy: string | undefined };
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
  const ids = $derived<Ids>({ id: `${uid}-control`, labelId: `${uid}-label`, describedBy: help ? `${uid}-help` : undefined });
</script>

<div class={cn("flex min-h-[42px] items-center gap-2.5 px-3 py-2.5", column && "flex-col items-stretch", className)}>
  <div class="flex min-w-0 flex-1 flex-col">
    {#if control}<label id={ids.labelId} for={ids.id} class="self-start">{label}</label>{:else}<span id={ids.labelId}>{label}</span>{/if}
    {#if help}<span id={ids.describedBy} class="mt-px text-xs leading-[1.4] text-muted-foreground">{help}</span>{/if}
  </div>
  {@render control?.(ids)}
  {@render children?.()}
</div>
