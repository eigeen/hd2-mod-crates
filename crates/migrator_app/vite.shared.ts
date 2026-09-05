import { execFileSync } from "node:child_process";
import type { UserConfig } from "vite";

export type FrontendTarget = "web" | "desktop";

/** Keep build metadata and browser targets identical across both frontend hosts. */
export function sharedFrontendConfig(
  workspaceRoot: string,
  target: FrontendTarget,
): Pick<UserConfig, "build" | "define"> {
  return {
    define: {
      __APP_TARGET__: JSON.stringify(target),
      __GIT_HASH__: JSON.stringify(gitShortHash(workspaceRoot)),
    },
    build: {
      target: "es2022",
    },
  };
}

function gitShortHash(workspaceRoot: string): string {
  try {
    return execFileSync("git", ["rev-parse", "--short=7", "HEAD"], {
      cwd: workspaceRoot,
      encoding: "utf8",
    }).trim();
  } catch {
    return "unknown";
  }
}
