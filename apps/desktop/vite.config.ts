import { resolve } from "node:path";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "vite";

// Two pages: the dashboard window (index.html) and the menu bar popover (popover.html).
export default defineConfig({
  build: {
    rollupOptions: {
      input: {
        dashboard: resolve(import.meta.dirname, "index.html"),
        popover: resolve(import.meta.dirname, "popover.html"),
      },
    },
    target: "safari16.4",
  },
  clearScreen: false,
  plugins: [tailwindcss(), svelte()],
  resolve: { alias: { $lib: resolve(import.meta.dirname, "src/lib") } },
  server: { port: 1420, strictPort: true },
  test: { environment: "node", include: ["src/**/*.test.ts"] },
});
