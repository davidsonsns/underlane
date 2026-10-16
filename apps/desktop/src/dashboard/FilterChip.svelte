<script lang="ts">
  import X from "@lucide/svelte/icons/x";
  import IconAction from "$lib/components/app/IconAction.svelte";

  let {
    label,
    value = $bindable(""),
    values,
    removeLabel = `Remove ${label.toLowerCase()} filter`,
    hidden = false,
    onremove,
    onfocus,
  }: {
    label: string;
    value?: string;
    values?: (string | number)[];
    removeLabel?: string;
    hidden?: boolean;
    onremove?: () => void;
    onfocus?: () => void;
  } = $props();
  const displayValue = $derived(values?.slice(0, 2).join(", ") ?? value);
  const fullValue = $derived(values?.join(", ") ?? value);
</script>
{#if !hidden}
  <div
    class="flex items-center gap-1 rounded-md border border-input bg-fill/50 pl-2 text-xs"
    title={fullValue}
  >
    <span class="text-muted-foreground">{label}</span>
    {#if onfocus}
      <button
        class="max-w-48 truncate rounded-sm kb-focus:ring-[3px] kb-focus:ring-ring/50 outline-none"
        onclick={onfocus}
        type="button"
      >
        {displayValue}
      </button>
    {:else}
      <span class="max-w-48 truncate">{displayValue}</span>
    {/if}
    {#if values && values.length > 2}
      <span class="shrink-0 text-muted-foreground">+{values.length - 2}</span>
    {/if}
    <IconAction
      label={removeLabel}
      onclick={() => {
        if (onremove) {
          onremove();
        } else {
          value = "";
        }
      }}
      size="icon-sm"
      ><X /></IconAction
    >
  </div>
{/if}
