import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { readFileSync } from "fs";
import { execSync } from "child_process";

const host = process.env.TAURI_DEV_HOST;

// Version single source of truth: src-tauri/tauri.conf.json (the same file
// build-signed.sh and Cargo.toml track). Reading package.json here showed a
// stale number — releases bump package.json via a clean git blob without
// rewriting the working tree, so its version drifts behind the bundle's.
// Local/uncommitted builds get a `-dev` suffix so a hand-built test app is
// never mistaken for a real release (CI builds a clean, tagged commit → no suffix).
function appVersion(): string {
  const base = JSON.parse(
    readFileSync("src-tauri/tauri.conf.json", "utf-8"),
  ).version as string;
  try {
    const dirty =
      execSync("git status --porcelain", {
        encoding: "utf-8",
        stdio: ["ignore", "pipe", "ignore"],
      }).trim().length > 0;
    return dirty ? `${base}-dev` : base;
  } catch {
    return base;
  }
}

export default defineConfig(async ({ mode }) => ({
  plugins: [react()],
  define: {
    __APP_VERSION__: JSON.stringify(mode === "development" ? "dev" : appVersion()),
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
