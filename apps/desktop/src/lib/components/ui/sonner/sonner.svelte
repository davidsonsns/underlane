<script lang="ts">
	// underlane: macOS overrides, see .scratch/shadcn-luma/01-theming.md §5 (stock Luma: commit 9f0aa7b).
	// Underlane: no theme-watcher package (it fights data-theme). The caller passes settings.theme ("system" | "light" | "dark");
	// "system" makes sonner follow prefers-color-scheme, which Rust's set_theme keeps in sync.
	// Native inset confirmations: opaque surface, quiet border and a small shadow.
	import { Toaster as Sonner, type ToasterProps as SonnerProps } from "svelte-sonner";
	import Loader2Icon from "@lucide/svelte/icons/loader-2";
	import CircleCheckIcon from "@lucide/svelte/icons/circle-check";
	import OctagonXIcon from "@lucide/svelte/icons/octagon-x";
	import InfoIcon from "@lucide/svelte/icons/info";
	import TriangleAlertIcon from "@lucide/svelte/icons/triangle-alert";

	let { theme = "system", ...restProps }: SonnerProps = $props();
</script>

<Sonner
	{theme}
	class="toaster group"
	style="--normal-bg: var(--control); --normal-text: var(--foreground); --normal-border: var(--input); --border-radius: var(--radius-lg); --width: 320px;"
	toastOptions={{
		classes: {
			toast: "!font-sans !text-sm !border !shadow-[0_3px_10px_color-mix(in_srgb,var(--foreground)_8%,transparent)] !gap-2.5 !px-3.5 !py-3",
			title: "!text-sm !font-semibold",
			description: "!text-callout !text-muted-foreground",
			success: "[&_[data-icon]]:!text-ok-text",
			error: "[&_[data-icon]]:!text-bad-text",
			warning: "[&_[data-icon]]:!text-degraded-text",
			actionButton: "!h-7 !rounded-md !bg-control !px-2.5 !text-sm !font-normal !text-foreground !border !border-solid !border-input",
			cancelButton: "!h-7 !rounded-md !bg-control !px-2.5 !text-sm !font-normal !text-foreground !border !border-solid !border-input",
		},
	}}
	{...restProps}
>
	{#snippet loadingIcon()}
		<Loader2Icon class="size-4 animate-spin" />
	{/snippet}
	{#snippet successIcon()}
		<CircleCheckIcon class="size-4" />
	{/snippet}
	{#snippet errorIcon()}
		<OctagonXIcon class="size-4" />
	{/snippet}
	{#snippet infoIcon()}
		<InfoIcon class="size-4" />
	{/snippet}
	{#snippet warningIcon()}
		<TriangleAlertIcon class="size-4" />
	{/snippet}
</Sonner>
