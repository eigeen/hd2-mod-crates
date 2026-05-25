import type { PatchFiles } from "./types";

const GPU_SUFFIX = ".gpu_resources";
const STREAM_SUFFIX = ".stream";

const TOC_HEADER_BASE = 72;
const TOC_FILE_TYPE_SIZE = 32;
const TOC_ENTRY_SIZE = 80;
const LEGACY_MAGIC = 0xf0000011;

export interface PatchFileMessages {
  noToc: string;
  missingIntro: string;
  missingAction: (toc: string, gpu: string, stream: string) => string;
  missingSidecar: (filename: string, expected: number) => string;
  shortSidecar: (filename: string, expected: number, actual: number) => string;
}

export async function patchFilesFromList(
  files: FileList | File[],
  messages: PatchFileMessages,
) {
  const values = Array.from(files);
  const toc = values.find(isTocFile);
  if (!toc) {
    throw new Error(messages.noToc);
  }
  const gpuFile = values.find((file) => file.name === `${toc.name}${GPU_SUFFIX}`);
  const streamFile = values.find((file) => file.name === `${toc.name}${STREAM_SUFFIX}`);
  const tocBytes = await fileBytes(toc);
  const gpuBytes = gpuFile ? await fileBytes(gpuFile) : new Uint8Array();
  const streamBytes = streamFile ? await fileBytes(streamFile) : new Uint8Array();
  const patch: PatchFiles = {
    name: toc.name,
    toc: tocBytes,
    gpu: gpuBytes,
    stream: streamBytes,
  };
  validatePatchFiles(patch, {
    hasGpuFile: Boolean(gpuFile),
    hasStreamFile: Boolean(streamFile),
    messages,
  });
  return patch;
}

// 校验已加载的 PatchFiles 是否满足 TOC 引用的 sidecar 尺寸；任何路径加载后都应调用。
export function validatePatchFiles(
  patch: PatchFiles,
  presence: { hasGpuFile: boolean; hasStreamFile: boolean; messages: PatchFileMessages },
) {
  validatePatchSidecars({
    name: patch.name,
    toc: patch.toc,
    gpuLen: patch.gpu.length,
    streamLen: patch.stream.length,
    hasGpuFile: presence.hasGpuFile,
    hasStreamFile: presence.hasStreamFile,
    messages: presence.messages,
  });
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
  messages: PatchFileMessages;
}

function validatePatchSidecars(check: PatchSidecarCheck) {
  const required = patchSidecarRequirements(check.toc);
  if (!required) {
    return;
  }
  const missing: string[] = [];
  if (required.gpu > check.gpuLen) {
    missing.push(buildSidecarHint(check.name, GPU_SUFFIX, required.gpu, check.gpuLen, check.hasGpuFile, check.messages));
  }
  if (required.stream > check.streamLen) {
    missing.push(buildSidecarHint(check.name, STREAM_SUFFIX, required.stream, check.streamLen, check.hasStreamFile, check.messages));
  }
  if (missing.length === 0) {
    return;
  }
  const gpuName = `${check.name}${GPU_SUFFIX}`;
  const streamName = `${check.name}${STREAM_SUFFIX}`;
  throw new Error(
    `${check.messages.missingIntro}\n${missing.join("\n")}\n${check.messages.missingAction(check.name, gpuName, streamName)}`,
  );
}

function buildSidecarHint(
  baseName: string,
  suffix: string,
  expected: number,
  actual: number,
  provided: boolean,
  messages: PatchFileMessages,
) {
  const filename = `${baseName}${suffix}`;
  if (!provided) {
    return messages.missingSidecar(filename, expected);
  }
  return messages.shortSidecar(filename, expected, actual);
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

function isTocFile(file: File) {
  return !file.name.endsWith(GPU_SUFFIX) && !file.name.endsWith(STREAM_SUFFIX);
}

async function fileBytes(file: File) {
  return new Uint8Array(await file.arrayBuffer());
}
