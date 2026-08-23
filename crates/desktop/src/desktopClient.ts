import { Channel, invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import type {
  EquipmentOption,
  AppUpdateMetadata,
  EquipmentMappingPreview,
  GameDataDiscovery,
  InspectPatchResult,
  MigrateRequest,
  MigrationProgressEvent,
  MigrationSummary,
  MigrationMapping,
  RepatchRequest,
  UnitRepatchSummary,
} from "./types";

export function checkAppUpdate(): Promise<AppUpdateMetadata | null> {
  return invoke("check_app_update");
}

export function installAppUpdate(version: string): Promise<void> {
  return invoke("install_app_update", { version });
}

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

export function previewEquipmentMapping(
  patchPaths: string[],
  mapping: MigrationMapping,
): Promise<EquipmentMappingPreview> {
  return invoke("preview_equipment_mapping", { request: { patchPaths, mapping } });
}

export function previewEquipmentMappings(
  patchPaths: string[],
  mappings: MigrationMapping[],
): Promise<EquipmentMappingPreview[]> {
  return invoke("preview_equipment_mappings", { request: { patchPaths, mappings } });
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

export interface DesktopTask<T> {
  id: string;
  result: Promise<T>;
  cancel: () => Promise<boolean>;
}

export function startMigration(
  request: MigrateRequest,
  onProgress: (event: MigrationProgressEvent) => void,
): DesktopTask<MigrationSummary> {
  return startTask("migrate_equipment", request, onProgress);
}

export function startRepatch(
  request: RepatchRequest,
  onProgress: (event: MigrationProgressEvent) => void,
): DesktopTask<UnitRepatchSummary> {
  return startTask("repatch_mod", request, onProgress);
}

function startTask<TRequest, TResult>(
  command: string,
  request: TRequest,
  onProgress: (event: MigrationProgressEvent) => void,
): DesktopTask<TResult> {
  const id = crypto.randomUUID();
  const channel = new Channel<MigrationProgressEvent>();
  channel.onmessage = onProgress;
  return {
    id,
    result: invoke<TResult>(command, { request, taskId: id, onProgress: channel }),
    cancel: () => invoke<boolean>("cancel_task", { taskId: id }),
  };
}
