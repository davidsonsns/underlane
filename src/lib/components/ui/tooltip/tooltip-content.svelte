<!-- Tip renders this component after importing the shared lazy chunk. -->
<!-- fallow-ignore-file unrendered-component -->
<script lang="ts">
	// underlane: macOS overrides, see .scratch/shadcn-luma/01-theming.md §5 (stock Luma: commit 9f0aa7b).
	import { Tooltip as TooltipPrimitive } from "bits-ui";
	import { cn } from "$lib/utils.js";
	import type { WithoutChildrenOrChild } from "$lib/utils.js";
	import TooltipPortal from "./tooltip-portal.svelte";
	import type { ComponentProps } from "svelte";

	let {
		ref = $bindable(null),
		class: className,
		sideOffset = 6,
		side = "bottom",
		children,
		arrowClasses,
		portalProps,
		...restProps
	}: TooltipPrimitive.ContentProps & {
		arrowClasses?: string;
		portalProps?: WithoutChildrenOrChild<ComponentProps<typeof TooltipPortal>>;
	} = $props();
</script>

<TooltipPortal {...portalProps}>
	<TooltipPrimitive.Content
		bind:ref
		data-slot="tooltip-content"
		{sideOffset}
		{side}
		class={cn(
			"z-50 w-fit max-w-xs origin-(--bits-tooltip-content-transform-origin) rounded-[5px] bg-background px-1.5 py-0.5 text-xs text-foreground border-[0.5px] border-input data-open:animate-in data-open:fade-in-0 data-open:duration-100 data-[state=delayed-open]:animate-in data-[state=delayed-open]:fade-in-0 data-[state=delayed-open]:duration-100 data-closed:animate-out data-closed:fade-out-0 data-closed:duration-100",
			className
		)}
		{...restProps}
	>
		{@render children?.()}
	</TooltipPrimitive.Content>
</TooltipPortal>
