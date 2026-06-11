import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import { fileURLToPath } from "node:url";

// Mirrors the `@core` alias + the `.js`->`.ts` rewrite from vite.config.ts so the
// tests consume the SAME core TS source the app bundles (no node:crypto path).
const coreSrc = fileURLToPath(new URL("../core/src", import.meta.url));

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

export default defineConfig({
  plugins: [coreJsToTs(), react()],
  resolve: {
    alias: {
      "@core": coreSrc,
    },
  },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test/setup.ts"],
  },
});
