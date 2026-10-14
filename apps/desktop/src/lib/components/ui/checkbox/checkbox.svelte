<script lang="ts">
	// underlane: macOS overrides, see .scratch/shadcn-luma/01-theming.md §5.21 (stock Luma: shadcn-svelte@1.7.0 add).
	// NSButton checkbox: 14px, 3px corners, control bezel; checked/mixed fill with the tint and
	// a white glyph; ring only for keyboard navigation (kbRing).
	import { Checkbox as CheckboxPrimitive } from "bits-ui";
	import CheckIcon from "@lucide/svelte/icons/check";
	import MinusIcon from "@lucide/svelte/icons/minus";
	import { cn, type WithoutChildrenOrChild } from "$lib/utils.js";
	import { kbRing } from "../mac.js";

	let {
		ref = $bindable(null),
		checked = $bindable(false),
		indeterminate = $bindable(false),
		class: className,
		...restProps
	}: WithoutChildrenOrChild<CheckboxPrimitive.RootProps> = $props();
</script>

<CheckboxPrimitive.Root
	bind:ref
	data-slot="checkbox"
	class={cn(
		"peer relative flex size-[14px] shrink-0 items-center justify-center rounded-[3px] bg-control text-white border-[0.5px] border-input transition-colors duration-100 data-checked:border-transparent data-checked:bg-tint data-[state=indeterminate]:border-transparent data-[state=indeterminate]:bg-tint disabled:cursor-default disabled:opacity-45 aria-invalid:ring-[3px] aria-invalid:ring-destructive/40 after:absolute after:-inset-x-1.5 after:-inset-y-1",
		kbRing,
		className
	)}
	bind:checked
	bind:indeterminate
	{...restProps}
>
	{#snippet children({ checked, indeterminate })}
		<div data-slot="checkbox-indicator" class="grid place-content-center text-current [&>svg]:size-2.5 [&>svg]:stroke-[3]">
			{#if indeterminate}
				<MinusIcon />
			{:else if checked}
				<CheckIcon />
			{/if}
		</div>
	{/snippet}
</CheckboxPrimitive.Root>
