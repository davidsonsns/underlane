<script lang="ts">
	// burrow: macOS overrides, see .scratch/shadcn-luma/01-theming.md §5 (stock Luma: commit 9f0aa7b).
	import { cn, type WithElementRef } from "$lib/utils.js";
	import type { HTMLInputAttributes, HTMLInputTypeAttribute } from "svelte/elements";

	type InputType = Exclude<HTMLInputTypeAttribute, "file">;

	type Props = WithElementRef<
		Omit<HTMLInputAttributes, "type"> &
			({ type: "file"; files?: FileList } | { type?: InputType; files?: undefined })
	>;

	let {
		ref = $bindable(null),
		value = $bindable(),
		type,
		files = $bindable(),
		class: className,
		"data-slot": dataSlot = "input",
		...restProps
	}: Props = $props();
</script>

{#if type === "file"}
	<input
		bind:this={ref}
		data-slot={dataSlot}
		class={cn(
			"h-[22px] w-full min-w-0 rounded-[5px] bg-control px-2 text-sm text-foreground shadow-[inset_0_0_0_0.5px_var(--line-strong)] transition-[box-shadow] duration-100 outline-none placeholder:text-faint focus-visible:ring-[3px] focus-visible:ring-ring/40 disabled:pointer-events-none disabled:opacity-45 aria-invalid:ring-[3px] aria-invalid:ring-destructive/40 file:inline-flex file:h-full file:border-0 file:bg-transparent file:text-sm file:text-foreground",
			className
		)}
		type="file"
		bind:files
		bind:value
		{...restProps}
	/>
{:else}
	<input
		bind:this={ref}
		data-slot={dataSlot}
		class={cn(
			"h-[22px] w-full min-w-0 rounded-[5px] bg-control px-2 text-sm text-foreground shadow-[inset_0_0_0_0.5px_var(--line-strong)] transition-[box-shadow] duration-100 outline-none placeholder:text-faint focus-visible:ring-[3px] focus-visible:ring-ring/40 disabled:pointer-events-none disabled:opacity-45 aria-invalid:ring-[3px] aria-invalid:ring-destructive/40 file:inline-flex file:h-full file:border-0 file:bg-transparent file:text-sm file:text-foreground",
			className
		)}
		{type}
		bind:value
		{...restProps}
	/>
{/if}
