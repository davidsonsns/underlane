<script lang="ts">
	// burrow: macOS overrides, see .scratch/shadcn-luma/01-theming.md §5.26 (stock Luma: shadcn-svelte@1.7.0 add).
	import { cn, type WithElementRef } from "$lib/utils.js";
	import type { HTMLAttributes } from "svelte/elements";

	let {
		ref = $bindable(null),
		class: className,
		children,
		...restProps
	}: WithElementRef<HTMLAttributes<HTMLElement>> = $props();
</script>

<kbd
	bind:this={ref}
	data-slot="kbd"
	class={cn(
		// Menu-shortcut style (mac.ts menuShortcut): glyphs only, no keycap box.
		"pointer-events-none inline-flex w-fit items-center gap-0.5 font-sans text-xs tracking-normal text-muted-foreground select-none [&_svg:not([class*='size-'])]:size-3",
		className
	)}
	{...restProps}
>
	{@render children?.()}
</kbd>
