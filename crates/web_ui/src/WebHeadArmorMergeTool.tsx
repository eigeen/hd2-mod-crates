import { useMemo } from "react";
import {
  HeadArmorMergeTool,
  type HeadMergeAdapter,
  type HeadMergeRecord,
  type MergeInputFile,
} from "@hd2-mod-tools/migrator-app";
import {
  headMergeCandidates,
  throwIfTaskCancelled,
  useI18n,
  type PatchFiles,
} from "@hd2-mod-tools/migrator-ui";
import { downloadZip } from "./fileInputs";
import {
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
    pickRecords: async () => {
      const files = await chooseFiles();
      return files ? createRecords(files.map(browserFileReference), t) : null;
    },
    recordsFromFiles: async (files) => createRecords(files, t),
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
): Promise<HeadMergeRecord<ManualMergeRecord>[]> {
  let records: ManualMergeRecord[];
  try {
    records = recordsFromMergeInputFiles(files);
    await validateManualMergeRecords(records, t, patchSidecarRequirements);
  } catch (error) {
    throw new Error(mergeInputErrorMessage(error, t), { cause: error });
  }
  return Promise.all(records.map(async (record) => {
    const inspection = await inspectEquipmentContents(await readRecordToc(record));
    return {
      armors: headMergeCandidates(inspection.sources, "Armor"),
      helmets: headMergeCandidates(inspection.sources, "Helmet"),
      id: record.id,
      name: record.name,
      source: record,
    };
  }));
}

async function runWebHeadMerge(
  request: Parameters<HeadMergeAdapter<ManualMergeRecord>["runMerge"]>[0],
) {
  throwIfTaskCancelled(request.signal);
  const armorPatch = await readRecordPatch(request.armorRecord.source);
  const headPatch = request.helmetRecord.id === request.armorRecord.id
    ? null
    : await readRecordPatch(request.helmetRecord.source);
  const result = await mergeHeadIntoArmor(armorPatch, headPatch, {
    armorHash: request.armorHash,
    helmetHash: request.helmetHash,
  }, { signal: request.signal });
  throwIfTaskCancelled(request.signal);
  const output = "hd2-head-merged-mod.zip";
  downloadZip(result.zipBlob, output);
  return { output, summary: result.summary };
}

async function readRecordToc(record: ManualMergeRecord): Promise<PatchFiles> {
  if (!record.toc) throw new Error(`Missing patch main file: ${record.name}`);
  return {
    name: record.name,
    toc: await fileBytes(record.toc),
    gpu: new Uint8Array(),
    stream: new Uint8Array(),
  };
}

async function readRecordPatch(record: ManualMergeRecord): Promise<PatchFiles> {
  const patch = await readRecordToc(record);
  patch.gpu = record.gpu ? await fileBytes(record.gpu) : new Uint8Array();
  patch.stream = record.stream ? await fileBytes(record.stream) : new Uint8Array();
  return patch;
}

function fileBytes(file: File): Promise<Uint8Array> {
  return file.arrayBuffer().then((buffer) => new Uint8Array(buffer));
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
