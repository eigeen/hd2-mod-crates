export type MergeFileRole = "toc" | "gpu" | "stream";

export interface ManualMergeRecord {
  id: string;
  name: string;
  toc?: File;
  gpu?: File;
  stream?: File;
}

export type ManualMergeInputErrorCode = "empty" | "unsupported" | "missingMain" | "duplicate";

export class ManualMergeInputError extends Error {
  constructor(
    readonly code: ManualMergeInputErrorCode,
    readonly filename = "",
  ) {
    super(code);
    this.name = "ManualMergeInputError";
  }
}

interface ClassifiedMergeFile {
  name: string;
  role: MergeFileRole;
  file: File;
}

/** Validate one submission as an isolated batch, then append all of its patch groups. */
export function appendManualMergeBatch(
  current: ManualMergeRecord[],
  files: FileList | File[],
  createId: () => string = () => crypto.randomUUID(),
): ManualMergeRecord[] {
  const batch = buildBatchRecords(Array.from(files), createId);
  return [...current, ...batch];
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

function classifyMergeFile(file: File): ClassifiedMergeFile {
  const lower = file.name.toLowerCase();
  if (lower.endsWith(".gpu_resources")) {
    return { name: file.name.slice(0, -".gpu_resources".length), role: "gpu", file };
  }
  if (lower.endsWith(".stream")) {
    return { name: file.name.slice(0, -".stream".length), role: "stream", file };
  }
  if (!isPatchMainName(file.name)) {
    throw new ManualMergeInputError("unsupported", file.name);
  }
  return { name: file.name, role: "toc", file };
}

function isPatchMainName(name: string) {
  return /^[0-9a-f]{16}(?:\.patch(?:_\d+)?)?$/i.test(name)
    || /\.patch(?:_\d+)?$/i.test(name)
    || /\.toc$/i.test(name);
}

function buildBatchRecords(
  files: File[],
  createId: () => string,
) {
  if (files.length === 0) throw new ManualMergeInputError("empty");
  const records = new Map<string, ManualMergeRecord>();
  for (const file of files) addBatchFile(records, classifyMergeFile(file), createId);
  const result = [...records.values()];
  const orphan = result.find((record) => !record.toc);
  if (orphan) {
    throw new ManualMergeInputError("missingMain", orphan.name);
  }
  return result;
}

function addBatchFile(
  records: Map<string, ManualMergeRecord>,
  classified: ClassifiedMergeFile,
  createId: () => string,
) {
  const key = classified.name.toLowerCase();
  const record = records.get(key) ?? { id: createId(), name: classified.name };
  if (record[classified.role]) {
    throw new ManualMergeInputError("duplicate", classified.file.name);
  }
  record[classified.role] = classified.file;
  records.set(key, record);
}
