// E2E snapshot mode (decisions.md §17). While the e2e harness reads the dashboard's text,
// the virtualized lists render every row, so innerText sees rows outside the viewport.
// Only App.svelte's e2e handler sets it; the control server exists only in e2e debug builds.
export const snapshot = $state({ full: false });
