import type {
  DirectoryArchiveInput,
  FileSystemDirectoryHandle,
  PatchFiles,
  TargetOption,
  WindowWithDirectoryPicker,
} from "./types";

const GPU_SUFFIX = ".gpu_resources";
const STREAM_SUFFIX = ".stream";

export async function patchFilesFromList(files: FileList | File[]) {
  const values = Array.from(files);
  const toc = values.find(isTocFile);
  if (!toc) {
    throw new Error("请选择补丁 TOC 文件。");
  }
  const gpu = values.find((file) => file.name === `${toc.name}${GPU_SUFFIX}`);
  const stream = values.find((file) => file.name === `${toc.name}${STREAM_SUFFIX}`);
  return {
    name: toc.name,
    toc: await fileBytes(toc),
    gpu: gpu ? await fileBytes(gpu) : new Uint8Array(),
    stream: stream ? await fileBytes(stream) : new Uint8Array(),
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
    archives.push(await readArchive(directory, target));
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

async function readArchive(directory: FileSystemDirectoryHandle, target: TargetOption) {
  const toc = await readNamedFile(directory, target.hash);
  return {
    hash: target.hash,
    name: target.name,
    toc,
  } satisfies DirectoryArchiveInput;
}

async function readNamedFile(directory: FileSystemDirectoryHandle, name: string) {
  const handle = await directory.getFileHandle(name);
  return fileBytes(await handle.getFile());
}

function isTocFile(file: File) {
  return !file.name.endsWith(GPU_SUFFIX) && !file.name.endsWith(STREAM_SUFFIX);
}

async function fileBytes(file: File) {
  return new Uint8Array(await file.arrayBuffer());
}
