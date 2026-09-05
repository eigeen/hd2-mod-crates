import { useMemo } from "react";
import {
  ManualMergeTool,
  type ManualMergeAdapter,
  type ManualMergeRecord,
} from "@hd2-mod-tools/migrator-app";
import {
  throwIfTaskCancelled,
  useI18n,
  type PatchFiles,
} from "@hd2-mod-tools/migrator-ui";
import { downloadRepatchedPatch } from "./fileInputs";
import {
  appendManualMergeBatch,
  mergeInputErrorMessage,
  type ManualMergeRecord as WebMergeRecord,
} from "./manualMergeInputs";
import { mergePatches } from "./wasmClient";

export function WebManualMergeTool() {
  const { t } = useI18n();
  const adapter = useMemo<ManualMergeAdapter<WebMergeRecord>>(() => ({
    cancellable: false,
    pickRecords: async () => {
      const files = await chooseFiles();
      return files ? createRecords(files, t) : null;
    },
    recordsFromDrop: async (transfer) => createRecords(Array.from(transfer.files), t),
    runMerge: runWebMerge,
  }), [t]);
  return <ManualMergeTool adapter={adapter} />;
}

async function runWebMerge(request: Parameters<ManualMergeAdapter<WebMergeRecord>["runMerge"]>[0]) {
  throwIfTaskCancelled(request.signal);
  const result = await mergePatches(
    await Promise.all(request.records.map((record) => readPatchInput(record.source))),
    request.outputName,
  );
  throwIfTaskCancelled(request.signal);
  const output = "hd2-merged-mod.zip";
  downloadRepatchedPatch(result.patch, result.patch.toc, output);
  return { output, summary: result.summary };
}

function createRecords(
  files: File[],
  t: ReturnType<typeof useI18n>["t"],
): ManualMergeRecord<WebMergeRecord>[] {
  try {
    return appendManualMergeBatch([], files).map((record) => ({
      fileRoles: {
        gpu: Boolean(record.gpu),
        stream: Boolean(record.stream),
        toc: Boolean(record.toc),
      },
      id: record.id,
      name: record.name,
      source: record,
    }));
  } catch (error) {
    throw new Error(mergeInputErrorMessage(error, t), { cause: error });
  }
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
