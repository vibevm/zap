import { resolve } from "node:path";

import { defineConfig } from "vite";

export default defineConfig({
  build: {
    outDir: resolve(import.meta.dirname, "dist/quicklens/electron"),
    emptyOutDir: false,
    target: "node22",
    sourcemap: true,
    lib: {
      entry: resolve(import.meta.dirname, "src/quicklens/electron/preload.ts"),
      formats: ["cjs"],
      fileName: () => "preload.cjs",
    },
    rollupOptions: {
      external: ["electron"],
    },
  },
});
