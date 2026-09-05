export type MergeFileRole = "toc" | "gpu" | "stream";

export interface MergeInputFile<Source> {
  byteLength?: number;
  name: string;
  relativePath: string;
  source: Source;
}

export interface GroupedMergeInput<Source> {
  gpu?: MergeInputFile<Source>;
  id: string;
  name: string;
  stream?: MergeInputFile<Source>;
  toc: MergeInputFile<Source>;
}

export type MergeInputErrorCode = "empty" | "unsupported" | "missingMain" | "duplicate";

export class MergeInputError extends Error {
  constructor(readonly code: MergeInputErrorCode, readonly filename = "") {
    super(code);
    this.name = "MergeInputError";
  }
}

interface ClassifiedMergeFile<Source> {
  baseName: string;
  file: MergeInputFile<Source>;
  role: MergeFileRole;
}

/** Group one file submission by directory and case-insensitive patch basename. */
export function groupMergeInputFiles<Source>(
  files: MergeInputFile<Source>[],
  createId: () => string = () => crypto.randomUUID(),
): GroupedMergeInput<Source>[] {
  if (!files.length) throw new MergeInputError("empty");
  const groups = new Map<string, Partial<GroupedMergeInput<Source>>>();
  for (const file of files) addFileToGroup(groups, classifyMergeFile(file), createId);
  return [...groups.values()].map(requireMainFile);
}

function classifyMergeFile<Source>(file: MergeInputFile<Source>): ClassifiedMergeFile<Source> {
  const lower = file.name.toLowerCase();
  if (lower.endsWith(".gpu_resources")) {
    return { baseName: file.name.slice(0, -".gpu_resources".length), role: "gpu", file };
  }
  if (lower.endsWith(".stream")) {
    return { baseName: file.name.slice(0, -".stream".length), role: "stream", file };
  }
  if (!isPatchMainName(file.name)) throw new MergeInputError("unsupported", file.name);
  return { baseName: file.name, role: "toc", file };
}

function addFileToGroup<Source>(
  groups: Map<string, Partial<GroupedMergeInput<Source>>>,
  classified: ClassifiedMergeFile<Source>,
  createId: () => string,
): void {
  const key = `${parentPath(classified.file.relativePath)}\0${classified.baseName.toLowerCase()}`;
  const group = groups.get(key) ?? { id: createId(), name: classified.baseName };
  if (group[classified.role]) throw new MergeInputError("duplicate", classified.file.name);
  group[classified.role] = classified.file;
  groups.set(key, group);
}

function requireMainFile<Source>(group: Partial<GroupedMergeInput<Source>>): GroupedMergeInput<Source> {
  if (!group.toc) throw new MergeInputError("missingMain", group.name);
  return group as GroupedMergeInput<Source>;
}

function parentPath(path: string): string {
  const normalized = path.replace(/\\/g, "/");
  const separator = normalized.lastIndexOf("/");
  return separator < 0 ? "" : normalized.slice(0, separator);
}

function isPatchMainName(name: string): boolean {
  return /^[0-9a-f]{16}(?:\.patch(?:_\d+)?)?$/i.test(name)
    || /\.patch(?:_\d+)?$/i.test(name)
    || /\.toc$/i.test(name);
}

export function isSupportedMergeInputName(name: string): boolean {
  const lower = name.toLowerCase();
  return lower.endsWith(".gpu_resources") || lower.endsWith(".stream") || isPatchMainName(name);
}
