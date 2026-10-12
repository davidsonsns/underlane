<script lang="ts">
	// burrow: macOS overrides, see .scratch/shadcn-luma/01-theming.md §5 (stock Luma: commit 9f0aa7b).
	// Burrow: no theme-watcher package (it fights data-theme). The caller passes settings.theme ("system" | "light" | "dark");
	// "system" makes sonner follow prefers-color-scheme, which Rust's set_theme keeps in sync.
	// Styled as an in-window notification banner: menu material, 12px corners, 13px text.
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
	style="--normal-bg: var(--popover); --normal-text: var(--popover-foreground); --normal-border: var(--input); --border-radius: var(--radius-xl); --width: 320px;"
	toastOptions={{
		classes: {
			toast: "!font-sans !text-sm !border-[0.5px] !shadow-none !backdrop-blur-xl !backdrop-saturate-150 !gap-2 !px-3 !py-2.5",
			title: "!text-sm !font-semibold",
			description: "!text-callout !text-muted-foreground",
			success: "[&_[data-icon]]:!text-ok",
			error: "[&_[data-icon]]:!text-bad",
			warning: "[&_[data-icon]]:!text-degraded",
			actionButton: "!h-[22px] !rounded-md !bg-primary !px-2.5 !text-sm !font-normal !text-primary-foreground",
			cancelButton: "!h-[22px] !rounded-md !bg-control !px-2.5 !text-sm !font-normal !text-foreground !border-[0.5px] !border-solid !border-input",
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
