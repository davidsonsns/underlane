<script lang="ts" module>
	// burrow: macOS overrides, see .scratch/shadcn-luma/01-theming.md §5.22 (stock Luma: shadcn-svelte@1.7.0 add).
	import { tv, type VariantProps } from "$lib/utils.js"; // burrow: tv without its own merge (01 §5)

	export const badgeVariants = tv({
		// Small label: 16px tall, 4px corners, 10px medium text, no border, no hover.
		base: "group/badge inline-flex h-4 w-fit shrink-0 items-center justify-center gap-1 overflow-hidden rounded-[4px] px-1.5 text-2xs font-medium whitespace-nowrap select-none [&>svg]:pointer-events-none [&>svg]:size-2.5!",
		variants: {
			variant: {
				// Method/kind pills (RequestDetail).
				default: "bg-fill text-muted-foreground",
				// Sidebar running count.
				ok: "bg-ok/15 text-ok-text",
				// "Truncated" and other cautions.
				warning: "bg-starting/15 text-starting-text",
				destructive: "bg-bad/10 text-bad-text",
				outline: "text-muted-foreground border-[0.5px] border-input",
			},
		},
		defaultVariants: {
			variant: "default",
		},
	});

	export type BadgeVariant = VariantProps<typeof badgeVariants>["variant"];
</script>

<script lang="ts">
	import { cn, type WithElementRef } from "$lib/utils.js";
	import type { HTMLAnchorAttributes } from "svelte/elements";

	let {
		ref = $bindable(null),
		href,
		class: className,
		variant = "default",
		children,
		...restProps
	}: WithElementRef<HTMLAnchorAttributes> & {
		variant?: BadgeVariant;
	} = $props();
</script>

<svelte:element
	this={href ? "a" : "span"}
	bind:this={ref}
	data-slot="badge"
	{href}
	class={cn(badgeVariants({ variant }), className)}
	{...restProps}
>
	{@render children?.()}
</svelte:element>
