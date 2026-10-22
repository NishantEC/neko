import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// The daemon serves `dist/` (see crates/neko-daemon/src/web.rs), so the app is
// built with relative-free absolute asset paths and no dev server is needed:
// `pnpm watch` rebuilds on every save and the page reloads itself.
//
// The daemon's Content-Security-Policy is `default-src 'self'`, which refuses
// `data:` fonts: nothing may be inlined into the CSS (assetsInlineLimit 0).
export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
      mermaid: fileURLToPath(new URL("./src/stubs/mermaid.ts", import.meta.url)),
    },
  },
  build: { outDir: "dist", emptyOutDir: true, sourcemap: true, target: "es2022", assetsInlineLimit: 0 },
});
