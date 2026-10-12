<script lang="ts" module>
	// underlane: macOS overrides, see .scratch/shadcn-luma/01-theming.md §5 (stock Luma: commit 9f0aa7b).
	import { tv, type VariantProps } from "$lib/utils.js"; // underlane: tv without its own merge (01 §5)
	import { cn, type WithElementRef } from "$lib/utils.js";
	import type { HTMLAnchorAttributes, HTMLButtonAttributes } from "svelte/elements";

	export const buttonVariants = tv({
		// macOS push button: 22px, 13px regular, 6px corners, no hover state (AppKit has none),
		// darkens while pressed, ring only for keyboard navigation.
		base: "group/button inline-flex shrink-0 items-center justify-center gap-1.5 rounded-md text-sm font-normal whitespace-nowrap select-none outline-none transition-[box-shadow] duration-100 kb-focus:ring-[3px] kb-focus:ring-ring/50 kb-focus:aria-expanded:ring-0 active:brightness-[0.94] dark:active:brightness-[1.25] disabled:pointer-events-none disabled:opacity-45 aria-invalid:ring-[3px] aria-invalid:ring-destructive/40 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-[15px]",
		variants: {
			variant: {
				// Default (Return) button: accent fill, white text.
				default: "bg-primary text-primary-foreground disabled:bg-control disabled:text-foreground disabled:border-[0.5px] disabled:border-input",
				// Regular push button.
				outline: "bg-control text-foreground border-[0.5px] border-input",
				secondary: "bg-control text-foreground border-[0.5px] border-input",
				// Borderless/toolbar button: hover fill is the one place macOS shows hover.
				ghost: "bg-transparent text-muted-foreground hover:bg-fill hover:text-foreground aria-expanded:bg-fill aria-expanded:text-foreground",
				destructive: "bg-control text-destructive border-[0.5px] border-input",
				link: "h-auto! px-0! text-link underline-offset-2 hover:underline",
			},
			size: {
				default: "h-[22px] px-2.5",
				xs: "h-4 px-1.5 text-xs [&_svg:not([class*='size-'])]:size-3",
				sm: "h-[18px] px-2 text-xs [&_svg:not([class*='size-'])]:size-3",
				lg: "h-6 px-3",
				icon: "size-6",
				"icon-xs": "size-4 [&_svg:not([class*='size-'])]:size-3",
				"icon-sm": "size-[18px] [&_svg:not([class*='size-'])]:size-3",
				"icon-lg": "size-7",
			},
		},
		defaultVariants: {
			variant: "default",
			size: "default",
		},
	});

	export type ButtonVariant = VariantProps<typeof buttonVariants>["variant"];
	export type ButtonSize = VariantProps<typeof buttonVariants>["size"];

	export type ButtonProps = WithElementRef<HTMLButtonAttributes> &
		WithElementRef<HTMLAnchorAttributes> & {
			variant?: ButtonVariant;
			size?: ButtonSize;
		};
</script>

<script lang="ts">
	let {
		class: className,
		variant = "default",
		size = "default",
		ref = $bindable(null),
		href = undefined,
		type = "button",
		disabled,
		children,
		...restProps
	}: ButtonProps = $props();
</script>

{#if href}
	<a
		bind:this={ref}
		data-slot="button"
		class={cn(buttonVariants({ variant, size }), className)}
		href={disabled ? undefined : href}
		aria-disabled={disabled}
		role={disabled ? "link" : undefined}
		tabindex={disabled ? -1 : undefined}
		{...restProps}
	>
		{@render children?.()}
	</a>
{:else}
	<button
		bind:this={ref}
		data-slot="button"
		class={cn(buttonVariants({ variant, size }), className)}
		{type}
		{disabled}
		{...restProps}
	>
		{@render children?.()}
	</button>
{/if}
