<script lang="ts">
	// underlane: macOS overrides, see .scratch/shadcn-luma/01-theming.md §5 (stock Luma: commit 9f0aa7b).
	import { Switch as SwitchPrimitive } from "bits-ui";
	import { cn, type WithoutChildrenOrChild } from "$lib/utils.js";
	import { kbRing } from "../mac.js";

	let {
		ref = $bindable(null),
		class: className,
		checked = $bindable(false),
		size = "default",
		...restProps
	}: WithoutChildrenOrChild<SwitchPrimitive.RootProps> & {
		size?: "sm" | "default";
	} = $props();
</script>

<SwitchPrimitive.Root
	bind:ref
	bind:checked
	data-slot="switch"
	data-size={size}
	class={cn(
			"peer group/switch relative inline-flex shrink-0 items-center rounded-full p-[2px] transition-colors duration-150 data-checked:bg-tint data-unchecked:bg-fill-strong before:pointer-events-none before:absolute before:inset-0 before:rounded-full before:border-[0.5px] before:border-transparent data-unchecked:before:border-border data-[size=default]:h-[18px] data-[size=default]:w-[30px] data-[size=sm]:h-[14px] data-[size=sm]:w-[24px] data-disabled:opacity-40 aria-invalid:ring-[3px] aria-invalid:ring-destructive/40 after:absolute after:-inset-x-2 after:-inset-y-1",
			kbRing,
			className
	)}
	{...restProps}
>
	<SwitchPrimitive.Thumb
		data-slot="switch-thumb"
		class={cn(
			"pointer-events-none block rounded-full bg-white border-[0.5px] border-black/15 transition-transform duration-200 ease-spring group-data-[size=default]/switch:size-[14px] group-data-[size=sm]/switch:size-[10px] data-checked:translate-x-3 group-data-[size=sm]/switch:data-checked:translate-x-[10px] rtl:data-checked:-translate-x-3"
		)}
	/>
</SwitchPrimitive.Root>
