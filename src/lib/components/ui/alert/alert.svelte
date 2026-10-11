<script lang="ts" module>
	// burrow: macOS overrides, see .scratch/shadcn-luma/01-theming.md §5.24 (stock Luma: shadcn-svelte@1.7.0 add).
	import { tv, type VariantProps } from "$lib/utils.js"; // burrow: tv without its own merge (01 §5)

	export const alertVariants = tv({
		// Inline banner: tinted fill, no border, 12px text.
		base: "group/alert relative grid w-full gap-0.5 rounded-lg px-3 py-2 text-left text-callout has-data-[slot=alert-action]:pr-16 has-[>svg]:grid-cols-[auto_1fr] has-[>svg]:gap-x-1.5 *:[svg]:row-span-2 *:[svg]:translate-y-0.5 *:[svg]:text-current *:[svg:not([class*='size-'])]:size-3.5",
		variants: {
			variant: {
				default: "bg-fill text-foreground",
				warning: "bg-starting/12 text-starting-text",
				destructive: "bg-bad/10 text-bad-text",
			},
		},
		defaultVariants: {
			variant: "default",
		},
	});

	export type AlertVariant = VariantProps<typeof alertVariants>["variant"];
</script>

<script lang="ts">
	import { cn, type WithElementRef } from "$lib/utils.js";
	import type { HTMLAttributes } from "svelte/elements";

	let {
		ref = $bindable(null),
		class: className,
		variant = "default",
		children,
		...restProps
	}: WithElementRef<HTMLAttributes<HTMLDivElement>> & {
		variant?: AlertVariant;
	} = $props();
</script>

<div
	bind:this={ref}
	data-slot="alert"
	role="alert"
	class={cn(alertVariants({ variant }), className)}
	{...restProps}
>
	{@render children?.()}
</div>
