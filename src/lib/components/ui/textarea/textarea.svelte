<script lang="ts">
	// underlane: macOS overrides, see .scratch/shadcn-luma/01-theming.md §5 (stock Luma: commit 9f0aa7b).
	// underlane: placeholder:text-placeholder (>= 4.5:1, 03-review R-48); text-faint is for disabled text only.
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
		"flex field-sizing-content min-h-16 w-full rounded-md bg-control px-2.5 py-2 text-sm text-foreground border border-input focus-visible:border-ring outline-none placeholder:text-placeholder disabled:cursor-default disabled:opacity-45 aria-invalid:ring-[3px] aria-invalid:ring-destructive/40",
		className
	)}
	bind:value
	{...restProps}></textarea>
