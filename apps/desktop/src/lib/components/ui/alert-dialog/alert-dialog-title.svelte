<script lang="ts">
	// underlane: macOS overrides, see .scratch/shadcn-luma/01-theming.md §5.20 (stock Luma: shadcn-svelte@1.7.0 add).
	import { AlertDialog as AlertDialogPrimitive } from "bits-ui";
	import { cn } from "$lib/utils.js";

	let {
		ref = $bindable(null),
		class: className,
		...restProps
	}: AlertDialogPrimitive.TitleProps = $props();
</script>

<AlertDialogPrimitive.Title
	bind:ref
	data-slot="alert-dialog-title"
	class={cn("text-sm font-semibold", className)}
	{...restProps}
/>
