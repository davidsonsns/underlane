<script lang="ts">
	// underlane: macOS overrides, see .scratch/shadcn-luma/01-theming.md §5.24 (stock Luma: shadcn-svelte@1.7.0 add).
	import { cn, type WithElementRef } from "$lib/utils.js";
	import type { HTMLAttributes } from "svelte/elements";

	let {
		ref = $bindable(null),
		class: className,
		children,
		...restProps
	}: WithElementRef<HTMLAttributes<HTMLDivElement>> = $props();
</script>

<div
	bind:this={ref}
	data-slot="alert-description"
	class={cn(
		"text-callout text-current group-has-[>svg]/alert:col-start-2 select-text [&_p:not(:last-child)]:mb-2 [&_a]:underline [&_a]:underline-offset-2",
		className
	)}
	{...restProps}
>
	{@render children?.()}
</div>
