<script lang="ts">
	// underlane: macOS overrides, see .scratch/shadcn-luma/01-theming.md §5 (stock Luma: commit 9f0aa7b).
	// underlane: placeholder:text-placeholder (>= 4.5:1, 03-review R-48); text-faint is for disabled text only.
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
			"h-[22px] w-full min-w-0 rounded-[5px] bg-control px-2 text-sm text-foreground border-[0.5px] border-input focus-visible:border-ring transition-[box-shadow] duration-100 outline-none placeholder:text-placeholder disabled:pointer-events-none disabled:opacity-45 aria-invalid:ring-[3px] aria-invalid:ring-destructive/40 file:inline-flex file:h-full file:border-0 file:bg-transparent file:text-sm file:text-foreground",
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
			"h-[22px] w-full min-w-0 rounded-[5px] bg-control px-2 text-sm text-foreground border-[0.5px] border-input focus-visible:border-ring transition-[box-shadow] duration-100 outline-none placeholder:text-placeholder disabled:pointer-events-none disabled:opacity-45 aria-invalid:ring-[3px] aria-invalid:ring-destructive/40 file:inline-flex file:h-full file:border-0 file:bg-transparent file:text-sm file:text-foreground",
			className
		)}
		{type}
		bind:value
		{...restProps}
	/>
{/if}
