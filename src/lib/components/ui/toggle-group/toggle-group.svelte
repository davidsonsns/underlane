<script lang="ts" module>
	// underlane: macOS overrides, see .scratch/shadcn-luma/01-theming.md §5 (stock Luma: commit 9f0aa7b).
	import { getContext, setContext } from "svelte";
	import { toggleVariants } from "$lib/components/ui/toggle/index.js";
	import type { VariantProps } from "$lib/utils.js";

	type ToggleVariants = VariantProps<typeof toggleVariants>;

	interface ToggleGroupContext extends ToggleVariants {
		spacing?: number;
		orientation?: "horizontal" | "vertical";
		/** underlane: single-select segmented group; the selection is a sliding thumb, not item backgrounds. */
		sliding?: boolean;
	}

	export function setToggleGroupCtx(props: ToggleGroupContext) {
		setContext("toggleGroup", props);
	}

	export function getToggleGroupCtx() {
		return getContext<Required<ToggleGroupContext>>("toggleGroup");
	}
</script>

<script lang="ts">
	import { ToggleGroup as ToggleGroupPrimitive } from "bits-ui";
	import { cn } from "$lib/utils.js";
	import SelectionThumb from "../SelectionThumb.svelte";

	let {
		ref = $bindable(null),
		value = $bindable(),
		class: className,
		size = "default",
		spacing = 0,
		orientation = "horizontal",
		variant = "default",
		children,
		...restProps
	}: ToggleGroupPrimitive.RootProps &
		ToggleVariants & {
			spacing?: number;
			orientation?: "horizontal" | "vertical";
		} = $props();

	const sliding = $derived(restProps.type === "single" && spacing === 0);

	setToggleGroupCtx({
		get variant() {
			return variant;
		},
		get size() {
			return size;
		},
		get spacing() {
			return spacing;
		},
		get orientation() {
			return orientation;
		},
		get sliding() {
			return sliding;
		},
	});

	// underlane: the selected segment's background is one thumb that slides to the new item
	// (NSSegmentedControl). It follows data-state rather than `value`, so it tracks any change
	// bits makes, and re-measures on resize. The first placement doesn't animate.
	let thumb = $state<{ x: number; y: number; w: number; h: number } | null>(null);
	let animate = $state(false);

	$effect(() => {
		if (!sliding || !ref) return;
		const root = ref;
		const measure = () => {
			const on = root.querySelector<HTMLElement>('[data-slot="toggle-group-item"][data-state="on"]');
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

<!--
Discriminated Unions + Destructing (required for bindable) do not
get along, so we shut typescript up by casting `value` to `never`.
-->
<ToggleGroupPrimitive.Root
	bind:value={value as never}
	bind:ref
	{orientation}
	data-slot="toggle-group"
	data-variant={variant}
	data-size={size}
	data-spacing={spacing}
	style={`--gap: ${spacing}`}
	class={cn(
		"group/toggle-group relative isolate flex w-fit flex-row items-center gap-[--spacing(var(--gap))] data-vertical:flex-col data-vertical:items-stretch data-[spacing=0]:h-7 data-[spacing=0]:gap-px data-[spacing=0]:rounded-md data-[spacing=0]:bg-fill data-[spacing=0]:p-px data-[spacing=0]:border-[0.5px] data-[spacing=0]:border-border",
		className
	)}
	{...restProps}
>
	{#if sliding && thumb}
		<SelectionThumb {thumb} {animate} />
	{/if}
	{@render children?.()}
</ToggleGroupPrimitive.Root>
