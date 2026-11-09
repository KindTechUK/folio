import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { fileURLToPath } from "node:url";

// `@core` resolves to the core TS SOURCE (core has no build step). We import the
// crypto-free modules directly (editor/transcript/types) and NEVER core/src/index.ts,
// which re-exports modelpack.ts -> node:crypto and would break the browser build.
const coreSrc = fileURLToPath(new URL("../core/src", import.meta.url));

// Core's relative imports are written with a `.js` suffix (NodeNext style) but the
// files on disk are `.ts`. This plugin rewrites those `.js` specifiers to `.ts` for
// anything resolved under core/src.
function coreJsToTs() {
  return {
    name: "core-js-to-ts",
    enforce: "pre" as const,
    async resolveId(source: string, importer: string | undefined) {
      if (
        importer &&
        importer.includes("/core/src/") &&
        source.startsWith(".") &&
        source.endsWith(".js")
      ) {
        const resolved = await this.resolve(
          source.replace(/\.js$/, ".ts"),
          importer,
          { skipSelf: true },
        );
        if (resolved) return resolved;
      }
      return null;
    },
  };
}

// Tauri expects a fixed dev port and ignores the src-tauri folder for HMR.
// No proxy / no network targets — the frontend talks to the backend only via
// the Tauri IPC bridge, never over HTTP. This is part of the zero-egress posture.
export default defineConfig({
  plugins: [coreJsToTs(), react()],
  clearScreen: false,
  resolve: {
    alias: {
      "@core": coreSrc,
    },
  },
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    // Keep the bundle inspectable for the assurance pack.
    target: "es2021",
    sourcemap: true,
  },
});
