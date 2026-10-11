<script lang="ts" module>
	// burrow: macOS overrides, see .scratch/shadcn-luma/01-theming.md §5.25 (stock Luma: shadcn-svelte@1.7.0 add).
	import { tv, type VariantProps } from "$lib/utils.js"; // burrow: tv without its own merge (01 §5)
	export const inputGroupAddonVariants = tv({
		// Unit/icon addon: 12px muted text, tight padding inside a 22px field.
		base: "flex h-auto cursor-text items-center justify-center gap-1 text-callout text-muted-foreground select-none group-data-[disabled=true]/input-group:opacity-45 [&>svg:not([class*='size-'])]:size-3.5",
		variants: {
			align: {
				"inline-start": "order-first ps-1.5 has-[>button]:ps-0.5",
				"inline-end": "order-last pe-1.5 has-[>button]:pe-0.5",
				"block-start": "order-first w-full justify-start px-2 pt-1.5",
				"block-end": "order-last w-full justify-start px-2 pb-1.5",
			},
		},
		defaultVariants: {
			align: "inline-start",
		},
	});

	export type InputGroupAddonAlign = VariantProps<typeof inputGroupAddonVariants>["align"];
</script>

<script lang="ts">
	import { cn, type WithElementRef } from "$lib/utils.js";
	import type { HTMLAttributes } from "svelte/elements";

	let {
		ref = $bindable(null),
		class: className,
		children,
		align = "inline-start",
		...restProps
	}: WithElementRef<HTMLAttributes<HTMLDivElement>> & {
		align?: InputGroupAddonAlign;
	} = $props();
</script>

<div
	bind:this={ref}
	role="group"
	data-slot="input-group-addon"
	data-align={align}
	class={cn(inputGroupAddonVariants({ align }), className)}
	onclick={(e) => {
		if ((e.target as HTMLElement).closest("button")) {
			return;
		}
		e.currentTarget.parentElement?.querySelector("input")?.focus();
	}}
	{...restProps}
>
	{@render children?.()}
</div>
