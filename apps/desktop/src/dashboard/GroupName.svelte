<script lang="ts">
  import { api } from "$lib/api";
  import EditableName from "$lib/components/app/EditableName.svelte";
  import { store } from "$lib/store.svelte";

  let { id, name }: { id: string; name: string } = $props();
</script>

<EditableName
  class="w-full px-2 py-1 text-xs font-semibold text-muted-foreground"
  label="Group name"
  {name}
  onsave={async (name) => {
    await api.renameGroup(id, name);
    await store.refresh();
  }}
/>
