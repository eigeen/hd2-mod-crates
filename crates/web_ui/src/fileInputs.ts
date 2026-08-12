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
  downloadBlob(new Blob([exactBuffer(bytes)], { type: "application/zip" }), filename);
}

export function downloadRepatchedPatch(
  patch: PatchFiles,
  tocBytes: Uint8Array,
  filename: string,
) {
  const entries = [
    { name: patch.name, bytes: tocBytes },
    { name: `${patch.name}${GPU_SUFFIX}`, bytes: patch.gpu },
    { name: `${patch.name}${STREAM_SUFFIX}`, bytes: patch.stream },
  ].filter((entry) => entry.bytes.length > 0 || entry.name === patch.name);
  const blob = storeZip(entries);
  downloadBlob(blob, filename);
}

interface StoreZipEntry {
  name: string;
  bytes: Uint8Array;
}

function storeZip(entries: StoreZipEntry[]): Blob {
  const parts: BlobPart[] = [];
  const centralParts: BlobPart[] = [];
  let offset = 0;
  let centralSize = 0;
  for (const entry of entries) {
    const encodedName = new TextEncoder().encode(entry.name);
    const crc = crc32(entry.bytes);
    const local = localZipHeader(encodedName, entry.bytes.length, crc);
    const central = centralZipHeader(encodedName, entry.bytes.length, crc, offset);
    parts.push(exactBuffer(local), exactBuffer(entry.bytes));
    centralParts.push(exactBuffer(central));
    offset += local.length + entry.bytes.length;
    centralSize += central.length;
  }
  parts.push(...centralParts, exactBuffer(endOfCentralDirectory(entries.length, centralSize, offset)));
  return new Blob(parts, { type: "application/zip" });
}

function localZipHeader(name: Uint8Array, size: number, crc: number): Uint8Array {
  ensureZip32(size);
  const header = new Uint8Array(30 + name.length);
  const view = new DataView(header.buffer);
  view.setUint32(0, 0x04034b50, true);
  view.setUint16(4, 20, true);
  view.setUint32(14, crc, true);
  view.setUint32(18, size, true);
  view.setUint32(22, size, true);
  view.setUint16(26, name.length, true);
  header.set(name, 30);
  return header;
}

function centralZipHeader(name: Uint8Array, size: number, crc: number, offset: number): Uint8Array {
  ensureZip32(offset);
  const header = new Uint8Array(46 + name.length);
  const view = new DataView(header.buffer);
  view.setUint32(0, 0x02014b50, true);
  view.setUint16(4, 20, true);
  view.setUint16(6, 20, true);
  view.setUint32(16, crc, true);
  view.setUint32(20, size, true);
  view.setUint32(24, size, true);
  view.setUint16(28, name.length, true);
  view.setUint32(42, offset, true);
  header.set(name, 46);
  return header;
}

function endOfCentralDirectory(count: number, size: number, offset: number): Uint8Array {
  ensureZip32(size);
  ensureZip32(offset);
  if (count > 0xffff) throw new Error("ZIP entry count exceeds the ZIP32 limit.");
  const header = new Uint8Array(22);
  const view = new DataView(header.buffer);
  view.setUint32(0, 0x06054b50, true);
  view.setUint16(8, count, true);
  view.setUint16(10, count, true);
  view.setUint32(12, size, true);
  view.setUint32(16, offset, true);
  return header;
}

const CRC32_TABLE = buildCrc32Table();

function crc32(bytes: Uint8Array): number {
  let crc = 0xffffffff;
  for (let index = 0; index < bytes.length; index += 1) {
    crc = CRC32_TABLE[(crc ^ bytes[index]) & 0xff] ^ (crc >>> 8);
  }
  return (crc ^ 0xffffffff) >>> 0;
}

function buildCrc32Table(): Uint32Array {
  const table = new Uint32Array(256);
  for (let index = 0; index < table.length; index += 1) {
    let value = index;
    for (let bit = 0; bit < 8; bit += 1) {
      value = (value & 1) ? 0xedb88320 ^ (value >>> 1) : value >>> 1;
    }
    table[index] = value >>> 0;
  }
  return table;
}

function exactBuffer(bytes: Uint8Array): ArrayBuffer {
  if (bytes.byteOffset === 0 && bytes.byteLength === bytes.buffer.byteLength) {
    return bytes.buffer as ArrayBuffer;
  }
  return bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer;
}

function ensureZip32(value: number): void {
  if (!Number.isSafeInteger(value) || value < 0 || value > 0xffffffff) {
    throw new Error("Patch is too large for ZIP32 output.");
  }
}

function downloadBlob(blob: Blob, filename: string): void {
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  link.click();
  window.setTimeout(() => URL.revokeObjectURL(url), 0);
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
