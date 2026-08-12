/// <reference types="vite/client" />

// File System Access API：lib.dom 暂未涵盖权限方法、entries() 与 showDirectoryPicker
// 因此在此处补全所需的最小类型表面，仅覆盖项目实际使用到的形态。
interface FileSystemHandlePermissionDescriptor {
  mode?: "read" | "readwrite";
}

interface FileSystemHandle {
  queryPermission(descriptor?: FileSystemHandlePermissionDescriptor): Promise<PermissionState>;
  requestPermission(descriptor?: FileSystemHandlePermissionDescriptor): Promise<PermissionState>;
}

interface FileSystemDirectoryHandle {
  entries(): AsyncIterableIterator<[string, FileSystemHandle]>;
}

interface ShowDirectoryPickerOptions {
  id?: string;
  mode?: "read" | "readwrite";
  startIn?: FileSystemHandle | "desktop" | "documents" | "downloads" | "music" | "pictures" | "videos";
}

interface Window {
  showDirectoryPicker(options?: ShowDirectoryPickerOptions): Promise<FileSystemDirectoryHandle>;
}

declare module "./wasm/hd2_migrator_wasm/hd2_migrator_wasm.js" {
  export default function init(input?: RequestInfo | URL | Response | BufferSource | WebAssembly.Module): Promise<unknown>;
  export function builtin_target_options(category?: string): unknown;
  export function detect_source(
    patchName: string,
    toc: Uint8Array,
    category?: string,
  ): unknown;
  export function migrate_one(
    patchName: string,
    toc: Uint8Array,
    gpu: Uint8Array,
    stream: Uint8Array,
    options: unknown,
    category?: string,
  ): unknown;
  export function migrate_many(
    patchName: string,
    toc: Uint8Array,
    gpu: Uint8Array,
    stream: Uint8Array,
    options: unknown,
    category?: string,
  ): unknown;
  export function migrate_cross_archive(
    patchName: string,
    toc: Uint8Array,
    gpu: Uint8Array,
    stream: Uint8Array,
    options: unknown,
    dataSource: unknown,
    progress: unknown,
    category?: string,
  ): Promise<unknown>;
  export function repatch_units(
    patchName: string,
    toc: Uint8Array,
    options: unknown,
    dataSource: unknown,
  ): Promise<unknown>;
}
