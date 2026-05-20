import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import path from "path";
import { defineConfig } from "vite";

// Mirrors the codeless-ui setup: React 19 + Tailwind v4 (Vite plugin)
// + the `@/` alias. The dev server proxies `/v1/*` and `/metrics` to
// the local hackline-gateway so the UI can use same-origin URLs and
// EventSource works without CORS gymnastics.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  server: {
    port: 1430,
    strictPort: true,
    // chokidar's default inotify watcher exhausts `fs.inotify.max_user_watches`
    // on workstations that already have other vite instances running, which
    // crashes startup with `ENOSPC`. Polling sidesteps the kernel limit at
    // the cost of a little CPU during dev — acceptable for a UI dev server.
    // Override with `HACKLINE_UI_NO_POLLING=1` when running on a host with
    // headroom and you want sub-second HMR.
    watch: process.env.HACKLINE_UI_NO_POLLING === "1"
      ? undefined
      : { usePolling: true, interval: 500 },
    proxy: {
      "/v1": {
        target: process.env.HACKLINE_GATEWAY_URL ?? "http://127.0.0.1:8080",
        changeOrigin: true,
      },
      "/metrics": {
        target: process.env.HACKLINE_GATEWAY_URL ?? "http://127.0.0.1:8080",
        changeOrigin: true,
      },
    },
  },
  build: {
    target: "es2020",
    chunkSizeWarningLimit: 1500,
  },
});
