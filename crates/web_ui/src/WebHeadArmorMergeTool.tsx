import { useMemo } from "react";
import {
  HeadArmorMergeTool,
  type HeadMergeAdapter,
  type HeadMergeRecord,
  type MergeInputFile,
} from "@hd2-mod-tools/migrator-app";
import {
  TaskError,
  headMergeCandidates,
  throwIfTaskCancelled,
  useI18n,
} from "@hd2-mod-tools/migrator-ui";
import { downloadZip } from "./fileInputs";
import {
  loadManualMergePatch,
  readManualMergeToc,
  mergeInputErrorMessage,
  recordsFromMergeInputFiles,
  validateManualMergeRecords,
  type ManualMergeRecord,
} from "./manualMergeInputs";
import { inspectEquipmentContents, mergeHeadIntoArmor, patchSidecarRequirements } from "./wasmClient";

export function WebHeadArmorMergeTool() {
  const { t } = useI18n();
  const adapter = useMemo<HeadMergeAdapter<ManualMergeRecord, File>>(() => ({
    browserFileReference,
    cancellable: true,
    pickRecords: async (signal) => {
      const files = await chooseFiles();
      throwIfTaskCancelled(signal);
      return files ? createRecords(files.map(browserFileReference), t, signal) : null;
    },
    recordsFromFiles: async (files, signal) => createRecords(files, t, signal),
    runMerge: runWebHeadMerge,
  }), [t]);
  return <HeadArmorMergeTool adapter={adapter} />;
}

function browserFileReference(file: File): MergeInputFile<File> {
  return {
    byteLength: file.size,
    name: file.name,
    relativePath: file.webkitRelativePath || file.name,
    source: file,
  };
}

async function createRecords(
  files: MergeInputFile<File>[],
  t: ReturnType<typeof useI18n>["t"],
  signal: AbortSignal,
): Promise<HeadMergeRecord<ManualMergeRecord>[]> {
  let records: ManualMergeRecord[];
  try {
    records = recordsFromMergeInputFiles(files);
    await validateManualMergeRecords(records, t,
      (toc, streamLength) => patchSidecarRequirements(toc, streamLength, { signal }), { signal });
  } catch (error) {
    if (error instanceof TaskError) throw error;
    throw new Error(mergeInputErrorMessage(error, t), { cause: error });
  }
  const inspected: HeadMergeRecord<ManualMergeRecord>[] = [];
  for (const record of records) {
    throwIfTaskCancelled(signal);
    const inspection = await inspectEquipmentContents(await readManualMergeToc(record, signal), undefined, { signal });
    inspected.push({
      armors: headMergeCandidates(inspection.sources, "Armor"),
      helmets: headMergeCandidates(inspection.sources, "Helmet"),
      id: record.id,
      name: record.name,
      source: record,
    });
  }
  return inspected;
}

async function runWebHeadMerge(
  request: Parameters<HeadMergeAdapter<ManualMergeRecord>["runMerge"]>[0],
) {
  throwIfTaskCancelled(request.signal);
  const armorPatch = await loadManualMergePatch(request.armorRecord.source, request.signal);
  const headPatch = request.helmetRecord.id === request.armorRecord.id
    ? null
    : await loadManualMergePatch(request.helmetRecord.source, request.signal);
  const result = await mergeHeadIntoArmor(armorPatch, headPatch, {
    armorHash: request.armorHash,
    helmetHash: request.helmetHash,
  }, { signal: request.signal });
  throwIfTaskCancelled(request.signal);
  const output = "hd2-head-merged-mod.zip";
  downloadZip(result.zipBlob, output);
  return { output, summary: result.summary };
}

function chooseFiles(): Promise<File[] | null> {
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.multiple = true;
    input.onchange = () => resolve(input.files ? Array.from(input.files) : null);
    input.addEventListener("cancel", () => resolve(null), { once: true });
    input.click();
  });
}
