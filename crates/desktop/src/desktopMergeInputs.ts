import { getCurrentWebview, type DragDropEvent } from "@tauri-apps/api/webview";
import {
  groupMergeInputFiles,
  isSupportedMergeInputName,
  type GroupedMergeInput,
  type MergeInputFile,
} from "@hd2-mod-tools/migrator-app";
import { enumeratePatchFiles } from "./desktopClient";
import type { DiscoveredPatchFile } from "./types";

export type DesktopMergeFile = MergeInputFile<string>;

export async function discoverDesktopPatchGroups(
  paths: string[],
): Promise<GroupedMergeInput<string>[]> {
  const files = await enumeratePatchFiles(paths);
  return groupMergeInputFiles(filterDirectoryExtras(files, paths).map(desktopMergeFile));
}

export function subscribeToDesktopMergeFiles(
  receive: (files: DesktopMergeFile[]) => void,
  reject: (error: unknown) => void,
): () => void {
  let disposed = false;
  let unsubscribe: (() => void) | undefined;
  void getCurrentWebview().onDragDropEvent(({ payload }) => {
    if (payload.type !== "drop" || payload.paths.length === 0) return;
    void discoverDesktopFiles(payload).then((files) => {
      if (!disposed) receive(files);
    }).catch((error) => {
      if (!disposed) reject(error);
    });
  }).then((value) => {
    if (disposed) value();
    else unsubscribe = value;
  }).catch((error) => {
    if (!disposed) reject(error);
  });
  return () => {
    disposed = true;
    unsubscribe?.();
  };
}

export function pathsFromDesktopGroup(group: GroupedMergeInput<string>): string[] {
  return [group.toc.source, group.gpu?.source, group.stream?.source]
    .filter((path): path is string => Boolean(path));
}

async function discoverDesktopFiles(event: Extract<DragDropEvent, { type: "drop" }>): Promise<DesktopMergeFile[]> {
  const files = await enumeratePatchFiles(event.paths);
  return filterDirectoryExtras(files, event.paths).map(desktopMergeFile);
}

function desktopMergeFile(file: DiscoveredPatchFile): DesktopMergeFile {
  return {
    byteLength: file.size,
    name: file.name,
    relativePath: joinPath(file.sourceRoot, file.relativePath),
    source: file.path,
  };
}

function joinPath(root: string, relativePath: string): string {
  return `${root.replace(/[\\/]+$/, "")}/${relativePath.replace(/^[\\/]+/, "")}`;
}

function filterDirectoryExtras(
  files: DiscoveredPatchFile[],
  selectedPaths: string[],
): DiscoveredPatchFile[] {
  const explicitlySelected = new Set(selectedPaths.map(normalizePath));
  return files.filter((file) => (
    explicitlySelected.has(normalizePath(file.path)) || isSupportedMergeInputName(file.name)
  ));
}

function normalizePath(path: string): string {
  return path.replace(/\\/g, "/").toLowerCase();
}
