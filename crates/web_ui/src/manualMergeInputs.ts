import {
  MergeInputError,
  groupMergeInputFiles,
  type GroupedMergeInput,
  type MergeInputFile,
} from "@hd2-mod-tools/migrator-app";
import { TaskError, throwIfTaskCancelled, type PatchFiles, type Translate } from "@hd2-mod-tools/migrator-ui";
import {
  loadPatchFiles,
  validatePatchSidecarSizes,
  type PatchFileMessages,
  type ReadPatchSidecarRequirements,
} from "./fileInputs";

export type ManualMergeInputErrorCode = "empty" | "unsupported" | "missingMain" | "duplicate";
export { MergeInputError as ManualMergeInputError };

export interface ManualMergeRecord {
  id: string;
  name: string;
  toc?: File;
  gpu?: File;
  stream?: File;
}

/** Read one record's metadata without materializing sidecar contents. */
export async function readManualMergeToc(record: ManualMergeRecord, signal?: AbortSignal) {
  if (signal) throwIfTaskCancelled(signal);
  if (!record.toc) throw new MergeInputError("missingMain", record.name);
  const toc = new Uint8Array(await record.toc.arrayBuffer());
  if (signal) throwIfTaskCancelled(signal);
  return { name: record.name, toc };
}

/** Execute with the same ordered, cancellable sidecar reads used by migration. */
export async function loadManualMergePatch(record: ManualMergeRecord, signal: AbortSignal): Promise<PatchFiles> {
  const metadata = await readManualMergeToc(record, signal);
  return loadPatchFiles({ ...metadata, gpu: record.gpu, stream: record.stream }, signal);
}

/** Localize a merge input validation error for display. */
export function mergeInputErrorMessage(error: unknown, t: Translate): string {
  if (!(error instanceof MergeInputError)) {
    return error instanceof Error ? error.message : String(error);
  }
  if (error.code === "empty") return t("merge.errorEmpty");
  if (error.code === "unsupported") return t("merge.errorUnsupported", { filename: error.filename });
  if (error.code === "duplicate") return t("merge.errorDuplicate", { filename: error.filename });
  return t("merge.errorMissingMain");
}

/** Validate one submission as an isolated batch, then append all of its patch groups. */
export function appendManualMergeBatch(
  current: ManualMergeRecord[],
  files: FileList | File[],
  createId: () => string = () => crypto.randomUUID(),
): ManualMergeRecord[] {
  return [...current, ...recordsFromMergeInputFiles(
    Array.from(files).map(browserMergeFile),
    createId,
  )];
}

export function recordsFromMergeInputFiles(
  files: MergeInputFile<File>[],
  createId: () => string = () => crypto.randomUUID(),
): ManualMergeRecord[] {
  return groupMergeInputFiles(files, createId).map(webMergeRecord);
}

/** Validate TOC requirements using sidecar metadata without loading large sidecar files. */
export async function validateManualMergeRecords(
  records: ManualMergeRecord[],
  t: Translate,
  readRequirements: ReadPatchSidecarRequirements,
  context: { signal?: AbortSignal } = {},
): Promise<void> {
  try {
    const messages = patchFileMessages(t);
    for (const record of records) {
      if (context.signal) throwIfTaskCancelled(context.signal);
      await validateManualMergeRecord(record, messages, readRequirements, context.signal);
    }
    if (context.signal) throwIfTaskCancelled(context.signal);
  } catch (error) {
    if (error instanceof TaskError) throw error;
    const message = error instanceof Error ? error.message : String(error);
    throw new TaskError("patch.inspectFailed", message);
  }
}

async function validateManualMergeRecord(
  record: ManualMergeRecord,
  messages: PatchFileMessages,
  readRequirements: ReadPatchSidecarRequirements,
  signal?: AbortSignal,
): Promise<void> {
  const { toc } = await readManualMergeToc(record, signal);
  const required = await readRequirements(
    toc,
    record.stream?.size ?? 0,
  );
  validatePatchSidecarSizes({
    gpuSize: record.gpu?.size ?? 0,
    hasGpuFile: Boolean(record.gpu),
    hasStreamFile: Boolean(record.stream),
    messages,
    name: record.name,
    required,
    streamSize: record.stream?.size ?? 0,
  });
}

export function moveManualMergeRecord(
  current: ManualMergeRecord[],
  draggedId: string,
  targetId: string,
) {
  const from = current.findIndex((record) => record.id === draggedId);
  const to = current.findIndex((record) => record.id === targetId);
  if (from < 0 || to < 0 || from === to) return current;
  const next = [...current];
  const [dragged] = next.splice(from, 1);
  next.splice(to, 0, dragged);
  return next;
}

function browserMergeFile(file: File) {
  return {
    byteLength: file.size,
    name: file.name,
    relativePath: file.webkitRelativePath || file.name,
    source: file,
  };
}

function webMergeRecord(group: GroupedMergeInput<File>): ManualMergeRecord {
  return {
    gpu: group.gpu?.source,
    id: group.id,
    name: group.name,
    stream: group.stream?.source,
    toc: group.toc.source,
  };
}

function patchFileMessages(t: Translate): PatchFileMessages {
  return {
    missingAction: (toc, gpu, stream) => t("patch.missingAction", { toc, gpu, stream }),
    missingIntro: t("patch.missingIntro"),
    missingSidecar: (filename, expected) => t("patch.missingSidecar", { filename, expected }),
    multipleToc: t("patch.multipleToc"),
    noToc: t("patch.noToc"),
    shortSidecar: (filename, expected, actual) => t("patch.shortSidecar", { filename, expected, actual }),
  };
}
