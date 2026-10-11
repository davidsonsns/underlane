<script lang="ts">
	// burrow: macOS overrides, see .scratch/shadcn-luma/01-theming.md §5.20 (stock Luma: shadcn-svelte@1.7.0 add).
	import { AlertDialog as AlertDialogPrimitive } from "bits-ui";
	import { cn, type WithoutChild, type WithoutChildrenOrChild } from "$lib/utils.js";
	import AlertDialogOverlay from "./alert-dialog-overlay.svelte";
	import AlertDialogPortal from "./alert-dialog-portal.svelte";
	import type { ComponentProps } from "svelte";

	let {
		ref = $bindable(null),
		class: className,
		size = "default",
		portalProps,
		onOpenAutoFocus,
		...restProps
	}: WithoutChild<AlertDialogPrimitive.ContentProps> & {
		size?: "default" | "sm";
		portalProps?: WithoutChildrenOrChild<ComponentProps<typeof AlertDialogPortal>>;
	} = $props();

	// burrow: open with focus on Cancel (Bits focuses the content itself). WebKit's plain Tab skips
	// buttons, and Return must never hit the destructive action by default. Deferred like Bits'
	// own initial focus, so the content is in the DOM.
	function focusCancel(e: Event) {
		onOpenAutoFocus?.(e);
		if (e.defaultPrevented) return;
		e.preventDefault();
		setTimeout(() => {
			const cancel = ref?.querySelector<HTMLElement>("[data-slot=alert-dialog-cancel]");
			(cancel ?? ref)?.focus({ preventScroll: true });
		});
	}
</script>

<AlertDialogPortal {...portalProps}>
	<AlertDialogOverlay />
	<AlertDialogPrimitive.Content
		bind:ref
		data-slot="alert-dialog-content"
		data-size={size}
		onOpenAutoFocus={focusCancel}
		class={cn(
			// NSAlert presented as a window sheet: the same geometry and motion as the dialog sheet
			// (§5.6), narrower. No icon, no close button; Cancel is the Escape action.
			"group/alert-dialog-content fixed top-(--sheet-top) left-1/2 z-50 grid max-h-[calc(100vh-var(--sheet-top)-24px)] -translate-x-1/2 gap-4 overflow-auto rounded-xl bg-background p-5 text-sm text-foreground shadow-sheet outline-none data-[size=default]:w-[min(420px,calc(100vw-48px))] data-[size=sm]:w-[min(320px,calc(100vw-48px))]",
			"data-open:animate-in data-open:fade-in-0 data-open:slide-in-from-top-4 data-open:duration-200 data-open:ease-out data-closed:animate-out data-closed:fade-out-0 data-closed:slide-out-to-top-2 data-closed:duration-150",
			className
		)}
		{...restProps}
	/>
</AlertDialogPortal>
