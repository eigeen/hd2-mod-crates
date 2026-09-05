import {
  HeadArmorMergeTool,
  type HeadMergeAdapter,
  type HeadMergeRecord,
} from "@hd2-mod-tools/migrator-app";
import { headMergeCandidates } from "@hd2-mod-tools/migrator-ui";
import {
  chooseOutputZip,
  choosePatchPaths,
  inspectPatch,
  startHeadArmorMerge,
  type DesktopTask,
} from "./desktopClient";

interface DesktopHeadMergeSource {
  paths: string[];
}

const desktopHeadMergeAdapter: HeadMergeAdapter<DesktopHeadMergeSource> = {
  cancellable: true,
  pickRecords: pickDesktopHeadMergeRecord,
  runMerge: runDesktopHeadMerge,
};

export function DesktopSharedHeadMergeTool() {
  return <HeadArmorMergeTool adapter={desktopHeadMergeAdapter} />;
}

async function pickDesktopHeadMergeRecord(): Promise<HeadMergeRecord<DesktopHeadMergeSource>[] | null> {
  const paths = await choosePatchPaths();
  if (!paths) return null;
  const result = await inspectPatch(paths, null);
  return [{
    armors: headMergeCandidates(result.inspection.sources, "Armor"),
    helmets: headMergeCandidates(result.inspection.sources, "Helmet"),
    id: crypto.randomUUID(),
    name: result.patch.name,
    source: { paths },
  }];
}

async function runDesktopHeadMerge(
  request: Parameters<HeadMergeAdapter<DesktopHeadMergeSource>["runMerge"]>[0],
) {
  const outputPath = await chooseOutputZip("hd2-head-merged-mod.zip");
  if (!outputPath) return null;
  const task = startHeadArmorMerge({
    armorPaths: request.armorRecord.source.paths,
    headPaths: request.helmetRecord.id === request.armorRecord.id
      ? null
      : request.helmetRecord.source.paths,
    outputPath,
    options: { armorHash: request.armorHash, helmetHash: request.helmetHash },
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
