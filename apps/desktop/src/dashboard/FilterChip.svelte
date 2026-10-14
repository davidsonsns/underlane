<script lang="ts">
  import X from "@lucide/svelte/icons/x";
  import IconAction from "$lib/components/app/IconAction.svelte";

  let {
    label,
    value = $bindable(""),
    removeLabel = `Remove ${label.toLowerCase()} filter`,
    hidden = false,
    onremove,
    onfocus,
  }: {
    label: string;
    value: string;
    removeLabel?: string;
    hidden?: boolean;
    onremove?: () => void;
    onfocus?: () => void;
  } = $props();
</script>
{#if !hidden}
  <div
    class="flex items-center gap-1 rounded-md border border-input bg-fill/50 pl-2 text-xs"
    title={value}
  >
    <span class="text-muted-foreground">{label}</span>
    {#if onfocus}
      <button
        class="max-w-48 truncate rounded-sm kb-focus:ring-[3px] kb-focus:ring-ring/50 outline-none"
        onclick={onfocus}
        type="button"
      >
        {value}
      </button>
    {:else}
      <span class="max-w-48 truncate">{value}</span>
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
