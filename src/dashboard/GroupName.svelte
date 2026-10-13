<script lang="ts">
  import { api } from "$lib/api";
  import { store } from "$lib/store.svelte";
  import EditableName from "$lib/components/app/EditableName.svelte";

  let { id, name }: { id: string; name: string } = $props();
</script>

<EditableName {name} label="Group name" class="w-full px-2 py-1 text-xs font-semibold text-muted-foreground" onsave={async (name) => {
  await api.renameGroup(id, name);
  await store.refresh();
}} />
