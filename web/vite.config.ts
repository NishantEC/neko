import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// The daemon serves `dist/` (see crates/neko-daemon/src/web.rs), so the app is
// built with relative-free absolute asset paths and no dev server is needed:
// `pnpm watch` rebuilds on every save and the page reloads itself.
export default defineConfig({
  plugins: [react()],
  build: { outDir: "dist", emptyOutDir: true, sourcemap: true, target: "es2022" },
});
