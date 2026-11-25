import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
// @ts-expect-error type error without @types/node package
import process from "node:process";
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
// Dev only: forwards errors from the Tauri web views to this terminal (see src/main.tsx).
const webviewLog = {
  name: "webview-log",
  configureServer(server: { ws: { on: (event: string, cb: (data: unknown) => void) => void } }) {
    server.ws.on("webview:log", (data) => console.log("[webview]", JSON.stringify(data)));
  },
};

export default defineConfig(() => ({
  plugins: [react(), webviewLog],

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
