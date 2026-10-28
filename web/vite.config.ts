import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { writeBuiltinCss } from "./scripts/themes.mjs";

// The daemon serves `dist/` (see crates/neko-daemon/src/web.rs), so the app is
// built with relative-free absolute asset paths and no dev server is needed:
// `pnpm watch` rebuilds on every save and the page reloads itself.
//
// The daemon's Content-Security-Policy is `default-src 'self'`, which refuses
// `data:` fonts: nothing may be inlined into the CSS (assetsInlineLimit 0).
// Libraries every first paint loads get chunks of their own, so shipping a UI
// change (scripts/ship-web.sh) leaves their hashes, and the browser's cached
// copies, alone. Only packages the main chunk already holds belong here: a
// library only lazy pages use (recharts, dockview, shiki) must stay with them.
const VENDOR: [string, RegExp][] = [
  ["react", /\/node_modules\/(react|react-dom|scheduler)\//],
  ["base-ui", /\/node_modules\/@(base-ui|floating-ui)\//],
  ["ai-sdk", /\/node_modules\/(ai|@ai-sdk\/[^/]+|zod|@standard-schema\/[^/]+)\//],
  ["motion", /\/node_modules\/(motion|motion-dom|motion-utils|framer-motion)\//],
];
function vendorChunk(id: string): string | undefined {
  return VENDOR.find(([, test]) => test.test(id))?.[0];
}

export default defineConfig({
  plugins: [
    // The built-in themes are data (src/themes/builtin/*.json); their stylesheet is derived from it on every build.
    { name: "neko-builtin-themes", buildStart: () => void writeBuiltinCss() },
    react(),
    tailwindcss(),
  ],
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
      mermaid: fileURLToPath(new URL("./src/stubs/mermaid.ts", import.meta.url)),
    },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    sourcemap: true,
    target: "es2022",
    assetsInlineLimit: 0,
    rollupOptions: { output: { manualChunks: vendorChunk } },
  },
});
