<script lang="ts">
	// burrow: macOS overrides, see .scratch/shadcn-luma/01-theming.md §5 (stock Luma: commit 9f0aa7b).
	import { cn, type WithElementRef, type WithoutChildren } from "$lib/utils.js";
	import type { HTMLTextareaAttributes } from "svelte/elements";

	let {
		ref = $bindable(null),
		value = $bindable(),
		class: className,
		"data-slot": dataSlot = "textarea",
		...restProps
	}: WithoutChildren<WithElementRef<HTMLTextareaAttributes>> = $props();
</script>

<textarea
	bind:this={ref}
	data-slot={dataSlot}
	class={cn(
		"flex field-sizing-content min-h-16 w-full rounded-[5px] bg-control px-2 py-1 text-sm text-foreground shadow-[inset_0_0_0_0.5px_var(--line-strong)] outline-none placeholder:text-faint focus-visible:ring-[3px] focus-visible:ring-ring/40 disabled:cursor-default disabled:opacity-45 aria-invalid:ring-[3px] aria-invalid:ring-destructive/40",
		className
	)}
	bind:value
	{...restProps}></textarea>
