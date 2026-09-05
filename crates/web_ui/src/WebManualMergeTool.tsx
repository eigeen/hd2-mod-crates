import { useMemo } from "react";
import {
  ManualMergeTool,
  type ManualMergeAdapter,
  type MergeInputFile,
  type ManualMergeRecord,
} from "@hd2-mod-tools/migrator-app";
import {
  throwIfTaskCancelled,
  useI18n,
  type PatchFiles,
} from "@hd2-mod-tools/migrator-ui";
import { downloadZip } from "./fileInputs";
import {
  appendManualMergeBatch,
  mergeInputErrorMessage,
  recordsFromMergeInputFiles,
  validateManualMergeRecords,
  type ManualMergeRecord as WebMergeRecord,
} from "./manualMergeInputs";
import { mergePatchesToZip, patchSidecarRequirements } from "./wasmClient";

export function WebManualMergeTool() {
  const { t } = useI18n();
  const adapter = useMemo<ManualMergeAdapter<WebMergeRecord, File>>(() => ({
    browserFileReference,
    cancellable: true,
    pickRecords: async () => {
      const files = await chooseFiles();
      return files ? createRecords(files, t) : null;
    },
    recordsFromFiles: async (files) => createRecordsFromInputs(files, t),
    runMerge: runWebMerge,
  }), [t]);
  return <ManualMergeTool adapter={adapter} />;
}

function browserFileReference(file: File): MergeInputFile<File> {
  return {
    byteLength: file.size,
    name: file.name,
    relativePath: file.webkitRelativePath || file.name,
    source: file,
  };
}

async function runWebMerge(request: Parameters<ManualMergeAdapter<WebMergeRecord>["runMerge"]>[0]) {
  throwIfTaskCancelled(request.signal);
  const result = await mergePatchesToZip(
    await Promise.all(request.records.map((record) => readPatchInput(record.source))),
    request.outputName,
    { signal: request.signal },
  );
  throwIfTaskCancelled(request.signal);
  const output = "hd2-merged-mod.zip";
  downloadZip(result.zipBlob, output);
  return { output, summary: result.summary };
}

async function createRecords(
  files: File[],
  t: ReturnType<typeof useI18n>["t"],
): Promise<ManualMergeRecord<WebMergeRecord>[]> {
  try {
    const records = appendManualMergeBatch([], files);
    await validateManualMergeRecords(records, t, patchSidecarRequirements);
    return records.map(toManualMergeRecord);
  } catch (error) {
    throw new Error(mergeInputErrorMessage(error, t), { cause: error });
  }
}

async function createRecordsFromInputs(
  files: MergeInputFile<File>[],
  t: ReturnType<typeof useI18n>["t"],
): Promise<ManualMergeRecord<WebMergeRecord>[]> {
  try {
    const records = recordsFromMergeInputFiles(files);
    await validateManualMergeRecords(records, t, patchSidecarRequirements);
    return records.map(toManualMergeRecord);
  } catch (error) {
    throw new Error(mergeInputErrorMessage(error, t), { cause: error });
  }
}

function toManualMergeRecord(record: WebMergeRecord): ManualMergeRecord<WebMergeRecord> {
  return {
    fileRoles: { gpu: Boolean(record.gpu), stream: Boolean(record.stream), toc: Boolean(record.toc) },
    id: record.id,
    name: record.name,
    source: record,
  };
}

async function readPatchInput(record: WebMergeRecord): Promise<PatchFiles> {
  if (!record.toc) throw new Error(`Missing patch main file: ${record.name}`);
  return {
    name: record.name,
    toc: await fileBytes(record.toc),
    gpu: record.gpu ? await fileBytes(record.gpu) : new Uint8Array(),
    stream: record.stream ? await fileBytes(record.stream) : new Uint8Array(),
  };
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
