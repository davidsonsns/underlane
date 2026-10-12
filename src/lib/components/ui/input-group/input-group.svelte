<script lang="ts">
	// underlane: macOS overrides, see .scratch/shadcn-luma/01-theming.md §5.25 (stock Luma: shadcn-svelte@1.7.0 add).
	import { cn, type WithElementRef } from "$lib/utils.js";
	import type { HTMLAttributes } from "svelte/elements";

	let {
		ref = $bindable(null),
		class: className,
		children,
		...props
	}: WithElementRef<HTMLAttributes<HTMLDivElement>> = $props();
</script>

<div
	bind:this={ref}
	data-slot="input-group"
	role="group"
	class={cn(
		// The 22px NSTextField from input.svelte; the ring sits on the group, not the inner control.
		"group/input-group relative flex h-[22px] w-full min-w-0 items-center rounded-[5px] bg-control text-sm border-[0.5px] border-input has-[[data-slot=input-group-control]:focus-visible]:border-ring transition-[box-shadow] duration-100 outline-none has-[[data-slot][aria-invalid=true]]:ring-[3px] has-[[data-slot][aria-invalid=true]]:ring-destructive/40 has-[>textarea]:h-auto has-[>[data-align=block-end]]:h-auto has-[>[data-align=block-end]]:flex-col has-[>[data-align=block-start]]:h-auto has-[>[data-align=block-start]]:flex-col has-[>[data-align=inline-end]]:[&>input]:pr-0.5 has-[>[data-align=inline-start]]:[&>input]:pl-0.5",
		className
	)}
	{...props}
>
	{@render children?.()}
</div>
