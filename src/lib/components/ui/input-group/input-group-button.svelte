<script lang="ts" module>
	// burrow: macOS overrides, see .scratch/shadcn-luma/01-theming.md §5.25 (stock Luma: shadcn-svelte@1.7.0 add).
	import { tv, type VariantProps } from "$lib/utils.js"; // burrow: tv without its own merge (01 §5)

	const inputGroupButtonVariants = tv({
		// Borderless buttons sized to fit inside the 22px field.
		base: "flex items-center gap-1 rounded-[4px] text-sm shadow-none",
		variants: {
			size: {
				xs: "h-4 px-1 text-xs [&>svg:not([class*='size-'])]:size-3",
				sm: "h-[18px] px-1.5 [&>svg:not([class*='size-'])]:size-3.5",
				"icon-xs": "size-4 p-0 has-[>svg]:p-0 [&>svg:not([class*='size-'])]:size-3",
				"icon-sm": "size-[18px] p-0 has-[>svg]:p-0 [&>svg:not([class*='size-'])]:size-3.5",
			},
		},
		defaultVariants: {
			size: "xs",
		},
	});

	export type InputGroupButtonSize = VariantProps<typeof inputGroupButtonVariants>["size"];
</script>

<script lang="ts">
	import { Button } from "$lib/components/ui/button/index.js";
	import { cn } from "$lib/utils.js";
	import type { ComponentProps } from "svelte";

	let {
		ref = $bindable(null),
		class: className,
		children,
		type = "button",
		variant = "ghost",
		size = "xs",
		...restProps
	}: Omit<ComponentProps<typeof Button>, "href" | "size"> & {
		size?: InputGroupButtonSize;
	} = $props();
</script>

<Button
	bind:ref
	{type}
	data-size={size}
	{variant}
	class={cn(inputGroupButtonVariants({ size }), className)}
	{...restProps}
>
	{@render children?.()}
</Button>
