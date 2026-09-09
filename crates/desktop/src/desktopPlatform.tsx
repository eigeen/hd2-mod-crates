import { getCurrentWebview, type DragDropEvent } from "@tauri-apps/api/webview";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { useCallback, useEffect, useRef, useState } from "react";
import { createMigrationProgressCounter } from "@hd2-mod-tools/migrator-ui";
import {
  DEFAULT_REPATCH_OUTPUT,
  type MigratorPlatform,
  type SelectedGameData,
  type SelectedPatch,
  type SetupPanelsProps,
} from "@hd2-mod-tools/migrator-app";
import {
  chooseGameDataDir,
  chooseOutputZip,
  choosePatchPaths,
  describePatch,
  detectGameDataDir,
  inspectPatch,
  loadEquipmentOptions,
  previewEquipmentMappings,
  startMigration,
  startRepatch,
  validateGameDataDir,
} from "./desktopClient";
import { waitForDesktopTask } from "./desktopTask";
import { DesktopGameDataPanel, DesktopPatchPanel } from "./DesktopPanels";
import { dropZoneFromPhysicalPosition, type DropZone } from "./dropTarget";
import type { PatchDescriptor } from "./types";

const GAME_DATA_STORAGE_KEY = "hd2-migrator-native-game-data";

export const desktopPlatform: MigratorPlatform<string[], string> = {
  copyDiagnostic: writeText,
  inspectPatch: async (paths, dataDir) => {
    const result = await inspectPatch(paths, dataDir);
    return { equipmentGraph: result.equipmentGraph, inspection: result.inspection };
  },
  loadEquipmentOptions,
  previewMappings: previewEquipmentMappings,
  restoreGameData: restoreDesktopGameData,
  revealOutput: revealItemInDir,
  runMigration: runDesktopMigration,
  runRepatch: runDesktopRepatch,
};

export function DesktopSetupPanels(props: SetupPanelsProps<string[], string>) {
  const [hoveredDropZone, setHoveredDropZone] = useState<DropZone | null>(null);
  const hoveredDropZoneRef = useRef<DropZone | null>(null);

  const importPatchPaths = useCallback(async (paths: string[]) => {
    try {
      props.onPatchChange(await selectedDesktopPatch(paths));
    } catch (error) {
      props.reportError(error);
    }
  }, [props.onPatchChange, props.reportError]);

  const importGameData = useCallback(async (path: string) => {
    try {
      await validateGameDataDir(path);
      localStorage.setItem(GAME_DATA_STORAGE_KEY, path);
      props.onGameDataChange(selectedDesktopGameData(path));
    } catch (error) {
      props.reportError(error);
    }
  }, [props.onGameDataChange, props.reportError]);

  useEffect(() => subscribeToDesktopDrop({
    hoveredDropZoneRef,
    importGameData,
    importPatchPaths,
    setHoveredDropZone,
  }), [importGameData, importPatchPaths]);

  return (
    <div className="flex flex-col min-[51.25rem]:flex-row">
      <div className="flex min-w-0 flex-1">
        <DesktopGameDataPanel
          dataDir={props.gameData?.label ?? null}
          dragging={hoveredDropZone === "gameData"}
          onChange={() => void chooseGameDataDir().then((path) => {
            if (path) return importGameData(path);
          })}
          onClear={() => {
            localStorage.removeItem(GAME_DATA_STORAGE_KEY);
            props.onGameDataChange(null);
          }}
        />
      </div>
      <div className="flex min-w-0 flex-1 border-t border-hd2-border min-[51.25rem]:border-t-0 min-[51.25rem]:border-l">
        <DesktopPatchPanel
          dragging={hoveredDropZone === "patch"}
          onChoose={() => void choosePatchPaths().then((paths) => {
            if (paths) return importPatchPaths(paths);
          })}
          patch={props.patch ? descriptorFromSelection(props.patch) : null}
        />
      </div>
    </div>
  );
}

async function runDesktopMigration(
  request: Parameters<typeof desktopPlatform.runMigration>[0],
) {
  const outputPath = await chooseOutputZip(
    request.suggestedOutputFilename,
  );
  if (!outputPath) return null;
  const counter = createMigrationProgressCounter(request.progressLabels, request.onProgressLabel);
  const task = startMigration({
    patchPaths: request.patch.source,
    dataDir: request.gameData,
    outputPath,
    options: request.options,
  }, (event) => {
    if (event.kind === "targetFinish") counter.finish(event.targetName);
  });
  const summary = await waitForDesktopTask(task, request.signal);
  return { output: outputPath, summary };
}

async function runDesktopRepatch(
  request: Parameters<typeof desktopPlatform.runRepatch>[0],
) {
  const outputPath = await chooseOutputZip(DEFAULT_REPATCH_OUTPUT);
  if (!outputPath) return null;
  request.onStage("reading_patch");
  const task = startRepatch({
    patchPaths: request.patch.source,
    dataDir: request.gameData,
    outputPath,
    options: { missingUnitPolicy: request.missingUnitPolicy },
  }, (event) => {
    if (event.kind === "stage") request.onStage(event.stage);
  });
  const summary = await waitForDesktopTask(task, request.signal);
  return { output: outputPath, summary };
}

async function selectedDesktopPatch(paths: string[]): Promise<SelectedPatch<string[]>> {
  const descriptor = await describePatch(paths);
  return {
    byteLength: descriptor.byteLength,
    info: { name: descriptor.name },
    key: paths.join("|"),
    originalName: descriptor.originalName ?? undefined,
    source: paths,
  };
}

function descriptorFromSelection(selection: SelectedPatch<string[]>): PatchDescriptor {
  return {
    byteLength: selection.byteLength,
    name: selection.info.name,
    originalName: selection.originalName ?? null,
    path: selection.source[0] ?? "",
  };
}

function selectedDesktopGameData(path: string): SelectedGameData<string> {
  return { key: path, label: path, ready: true, source: path };
}

async function restoreDesktopGameData(): Promise<SelectedGameData<string> | null> {
  const stored = localStorage.getItem(GAME_DATA_STORAGE_KEY);
  if (stored && await isValidGameData(stored)) return selectedDesktopGameData(stored);
  if (stored) localStorage.removeItem(GAME_DATA_STORAGE_KEY);
  const discovery = await detectGameDataDir().catch(() => null);
  if (!discovery?.dataDir) return null;
  localStorage.setItem(GAME_DATA_STORAGE_KEY, discovery.dataDir);
  return selectedDesktopGameData(discovery.dataDir);
}

async function isValidGameData(path: string): Promise<boolean> {
  try {
    await validateGameDataDir(path);
    return true;
  } catch {
    return false;
  }
}

interface DesktopDropSubscription {
  hoveredDropZoneRef: { current: DropZone | null };
  importGameData: (path: string) => Promise<void>;
  importPatchPaths: (paths: string[]) => Promise<void>;
  setHoveredDropZone: (zone: DropZone | null) => void;
}

function subscribeToDesktopDrop(subscription: DesktopDropSubscription) {
  let disposed = false;
  let unsubscribe: (() => void) | undefined;
  void getCurrentWebview().onDragDropEvent(({ payload }) => {
    void handleDesktopDrop(payload, subscription);
  }).then((value) => {
    if (disposed) value();
    else unsubscribe = value;
  });
  return () => {
    disposed = true;
    unsubscribe?.();
  };
}

async function handleDesktopDrop(event: DragDropEvent, subscription: DesktopDropSubscription) {
  if (event.type === "leave") {
    updateHoveredDropZone(null, subscription);
    return;
  }
  const zone = dropZoneFromPhysicalPosition(event.position);
  updateHoveredDropZone(zone, subscription);
  if (event.type !== "drop") return;
  updateHoveredDropZone(null, subscription);
  if (!zone || event.paths.length === 0) return;
  if (zone === "gameData") await subscription.importGameData(event.paths[0]);
  else await subscription.importPatchPaths(event.paths);
}

function updateHoveredDropZone(zone: DropZone | null, subscription: DesktopDropSubscription) {
  subscription.hoveredDropZoneRef.current = zone;
  subscription.setHoveredDropZone(zone);
}
