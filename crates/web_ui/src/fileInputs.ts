import type {
  DirectoryArchiveInput,
  FileSystemDirectoryHandle,
  PatchFiles,
  TargetOption,
  WindowWithDirectoryPicker,
} from "./types";

const GPU_SUFFIX = ".gpu_resources";
const STREAM_SUFFIX = ".stream";

const TOC_HEADER_BASE = 72;
const TOC_FILE_TYPE_SIZE = 32;
const TOC_ENTRY_SIZE = 80;
const LEGACY_MAGIC = 0xf0000011;

export async function patchFilesFromList(files: FileList | File[]) {
  const values = Array.from(files);
  const toc = values.find(isTocFile);
  if (!toc) {
    throw new Error("请选择补丁 TOC 文件。");
  }
  const gpuFile = values.find((file) => file.name === `${toc.name}${GPU_SUFFIX}`);
  const streamFile = values.find((file) => file.name === `${toc.name}${STREAM_SUFFIX}`);
  const tocBytes = await fileBytes(toc);
  const gpuBytes = gpuFile ? await fileBytes(gpuFile) : new Uint8Array();
  const streamBytes = streamFile ? await fileBytes(streamFile) : new Uint8Array();
  validatePatchSidecars({
    name: toc.name,
    toc: tocBytes,
    gpuLen: gpuBytes.length,
    streamLen: streamBytes.length,
    hasGpuFile: Boolean(gpuFile),
    hasStreamFile: Boolean(streamFile),
  });
  return {
    name: toc.name,
    toc: tocBytes,
    gpu: gpuBytes,
    stream: streamBytes,
  } satisfies PatchFiles;
}

export async function metadataTextFromFile(file: File) {
  return file.text();
}

export async function archivesFromGameDirectory(targets: TargetOption[]) {
  const picker = (window as WindowWithDirectoryPicker).showDirectoryPicker;
  if (!picker) {
    throw new Error("访问目录功能需要 Chromium 内核浏览器。");
  }
  const directory = await picker();
  const archives: DirectoryArchiveInput[] = [];
  for (const target of targets) {
    const archive = await tryReadArchive(directory, target);
    if (archive) {
      archives.push(archive);
    }
  }
  if (archives.length === 0) {
    throw new Error("所选目录中未找到任何匹配的存档文件，请确认选择了正确的游戏 data 目录。");
  }
  return archives;
}

export function downloadZip(bytes: Uint8Array, filename: string) {
  const blobBytes = bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer;
  const blob = new Blob([blobBytes], { type: "application/zip" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  link.click();
  URL.revokeObjectURL(url);
}

interface PatchSidecarCheck {
  name: string;
  toc: Uint8Array;
  gpuLen: number;
  streamLen: number;
  hasGpuFile: boolean;
  hasStreamFile: boolean;
}

function validatePatchSidecars(check: PatchSidecarCheck) {
  const required = patchSidecarRequirements(check.toc);
  if (!required) {
    return;
  }
  const missing: string[] = [];
  if (required.gpu > check.gpuLen) {
    missing.push(buildSidecarHint(check.name, GPU_SUFFIX, required.gpu, check.gpuLen, check.hasGpuFile));
  }
  if (required.stream > check.streamLen) {
    missing.push(buildSidecarHint(check.name, STREAM_SUFFIX, required.stream, check.streamLen, check.hasStreamFile));
  }
  if (missing.length === 0) {
    return;
  }
  throw new Error(
    `补丁缺少必要的辅助文件：\n${missing.join("\n")}\n请在文件选择对话框中同时选中 ${check.name}、${check.name}${GPU_SUFFIX}、${check.name}${STREAM_SUFFIX}（按住 Ctrl 多选）。`,
  );
}

function buildSidecarHint(
  baseName: string,
  suffix: string,
  expected: number,
  actual: number,
  provided: boolean,
) {
  const filename = `${baseName}${suffix}`;
  if (!provided) {
    return `· 缺少 ${filename}（TOC 引用了 ${expected} 字节）`;
  }
  return `· ${filename} 大小不足：TOC 需要 ${expected} 字节，实际只有 ${actual} 字节`;
}

interface SidecarRequirements {
  gpu: number;
  stream: number;
}

function patchSidecarRequirements(toc: Uint8Array): SidecarRequirements | null {
  if (toc.length < TOC_HEADER_BASE) {
    return null;
  }
  const view = new DataView(toc.buffer, toc.byteOffset, toc.byteLength);
  if (view.getUint32(0, true) !== LEGACY_MAGIC) {
    return null;
  }
  const numTypes = view.getUint32(4, true);
  const numFiles = view.getUint32(8, true);
  const entriesStart = TOC_HEADER_BASE + numTypes * TOC_FILE_TYPE_SIZE;
  const bodiesStart = entriesStart + numFiles * TOC_ENTRY_SIZE;
  if (toc.length < bodiesStart) {
    return null;
  }
  let gpuEnd = 0;
  let streamEnd = 0;
  for (let i = 0; i < numFiles; i += 1) {
    const off = entriesStart + i * TOC_ENTRY_SIZE;
    const streamOff = Number(view.getBigUint64(off + 24, true));
    const gpuOff = Number(view.getBigUint64(off + 32, true));
    const streamSz = view.getUint32(off + 60, true);
    const gpuSz = view.getUint32(off + 64, true);
    if (gpuSz > 0) {
      gpuEnd = Math.max(gpuEnd, gpuOff + gpuSz);
    }
    if (streamSz > 0) {
      streamEnd = Math.max(streamEnd, streamOff + streamSz);
    }
  }
  return { gpu: gpuEnd, stream: streamEnd };
}

async function tryReadArchive(directory: FileSystemDirectoryHandle, target: TargetOption) {
  try {
    const handle = await directory.getFileHandle(target.hash);
    const toc = await fileBytes(await handle.getFile());
    return {
      hash: target.hash,
      name: target.name,
      toc,
    } satisfies DirectoryArchiveInput;
  } catch (error) {
    if (isNotFoundError(error)) {
      return null;
    }
    throw error;
  }
}

function isNotFoundError(error: unknown) {
  return error instanceof DOMException && error.name === "NotFoundError";
}

function isTocFile(file: File) {
  return !file.name.endsWith(GPU_SUFFIX) && !file.name.endsWith(STREAM_SUFFIX);
}

async function fileBytes(file: File) {
  return new Uint8Array(await file.arrayBuffer());
}
