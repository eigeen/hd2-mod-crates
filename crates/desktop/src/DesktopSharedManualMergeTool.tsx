import {
  ManualMergeTool,
  type ManualMergeAdapter,
  type ManualMergeRecord,
} from "@hd2-mod-tools/migrator-app";
import {
  chooseOutputZip,
  choosePatchPaths,
  describePatch,
  startPatchMerge,
  type DesktopTask,
} from "./desktopClient";

interface DesktopMergeSource {
  paths: string[];
}

const desktopMergeAdapter: ManualMergeAdapter<DesktopMergeSource> = {
  cancellable: true,
  pickRecords: pickDesktopMergeRecord,
  runMerge: runDesktopMerge,
};

export function DesktopSharedManualMergeTool() {
  return <ManualMergeTool adapter={desktopMergeAdapter} />;
}

async function pickDesktopMergeRecord(): Promise<ManualMergeRecord<DesktopMergeSource>[] | null> {
  const paths = await choosePatchPaths();
  if (!paths) return null;
  const patch = await describePatch(paths);
  return [{
    byteLength: patch.byteLength,
    id: crypto.randomUUID(),
    name: patch.name,
    source: { paths },
  }];
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
  const summary = await waitForTask(task, request.signal);
  return { output: outputPath, summary };
}

async function waitForTask<T>(task: DesktopTask<T>, signal: AbortSignal): Promise<T> {
  const cancel = () => { void task.cancel(); };
  signal.addEventListener("abort", cancel, { once: true });
  if (signal.aborted) cancel();
  try {
    return await task.result;
  } finally {
    signal.removeEventListener("abort", cancel);
  }
}
