<script lang="ts">
	// burrow: macOS overrides, see .scratch/shadcn-luma/01-theming.md §5.19 (stock Luma: shadcn-svelte@1.7.0 add).
	import { Popover as PopoverPrimitive } from "bits-ui";
	import { cn, type WithoutChildrenOrChild } from "$lib/utils.js";
	import PopoverPortal from "./popover-portal.svelte";
	import { menuContent } from "../mac.js";
	import type { ComponentProps } from "svelte";

	let {
		ref = $bindable(null),
		class: className,
		sideOffset = 3,
		align = "start",
		portalProps,
		...restProps
	}: PopoverPrimitive.ContentProps & {
		portalProps?: WithoutChildrenOrChild<ComponentProps<typeof PopoverPortal>>;
	} = $props();
</script>

<PopoverPortal {...portalProps}>
	<PopoverPrimitive.Content
		bind:ref
		data-slot="popover-content"
		{sideOffset}
		{align}
		class={cn(
			// The menu material (mac.ts): opens instantly like an NSMenu, 150ms fade out, no zoom.
			menuContent,
			"flex w-72 flex-col gap-2 origin-(--bits-popover-content-transform-origin)",
			className
		)}
		{...restProps}
	/>
</PopoverPortal>
