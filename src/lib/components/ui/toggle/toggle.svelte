<script lang="ts" module>
	// burrow: macOS overrides, see .scratch/shadcn-luma/01-theming.md §5 (stock Luma: commit 9f0aa7b).
	import { type VariantProps, tv } from "tailwind-variants";

	export const toggleVariants = tv({
		base: "group/toggle inline-flex items-center justify-center gap-1 rounded-md text-sm font-normal whitespace-nowrap text-foreground outline-none select-none kb-focus:ring-[3px] kb-focus:ring-ring/50 hover:bg-fill aria-pressed:bg-fill-strong data-[state=on]:bg-fill-strong disabled:pointer-events-none disabled:opacity-45 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-[15px]",
		variants: {
			variant: {
				default: "bg-transparent",
				outline: "bg-control shadow-control hover:bg-control",
			},
			size: {
				default: "h-[22px] min-w-[22px] px-2",
				sm: "h-[18px] min-w-[18px] px-1.5 text-xs",
				lg: "h-6 min-w-6 px-2.5",
			},
		},
		defaultVariants: {
			variant: "default",
			size: "default",
		},
	});

	export type ToggleVariant = VariantProps<typeof toggleVariants>["variant"];
	export type ToggleSize = VariantProps<typeof toggleVariants>["size"];
	export type ToggleVariants = VariantProps<typeof toggleVariants>;
</script>

<script lang="ts">
	import { Toggle as TogglePrimitive } from "bits-ui";
	import { cn } from "$lib/utils.js";

	let {
		ref = $bindable(null),
		pressed = $bindable(false),
		class: className,
		size = "default",
		variant = "default",
		...restProps
	}: TogglePrimitive.RootProps & {
		variant?: ToggleVariant;
		size?: ToggleSize;
	} = $props();
</script>

<TogglePrimitive.Root
	bind:ref
	bind:pressed
	data-slot="toggle"
	class={cn(toggleVariants({ variant, size }), className)}
	{...restProps}
/>
