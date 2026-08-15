import { execFileSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

const here = dirname(fileURLToPath(import.meta.url));
const workspaceRoot = resolve(here, "../..");
const host = process.env.TAURI_DEV_HOST;

function gitShortHash(): string {
  try {
    return execFileSync("git", ["rev-parse", "--short=7", "HEAD"], {
      cwd: workspaceRoot,
      encoding: "utf8",
    }).trim();
  } catch {
    return "unknown";
  }
}

export default defineConfig({
  plugins: [react(), tailwindcss()],
  publicDir: resolve(here, "../web_ui/public"),
  define: {
    __GIT_HASH__: JSON.stringify(gitShortHash()),
  },
  build: {
    target: "es2022",
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
});
