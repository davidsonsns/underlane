// What the popover loads after its first open (02 O10): the toast and tooltip surfaces. One module,
// so the bundler makes one lazy chunk (and fewer shared chunks) rather than one per import().
// The dashboard imports these statically too, so there the import() resolves at once.
export { toast } from "svelte-sonner";
export { default as Toaster } from "$lib/components/ui/sonner/sonner.svelte";
export { default as TooltipContent } from "$lib/components/ui/tooltip/tooltip-content.svelte";
