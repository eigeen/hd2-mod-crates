import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig, type Plugin } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

const here = dirname(fileURLToPath(import.meta.url));
const workspaceRoot = resolve(here, "../..");

const sharedAssets: Array<[string, string]> = [
  ["assets/armor_mappings.merged.json", "public/assets/armor_mappings.merged.json"],
  ["assets/helmet_mappings.json", "public/assets/helmet_mappings.json"],
];

function syncSharedAssets(): Plugin {
  return {
    name: "hd2-sync-shared-assets",
    configResolved() {
      for (const [from, to] of sharedAssets) {
        const src = resolve(workspaceRoot, from);
        const dest = resolve(here, to);
        mkdirSync(dirname(dest), { recursive: true });
        copyFileSync(src, dest);
      }
    },
  };
}

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
  plugins: [syncSharedAssets(), react(), tailwindcss()],
  define: {
    __GIT_HASH__: JSON.stringify(gitShortHash()),
  },
  build: {
    target: "es2022",
  },
});
