import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { readFileSync } from "fs";

const host = process.env.TAURI_DEV_HOST;
const pkg = JSON.parse(readFileSync("package.json", "utf-8"));

export default defineConfig(async ({ mode }) => ({
  plugins: [react()],
  define: {
    __APP_VERSION__: JSON.stringify(mode === "development" ? "dev" : pkg.version),
  },
  clearScreen: false,
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
      ignored: ["**/src-tauri/**"],
    },
  },
  worker: {
    format: "es" as const,
  },
  // Optimize Monaco editor bundling
  optimizeDeps: {
    include: ['monaco-editor'],
  },
}));
