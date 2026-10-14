<script lang="ts">
  import { cn } from "$lib/utils";
  let { thumb, animate }: { thumb: { x: number; y: number; w: number; h: number }; animate: boolean } = $props();
</script>

<div aria-hidden="true" class={cn("pointer-events-none absolute top-0 left-0 -z-1 rounded-[5px] border-[0.5px] border-input bg-segment", animate && "transition-[translate,width,height] duration-200 ease-out motion-reduce:transition-none")} style:translate="{thumb.x}px {thumb.y}px" style:width="{thumb.w}px" style:height="{thumb.h}px"></div>
