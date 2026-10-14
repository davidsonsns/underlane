<script lang="ts">
	// underlane: macOS overrides, see .scratch/shadcn-luma/01-theming.md §5.23 (stock Luma: shadcn-svelte@1.7.0 add).
	import Loader2Icon from '@lucide/svelte/icons/loader-2';
	import { cn } from "$lib/utils.js";
	import type { ComponentProps } from "svelte";

	// underlane: stock destructures role, aria-label, name, color, stroke and className one by one.
	// Typed as the icon's own props, everything passes through and a caller's role/aria-label
	// still wins; unset props aren't forwarded, so `stroke: undefined` can't override
	// currentColor. Less code, since StatePanel puts this in the popover's chunk.
	let { class: className, ...restProps }: ComponentProps<typeof Loader2Icon> = $props();
</script>

<Loader2Icon role="status" aria-label="Loading" {...restProps} class={cn("size-4 animate-spin text-muted-foreground motion-reduce:animate-none", className)} />
