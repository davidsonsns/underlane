<script lang="ts">
  import { statusHint } from "$lib/format";
  import type { Status } from "$lib/types";
  import StatusDot from "./StatusDot.svelte";
  import Tip from "./Tip.svelte";

  let { status }: { status: Status } = $props();
</script>

<!-- The row states important status changes; the dot's description stays hover-only. -->
<Tip text={statusHint[status]}>
  {#snippet children({
    props,
  })}
    <span {...{ ...props, tabindex: undefined }} class="flex"
      ><StatusDot {status} /></span
    >
  {/snippet}
</Tip>
