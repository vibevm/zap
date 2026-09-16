import { resolve } from "node:path";

import { qwikVite } from "@qwik.dev/core/optimizer";
import { defineConfig } from "vite";

const packageRoot = import.meta.dirname;
const browserRoot = resolve(packageRoot, "src/quicklens/browser");
const contentSecurityPolicy =
  "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self' http://127.0.0.1:* http://localhost:*; object-src 'none'; base-uri 'none'; frame-ancestors 'none'";
const securityHeaders = {
  "Content-Security-Policy": contentSecurityPolicy,
  "Referrer-Policy": "no-referrer",
  "X-Content-Type-Options": "nosniff",
};

export default defineConfig({
  root: browserRoot,
  base: "/",
  plugins: [
    qwikVite({
      csr: true,
      srcDir: resolve(packageRoot, "src"),
      tsconfigFileNames: ["tsconfig.browser.json"],
      lint: false,
    }),
  ],
  server: {
    host: "127.0.0.1",
    port: 4173,
    strictPort: true,
    headers: securityHeaders,
  },
  preview: {
    host: "127.0.0.1",
    port: 4174,
    strictPort: true,
    headers: securityHeaders,
  },
  build: {
    outDir: resolve(packageRoot, "dist/quicklens/browser"),
    emptyOutDir: true,
    target: "es2022",
    sourcemap: true,
  },
});
