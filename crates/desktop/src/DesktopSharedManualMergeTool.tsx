import {
  ManualMergeTool,
  groupMergeInputFiles,
  type ManualMergeAdapter,
  type ManualMergeRecord,
} from "@hd2-mod-tools/migrator-app";
import {
  chooseOutputZip,
  choosePatchPaths,
  describePatch,
  startPatchMerge,
} from "./desktopClient";
import { waitForDesktopTask } from "./desktopTask";
import {
  discoverDesktopPatchGroups,
  pathsFromDesktopGroup,
  subscribeToDesktopMergeFiles,
} from "./desktopMergeInputs";

interface DesktopMergeSource {
  paths: string[];
}

const desktopMergeAdapter: ManualMergeAdapter<DesktopMergeSource, string> = {
  cancellable: true,
  pickRecords: pickDesktopMergeRecord,
  recordsFromFiles: (files) => recordsFromGroups(groupMergeInputFiles(files)),
  runMerge: runDesktopMerge,
  subscribeToFileDrop: subscribeToDesktopMergeFiles,
};

export function DesktopSharedManualMergeTool() {
  return <ManualMergeTool adapter={desktopMergeAdapter} />;
}

async function pickDesktopMergeRecord(): Promise<ManualMergeRecord<DesktopMergeSource>[] | null> {
  const paths = await choosePatchPaths();
  if (!paths) return null;
  return recordsFromGroups(await discoverDesktopPatchGroups(paths));
}

async function recordsFromGroups(
  groups: ReturnType<typeof groupMergeInputFiles<string>>,
): Promise<ManualMergeRecord<DesktopMergeSource>[]> {
  return Promise.all(groups.map(async (group) => {
    const paths = pathsFromDesktopGroup(group);
    const patch = await describePatch(paths);
    return {
      byteLength: patch.byteLength,
      fileRoles: { gpu: Boolean(group.gpu), stream: Boolean(group.stream), toc: true },
      id: group.id,
      name: patch.name,
      source: { paths },
    };
  }));
}

async function runDesktopMerge(
  request: Parameters<ManualMergeAdapter<DesktopMergeSource>["runMerge"]>[0],
) {
  const outputPath = await chooseOutputZip("hd2-merged-mod.zip");
  if (!outputPath) return null;
  const task = startPatchMerge({
    inputPaths: request.records.map((record) => record.source.paths),
    outputName: request.outputName,
    outputPath,
  }, () => undefined);
  const summary = await waitForDesktopTask(task, request.signal);
  return { output: outputPath, summary };
}
