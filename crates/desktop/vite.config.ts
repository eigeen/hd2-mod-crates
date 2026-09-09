import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";
import { sharedFrontendConfig } from "../migrator_app/vite.shared.ts";

const here = dirname(fileURLToPath(import.meta.url));
const workspaceRoot = resolve(here, "../..");
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  ...sharedFrontendConfig(workspaceRoot, "desktop"),
  plugins: [react(), tailwindcss()],
  publicDir: resolve(here, "../migrator_app/public"),
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
});
