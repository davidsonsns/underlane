<script lang="ts">
	// burrow: macOS overrides, see .scratch/shadcn-luma/01-theming.md §5.23 (stock Luma: shadcn-svelte@1.7.0 add).
	import Loader2Icon from '@lucide/svelte/icons/loader-2';
	import { cn } from "$lib/utils.js";
	import type { SVGAttributes } from "svelte/elements";

	let {
		class: className,
		role = "status",
		// we add name, color, and stroke for compatibility with different icon libraries props.
		// only forward them when set, otherwise `stroke: undefined` overrides the icon's `currentColor`
		name,
		color,
		stroke,
		className: legacyClassName,
		"aria-label": ariaLabel = "Loading",
		...restProps
	}: SVGAttributes<SVGSVGElement> = $props();
</script>

<Loader2Icon {role} {...name != null ? { name } : {}} {...color != null ? { color } : {}} {...stroke != null ? { stroke } : {}} {...legacyClassName != null ? { className: legacyClassName } : {}} aria-label={ariaLabel} class={cn("size-4 animate-spin text-muted-foreground motion-reduce:animate-none", className)} {...restProps} />
