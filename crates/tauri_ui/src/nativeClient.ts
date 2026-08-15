import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import type {
  EquipmentOption,
  GameDataDiscovery,
  InspectPatchResult,
  MigrateRequest,
  MigrationProgressEvent,
  MigrationSummary,
  RepatchRequest,
  UnitRepatchSummary,
} from "./types";

export function loadEquipmentOptions(): Promise<EquipmentOption[]> {
  return invoke("load_equipment_options");
}

export function detectGameDataDir(): Promise<GameDataDiscovery> {
  return invoke("detect_game_data_dir");
}

export function validateGameDataDir(path: string): Promise<void> {
  return invoke("validate_game_data_dir", { path });
}

export function inspectPatch(paths: string[], dataDir: string | null): Promise<InspectPatchResult> {
  return invoke("inspect_patch", { request: { paths, dataDir } });
}

export function migrateEquipment(request: MigrateRequest): Promise<MigrationSummary> {
  return invoke("migrate_equipment", { request });
}

export function repatchMod(request: RepatchRequest): Promise<UnitRepatchSummary> {
  return invoke("repatch_mod", { request });
}

export async function choosePatchPaths(): Promise<string[] | null> {
  const selected = await open({ multiple: true, title: "Choose patch files" });
  if (!selected) return null;
  return Array.isArray(selected) ? selected : [selected];
}

export async function chooseGameDataDir(): Promise<string | null> {
  const selected = await open({ directory: true, multiple: false, title: "Choose game data folder" });
  return typeof selected === "string" ? selected : null;
}

export function chooseOutputZip(defaultPath: string): Promise<string | null> {
  return save({
    defaultPath,
    filters: [{ name: "ZIP archive", extensions: ["zip"] }],
  });
}

export function subscribeToMigrationProgress(
  onProgress: (event: MigrationProgressEvent) => void,
) {
  return listen<MigrationProgressEvent>("migration://progress", ({ payload }) => onProgress(payload));
}
