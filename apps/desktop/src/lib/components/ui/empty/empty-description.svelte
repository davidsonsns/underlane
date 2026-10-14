<script lang="ts">
	// underlane: macOS overrides, see .scratch/shadcn-luma/01-theming.md §5.23 (stock Luma: shadcn-svelte@1.7.0 add).
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
	data-slot="empty-description"
	class={cn(
		"max-w-80 text-callout text-muted-foreground [&>a]:text-link [&>a]:underline-offset-2 [&>a:hover]:underline",
		className
	)}
	{...restProps}
>
	{@render children?.()}
</div>
