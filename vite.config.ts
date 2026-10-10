import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { resolve } from "node:path";

// Two pages: the dashboard window (index.html) and the menu bar popover (popover.html).
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: {
    target: "safari16",
    rollupOptions: {
      input: {
        dashboard: resolve(__dirname, "index.html"),
        popover: resolve(__dirname, "popover.html"),
      },
    },
  },
  test: { environment: "node", include: ["src/**/*.test.ts"] },
});
