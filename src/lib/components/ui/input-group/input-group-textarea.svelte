<script lang="ts">
	// burrow: macOS overrides, see .scratch/shadcn-luma/01-theming.md §5.25 (stock Luma: shadcn-svelte@1.7.0 add).
	import { Textarea } from "$lib/components/ui/textarea/index.js";
	import { cn } from "$lib/utils.js";
	import type { ComponentProps } from "svelte";

	let {
		ref = $bindable(null),
		value = $bindable(),
		class: className,
		...props
	}: ComponentProps<typeof Textarea> = $props();
</script>

<Textarea
	bind:ref
	data-slot="input-group-control"
	class={cn("flex-1 resize-none rounded-none border-0 bg-transparent py-1 shadow-none ring-0 focus-visible:ring-0 aria-invalid:ring-0", className)}
	bind:value
	{...props}
/>
