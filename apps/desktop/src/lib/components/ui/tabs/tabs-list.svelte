<script lang="ts" module>
	// underlane: macOS overrides, see .scratch/shadcn-luma/01-theming.md §5 (stock Luma: commit 9f0aa7b).
	import { tv, type VariantProps } from "$lib/utils.js"; // underlane: tv without its own merge (01 §5)

	export const tabsListVariants = tv({
		base: "group/tabs-list relative isolate inline-flex w-fit items-center justify-center text-sm group-data-horizontal/tabs:h-[22px] group-data-vertical/tabs:h-fit group-data-vertical/tabs:flex-col data-[variant=line]:rounded-none",
		variants: {
			variant: {
				default: "gap-px rounded-md bg-fill p-px border-[0.5px] border-border",
				line: "gap-1 bg-transparent",
			},
		},
		defaultVariants: {
			variant: "default",
		},
	});

	export type TabsListVariant = VariantProps<typeof tabsListVariants>["variant"];
</script>

<script lang="ts">
	import { Tabs as TabsPrimitive } from "bits-ui";
	import { cn } from "$lib/utils.js";
	import SelectionThumb from "../SelectionThumb.svelte";

	let {
		ref = $bindable(null),
		variant = "default",
		class: className,
		children,
		...restProps
	}: TabsPrimitive.ListProps & {
		variant?: TabsListVariant;
	} = $props();

	// underlane: as in ui/toggle-group, the active tab's background is one thumb that slides to the
	// new tab (NSSegmentedControl). It follows data-state, re-measures on resize, and the first
	// placement doesn't animate. The line variant keeps its underline instead.
	let thumb = $state<{ x: number; y: number; w: number; h: number } | null>(null);
	let animate = $state(false);

	$effect(() => {
		if (variant !== "default" || !ref) return;
		const root = ref;
		const measure = () => {
			const on = root.querySelector<HTMLElement>('[data-slot="tabs-trigger"][data-state="active"]');
			thumb = on ? { x: on.offsetLeft, y: on.offsetTop, w: on.offsetWidth, h: on.offsetHeight } : null;
		};
		measure();
		const frame = requestAnimationFrame(() => (animate = true));
		const mo = new MutationObserver(measure);
		mo.observe(root, { subtree: true, attributes: true, attributeFilter: ["data-state"] });
		const ro = new ResizeObserver(measure);
		ro.observe(root);
		return () => {
			cancelAnimationFrame(frame);
			mo.disconnect();
			ro.disconnect();
			animate = false;
		};
	});
</script>

<TabsPrimitive.List
	bind:ref
	data-slot="tabs-list"
	data-variant={variant}
	class={cn(tabsListVariants({ variant }), className)}
	{...restProps}
>
	{#if variant === "default" && thumb}
		<SelectionThumb {thumb} {animate} />
	{/if}
	{@render children?.()}
</TabsPrimitive.List>
