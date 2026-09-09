import { describe, expect, test } from "bun:test";
import { resolve } from "node:path";

const workspaceRoot = resolve(import.meta.dir, "../../..");

describe("frontend platform boundaries", () => {
  test("the shared app does not import Tauri or WASM transports", async () => {
    const sources = await Promise.all([
      read("crates/migrator_app/src/MigratorApp.tsx"),
      read("crates/migrator_app/src/ManualMergeTool.tsx"),
      read("crates/migrator_app/src/HeadArmorMergeTool.tsx"),
      read("crates/migrator_app/src/platform.ts"),
      read("crates/migrator_app/src/useMigrationWorkspace.ts"),
    ]);
    const sharedSource = sources.join("\n");
    expect(sharedSource).not.toContain("@tauri-apps/");
    expect(sharedSource).not.toContain("wasmClient");
    expect(sharedSource).not.toContain("hd2_migrator_wasm");
  });

  test("each host entry only assembles the shared app", async () => {
    const webEntry = await read("crates/web_ui/src/App.tsx");
    const desktopEntry = await read("crates/desktop/src/App.tsx");
    expect(lineCount(webEntry)).toBeLessThan(120);
    expect(lineCount(desktopEntry)).toBeLessThan(120);
    expect(webEntry).not.toContain("wasmClient");
    expect(desktopEntry).not.toContain("desktopClient");
  });

  test("platform transports stay in their own adapters", async () => {
    const webAdapter = await read("crates/web_ui/src/webPlatform.tsx");
    const desktopAdapter = await read("crates/desktop/src/desktopPlatform.tsx");
    expect(webAdapter).not.toContain("@tauri-apps/");
    expect(desktopAdapter).not.toContain("wasmClient");
    expect(desktopAdapter).not.toContain("hd2_migrator_wasm");
  });

  test("Tailwind scans the shared app and both platform hosts", async () => {
    const styles = await read("crates/migrator_ui/src/styles.css");
    expect(styles).toContain('@source "../../migrator_app/src/**/*.{ts,tsx}"');
    expect(styles).toContain('@source "../../web_ui/src/**/*.{ts,tsx}"');
    expect(styles).toContain('@source "../../desktop/src/**/*.{ts,tsx}"');
  });
});

function read(path: string): Promise<string> {
  return Bun.file(resolve(workspaceRoot, path)).text();
}

function lineCount(source: string): number {
  return source.split(/\r?\n/).length;
}
