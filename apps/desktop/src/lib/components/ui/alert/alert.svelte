<script lang="ts" module>
	// underlane: macOS overrides, see .scratch/shadcn-luma/01-theming.md §5.24 (stock Luma: shadcn-svelte@1.7.0 add).
	import { tv, type VariantProps } from "$lib/utils.js"; // underlane: tv without its own merge (01 §5)

	export const alertVariants = tv({
		// Native inset: neutral surface, severity confined to the icon.
		base: "group/alert relative flex w-full items-start gap-3 rounded-lg bg-card p-3.5 text-left text-callout text-foreground leading-normal",
		variants: {
			variant: {
				default: "[&_[data-slot=alert-icon]]:text-muted-foreground",
				warning: "[&_[data-slot=alert-icon]]:text-degraded-text",
				destructive: "[&_[data-slot=alert-icon]]:text-bad-text",
				success: "[&_[data-slot=alert-icon]]:text-ok-text",
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
	import type { Snippet } from "svelte";

	let {
		ref = $bindable(null),
		class: className,
		variant = "default",
		children,
		icon,
		action,
		...restProps
	}: WithElementRef<HTMLAttributes<HTMLDivElement>> & {
		variant?: AlertVariant;
		icon?: Snippet;
		action?: Snippet;
	} = $props();
</script>

<div
	bind:this={ref}
	data-slot="alert"
	role="alert"
	class={cn(alertVariants({ variant }), className)}
	{...restProps}
>
	{#if icon}
		<span data-slot="alert-icon" aria-hidden="true" class="grid size-6.5 shrink-0 place-items-center rounded-md bg-control [&_svg]:size-4">
			{@render icon()}
		</span>
	{/if}
	<div class="flex min-w-0 flex-1 flex-wrap items-center gap-x-3 gap-y-2">
		<div class="min-w-[min(100%,180px)] flex-1 space-y-0.5 [overflow-wrap:anywhere]">{@render children?.()}</div>
		{#if action}<div data-slot="alert-action" class="shrink-0">{@render action()}</div>{/if}
	</div>
</div>
