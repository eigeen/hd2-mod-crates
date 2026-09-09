import {
  HeadArmorMergeTool,
  groupMergeInputFiles,
  type HeadMergeAdapter,
  type HeadMergeRecord,
} from "@hd2-mod-tools/migrator-app";
import { headMergeCandidates } from "@hd2-mod-tools/migrator-ui";
import {
  chooseOutputZip,
  choosePatchPaths,
  inspectPatch,
  startHeadArmorMerge,
} from "./desktopClient";
import { waitForDesktopTask } from "./desktopTask";
import {
  discoverDesktopPatchGroups,
  pathsFromDesktopGroup,
  subscribeToDesktopMergeFiles,
} from "./desktopMergeInputs";

interface DesktopHeadMergeSource {
  paths: string[];
}

const desktopHeadMergeAdapter: HeadMergeAdapter<DesktopHeadMergeSource, string> = {
  cancellable: true,
  pickRecords: pickDesktopHeadMergeRecord,
  recordsFromFiles: (files) => recordsFromGroups(groupMergeInputFiles(files)),
  runMerge: runDesktopHeadMerge,
  subscribeToFileDrop: subscribeToDesktopMergeFiles,
};

export function DesktopSharedHeadMergeTool() {
  return <HeadArmorMergeTool adapter={desktopHeadMergeAdapter} />;
}

async function pickDesktopHeadMergeRecord(): Promise<HeadMergeRecord<DesktopHeadMergeSource>[] | null> {
  const paths = await choosePatchPaths();
  if (!paths) return null;
  return recordsFromGroups(await discoverDesktopPatchGroups(paths));
}

async function recordsFromGroups(
  groups: ReturnType<typeof groupMergeInputFiles<string>>,
): Promise<HeadMergeRecord<DesktopHeadMergeSource>[]> {
  return Promise.all(groups.map(async (group) => {
    const paths = pathsFromDesktopGroup(group);
    const result = await inspectPatch(paths, null);
    return {
      armors: headMergeCandidates(result.inspection.sources, "Armor"),
      helmets: headMergeCandidates(result.inspection.sources, "Helmet"),
      id: group.id,
      name: result.patch.name,
      source: { paths },
    };
  }));
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
  const summary = await waitForDesktopTask(task, request.signal);
  return { output: outputPath, summary };
}
