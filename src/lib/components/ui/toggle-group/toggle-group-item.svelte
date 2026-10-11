<script lang="ts">
	// burrow: macOS overrides, see .scratch/shadcn-luma/01-theming.md §5 (stock Luma: commit 9f0aa7b).
	import { ToggleGroup as ToggleGroupPrimitive } from "bits-ui";
	import { type ToggleVariants, toggleVariants } from "$lib/components/ui/toggle/index.js";
	import { cn } from "$lib/utils.js";
	import { getToggleGroupCtx } from "./toggle-group.svelte";

	let {
		ref = $bindable(null),
		value = $bindable(),
		class: className,
		size,
		variant,
		...restProps
	}: ToggleGroupPrimitive.ItemProps & ToggleVariants = $props();

	const ctx = getToggleGroupCtx();
</script>

<ToggleGroupPrimitive.Item
	bind:ref
	data-slot="toggle-group-item"
	data-variant={ctx.variant || variant}
	data-size={ctx.size || size}
	data-spacing={ctx.spacing}
	class={cn(
		"shrink-0 focus-visible:z-10 group-data-[spacing=0]/toggle-group:h-full group-data-[spacing=0]/toggle-group:rounded-[5px] group-data-[spacing=0]/toggle-group:px-2.5 group-data-[spacing=0]/toggle-group:hover:bg-transparent group-data-[spacing=0]/toggle-group:data-[state=on]:bg-segment group-data-[spacing=0]/toggle-group:data-[state=on]:shadow-segment",
		toggleVariants({
			variant: ctx.variant || variant,
			size: ctx.size || size,
		}),
		className
	)}
	{value}
	{...restProps}
/>
