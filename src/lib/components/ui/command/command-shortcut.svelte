<script lang="ts">
	// burrow: macOS overrides, see .scratch/shadcn-luma/01-theming.md §5.18 (stock Luma: shadcn-svelte@1.7.0 add; the spike tuned only command and command-item).
	import { cn, type WithElementRef } from "$lib/utils.js";
	import type { HTMLAttributes } from "svelte/elements";

	let {
		ref = $bindable(null),
		class: className,
		children,
		...restProps
	}: WithElementRef<HTMLAttributes<HTMLSpanElement>> = $props();
</script>

<span
	bind:this={ref}
	data-slot="command-shortcut"
	class={cn("ms-auto ps-6 text-sm tracking-normal text-muted-foreground group-data-selected/command-item:text-accent-foreground", className)}
	{...restProps}
>
	{@render children?.()}
</span>
