<script lang="ts">
	// underlane: macOS overrides, see .scratch/shadcn-luma/01-theming.md §5 (stock Luma: commit 9f0aa7b).
	import { Select as SelectPrimitive } from "bits-ui";
	import ChevronsUpDownIcon from '@lucide/svelte/icons/chevrons-up-down';
	import { cn, type WithoutChild } from "$lib/utils.js";
	import { kbRing } from "../mac.js";

	let {
		ref = $bindable(null),
		class: className,
		children,
		size = "default",
		onpointerdown,
		onmousedown,
		...restProps
	}: WithoutChild<SelectPrimitive.TriggerProps> & {
		size?: "sm" | "default";
	} = $props();
</script>

<SelectPrimitive.Trigger
	bind:ref
	data-slot="select-trigger"
	data-size={size}
	class={cn(
		// macOS pop-up button, drawn like the Combobox trigger (an outline Button): 22px, value
		// then a plain up/down chevron (the macOS 26 look, 01 §5.2).
		"flex w-fit min-w-0 items-center justify-between gap-2 rounded-md border-[0.5px] border-input bg-control px-2.5 text-start text-sm whitespace-nowrap text-foreground select-none active:brightness-[0.94] dark:active:brightness-[1.25] disabled:cursor-default disabled:opacity-45 data-placeholder:text-faint data-[size=default]:h-[22px] data-[size=sm]:h-[18px] data-[size=sm]:text-xs *:data-[slot=select-value]:line-clamp-1 *:data-[slot=select-value]:flex *:data-[slot=select-value]:items-center *:data-[slot=select-value]:gap-1.5 aria-invalid:ring-[3px] aria-invalid:ring-destructive/40 [&_svg]:pointer-events-none [&_svg]:shrink-0",
		kbRing,
		className
	)}
	onpointerdown={(e) => {
		// WebKit never focuses a <button> on click (and its mousedown default action blurs
		// whatever had focus), so a mouse-opened select left focus on <body> and ignored the
		// arrow keys. Focus the trigger and keep WebKit's mousedown from taking it away.
		e.currentTarget.focus({ preventScroll: true });
		onpointerdown?.(e);
	}}
	onmousedown={(e) => {
		e.preventDefault();
		onmousedown?.(e);
	}}
	{...restProps}
>
	<span class="min-w-0 flex-1 truncate">{@render children?.()}</span>
	<ChevronsUpDownIcon class="size-3 text-muted-foreground" aria-hidden="true" />
</SelectPrimitive.Trigger>
