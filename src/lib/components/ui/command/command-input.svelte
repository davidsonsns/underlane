<script lang="ts">
	// burrow: macOS overrides, see .scratch/shadcn-luma/01-theming.md §5.18 (stock Luma: shadcn-svelte@1.7.0 add; the spike tuned only command and command-item).
	// burrow: the input keeps data-slot="input-group-control" (stock: "command-input"), like the
	// other input-group controls.
	import { Command as CommandPrimitive } from "bits-ui";
	import * as InputGroup from "$lib/components/ui/input-group/index.js";
	import SearchIcon from '@lucide/svelte/icons/search';
	import { cn } from "$lib/utils.js";

	let {
		ref = $bindable(null),
		class: className,
		value = $bindable(""),
		...restProps
	}: CommandPrimitive.InputProps = $props();
</script>

<div data-slot="command-input-wrapper" class="pb-[5px]">
	<InputGroup.Root>
		<CommandPrimitive.Input
			{value}
			data-slot="input-group-control"
			class={cn(
				"w-full text-sm outline-hidden disabled:cursor-default disabled:opacity-45",
				className
			)}
			{...restProps}
		>
			{#snippet child({ props })}
				<InputGroup.Input {...props} bind:value bind:ref />
			{/snippet}
		</CommandPrimitive.Input>
		<InputGroup.Addon>
			<SearchIcon class="size-3.5 shrink-0" />
		</InputGroup.Addon>
	</InputGroup.Root>
</div>
