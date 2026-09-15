import { resolve } from "node:path";

import { defineConfig } from "vite";

const contentSecurityPolicy =
  "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self' http://127.0.0.1:* http://localhost:*; object-src 'none'; base-uri 'none'; frame-ancestors 'none'";
const securityHeaders = {
  "Content-Security-Policy": contentSecurityPolicy,
  "Referrer-Policy": "no-referrer",
  "X-Content-Type-Options": "nosniff",
};

export default defineConfig({
  root: resolve(import.meta.dirname, "src/quicklens/browser"),
  build: {
    outDir: resolve(import.meta.dirname, "dist/quicklens/browser"),
  },
  preview: {
    host: "127.0.0.1",
    port: 4174,
    strictPort: true,
    headers: securityHeaders,
  },
});
