import { useMemo } from "react";
import {
  ManualMergeTool,
  type ManualMergeAdapter,
  type MergeInputFile,
  type ManualMergeRecord,
} from "@hd2-mod-tools/migrator-app";
import {
  TaskError,
  throwIfTaskCancelled,
  useI18n,
  type PatchFiles,
} from "@hd2-mod-tools/migrator-ui";
import { downloadZip } from "./fileInputs";
import {
  loadManualMergePatch,
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
    pickRecords: async (signal) => {
      const files = await chooseFiles();
      throwIfTaskCancelled(signal);
      return files ? createRecordsFromInputs(files.map(browserFileReference), t, signal) : null;
    },
    recordsFromFiles: async (files, signal) => createRecordsFromInputs(files, t, signal),
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
  const inputs: PatchFiles[] = [];
  for (const record of request.records) {
    inputs.push(await loadManualMergePatch(record.source, request.signal));
  }
  const result = await mergePatchesToZip(
    inputs,
    request.outputName,
    { signal: request.signal },
  );
  throwIfTaskCancelled(request.signal);
  const output = "hd2-merged-mod.zip";
  downloadZip(result.zipBlob, output);
  return { output, summary: result.summary };
}

async function createRecordsFromInputs(
  files: MergeInputFile<File>[],
  t: ReturnType<typeof useI18n>["t"],
  signal: AbortSignal,
): Promise<ManualMergeRecord<WebMergeRecord>[]> {
  try {
    const records = recordsFromMergeInputFiles(files);
    await validateManualMergeRecords(records, t,
      (toc, streamLength) => patchSidecarRequirements(toc, streamLength, { signal }), { signal });
    return records.map(toManualMergeRecord);
  } catch (error) {
    if (error instanceof TaskError) throw error;
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
