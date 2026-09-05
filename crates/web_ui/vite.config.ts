import { copyFileSync, mkdirSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig, type Plugin } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { sharedFrontendConfig } from "../migrator_app/vite.shared.ts";

const here = dirname(fileURLToPath(import.meta.url));
const workspaceRoot = resolve(here, "../..");
const sharedPublicDir = resolve(here, "../migrator_app/public");

const sharedAssets: Array<[string, string]> = [
  ["assets/armor_mappings.merged.json", "assets/armor_mappings.merged.json"],
  ["assets/helmet_mappings.json", "assets/helmet_mappings.json"],
];

function syncSharedAssets(): Plugin {
  return {
    name: "hd2-sync-shared-assets",
    configResolved() {
      for (const [from, to] of sharedAssets) {
        const src = resolve(workspaceRoot, from);
        const dest = resolve(sharedPublicDir, to);
        mkdirSync(dirname(dest), { recursive: true });
        copyFileSync(src, dest);
      }
    },
  };
}

function emitWebHostFiles(): Plugin {
  return {
    name: "hd2-emit-web-host-files",
    buildStart() {
      for (const name of ["_headers", "_redirects"]) {
        this.emitFile({
          type: "asset",
          fileName: name,
          source: readFileSync(resolve(here, "public", name), "utf8"),
        });
      }
    },
  };
}

export default defineConfig({
  ...sharedFrontendConfig(workspaceRoot, "web"),
  plugins: [syncSharedAssets(), emitWebHostFiles(), react(), tailwindcss()],
  publicDir: sharedPublicDir,
});
