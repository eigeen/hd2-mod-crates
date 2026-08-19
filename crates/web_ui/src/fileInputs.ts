import type { PatchFiles } from "@hd2-mod-tools/migrator-ui";

const GPU_SUFFIX = ".gpu_resources";
const STREAM_SUFFIX = ".stream";

export interface PatchFileMessages {
  noToc: string;
  multipleToc: string;
  missingIntro: string;
  missingAction: (toc: string, gpu: string, stream: string) => string;
  missingSidecar: (filename: string, expected: string) => string;
  shortSidecar: (filename: string, expected: string, actual: number) => string;
}

export interface PatchSidecarRequirements {
  gpu: string;
  stream: string;
}

export type ReadPatchSidecarRequirements = (
  toc: Uint8Array,
) => Promise<PatchSidecarRequirements>;

export async function patchFilesFromList(
  files: FileList | File[],
  messages: PatchFileMessages,
  readRequirements: ReadPatchSidecarRequirements,
  originalName?: string,
) {
  const values = Array.from(files);
  const toc = selectTocFile(values, messages);
  const tocBytes = await fileBytes(toc);
  const required = await readRequirements(tocBytes);
  const loaded = await loadPatchSelection(values, toc, tocBytes, originalName);
  validatePatchFiles(loaded.patch, loaded.presence, required, messages);
  return loaded.patch;
}

function selectTocFile(values: File[], messages: PatchFileMessages): File {
  const tocFiles = values.filter(isTocFile);
  if (tocFiles.length === 0) {
    throw new Error(messages.noToc);
  }
  if (tocFiles.length > 1) {
    throw new Error(messages.multipleToc);
  }
  return tocFiles[0];
}

async function loadPatchSelection(
  values: File[],
  toc: File,
  tocBytes: Uint8Array,
  originalName?: string,
) {
  const gpuFile = values.find((file) => file.name === `${toc.name}${GPU_SUFFIX}`);
  const streamFile = values.find((file) => file.name === `${toc.name}${STREAM_SUFFIX}`);
  return {
    patch: {
      name: toc.name,
      originalName: originalName ?? originalNameFromRelativePath(toc),
      toc: tocBytes,
      gpu: gpuFile ? await fileBytes(gpuFile) : new Uint8Array(),
      stream: streamFile ? await fileBytes(streamFile) : new Uint8Array(),
    } satisfies PatchFiles,
    presence: {
      hasGpuFile: Boolean(gpuFile),
      hasStreamFile: Boolean(streamFile),
    },
  };
}

function originalNameFromRelativePath(file: File): string | undefined {
  const [root, child] = (file.webkitRelativePath ?? "").split(/[\\/]/);
  return root && child ? root : undefined;
}

/** Validate loaded sidecars against requirements calculated by the shared Rust core. */
export function validatePatchFiles(
  patch: PatchFiles,
  presence: { hasGpuFile: boolean; hasStreamFile: boolean },
  required: PatchSidecarRequirements,
  messages: PatchFileMessages,
) {
  validatePatchSidecars({
    name: patch.name,
    gpuLen: patch.gpu.length,
    streamLen: patch.stream.length,
    hasGpuFile: presence.hasGpuFile,
    hasStreamFile: presence.hasStreamFile,
    required,
    messages,
  });
}

export function downloadZip(blob: Blob, filename: string) {
  downloadBlob(blob, filename);
}

export function downloadRepatchedPatch(
  patch: PatchFiles,
  tocBytes: Uint8Array,
  filename: string,
) {
  downloadBlob(buildRepatchedPatchZip(patch, tocBytes), filename);
}

export function buildRepatchedPatchZip(
  patch: PatchFiles,
  tocBytes: Uint8Array,
): Blob {
  const entries = [
    { name: patch.name, bytes: tocBytes },
    { name: `${patch.name}${GPU_SUFFIX}`, bytes: patch.gpu },
    { name: `${patch.name}${STREAM_SUFFIX}`, bytes: patch.stream },
  ];
  return storeZip(entries);
}

interface StoreZipEntry {
  name: string;
  bytes: Uint8Array;
}

function storeZip(entries: StoreZipEntry[]): Blob {
  const builder = new StoreZipBuilder();
  for (const entry of entries) {
    builder.add(entry.name, entry.bytes);
  }
  return builder.finish();
}

/** Store output files as Blob-backed ZIP pieces so completed WASM buffers can be released. */
export class StoreZipBuilder {
  private readonly fileParts: Blob[] = [];
  private readonly centralParts: ArrayBuffer[] = [];
  private offset = 0;
  private centralSize = 0;

  add(name: string, bytes: Uint8Array, precomputedCrc?: number): void {
    const encodedName = new TextEncoder().encode(name);
    const crc = precomputedCrc ?? crc32(bytes);
    const local = localZipHeader(encodedName, bytes.length, crc);
    const central = centralZipHeader(encodedName, bytes.length, crc, this.offset);
    this.fileParts.push(new Blob([exactBuffer(local), exactBuffer(bytes)]));
    this.centralParts.push(exactBuffer(central));
    this.offset += local.length + bytes.length;
    this.centralSize += central.length;
  }

  finish(): Blob {
    const end = endOfCentralDirectory(
      this.centralParts.length,
      this.centralSize,
      this.offset,
    );
    return new Blob(
      [...this.fileParts, ...this.centralParts, exactBuffer(end)],
      { type: "application/zip" },
    );
  }
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
  gpuLen: number;
  streamLen: number;
  hasGpuFile: boolean;
  hasStreamFile: boolean;
  required: PatchSidecarRequirements;
  messages: PatchFileMessages;
}

function validatePatchSidecars(check: PatchSidecarCheck) {
  const missing: string[] = [];
  if (BigInt(check.required.gpu) > BigInt(check.gpuLen)) {
    missing.push(buildSidecarHint(check, {
      suffix: GPU_SUFFIX,
      expected: check.required.gpu,
      actual: check.gpuLen,
      provided: check.hasGpuFile,
    }));
  }
  if (BigInt(check.required.stream) > BigInt(check.streamLen)) {
    missing.push(buildSidecarHint(check, {
      suffix: STREAM_SUFFIX,
      expected: check.required.stream,
      actual: check.streamLen,
      provided: check.hasStreamFile,
    }));
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

interface SidecarHintInput {
  suffix: string;
  expected: string;
  actual: number;
  provided: boolean;
}

function buildSidecarHint(check: PatchSidecarCheck, input: SidecarHintInput) {
  const filename = `${check.name}${input.suffix}`;
  if (!input.provided) {
    return check.messages.missingSidecar(filename, input.expected);
  }
  return check.messages.shortSidecar(filename, input.expected, input.actual);
}

function isTocFile(file: File) {
  return !file.name.endsWith(GPU_SUFFIX) && !file.name.endsWith(STREAM_SUFFIX);
}

async function fileBytes(file: File) {
  return new Uint8Array(await file.arrayBuffer());
}
