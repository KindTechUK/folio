import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri expects a fixed dev port and ignores the src-tauri folder for HMR.
// No proxy / no network targets — the frontend talks to the backend only via
// the Tauri IPC bridge, never over HTTP. This is part of the zero-egress posture.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
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
