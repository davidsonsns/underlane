<script lang="ts">
	// burrow: macOS overrides, see .scratch/shadcn-luma/01-theming.md §5.23 (stock Luma: shadcn-svelte@1.7.0 add).
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
	data-slot="empty"
	class={cn(
		// No card or border: a centred stack (StatePanel's metrics). Add flex-1 at the call
		// site to fill the pane.
		"flex min-w-0 flex-col items-center justify-center gap-1.5 px-5 py-8 text-center text-muted-foreground",
		className
	)}
	{...restProps}
>
	{@render children?.()}
</div>
