/// <reference types="vite/client" />

declare module "./wasm/hd2_migrator_wasm/hd2_migrator_wasm.js" {
  export default function init(input?: RequestInfo | URL | Response | BufferSource | WebAssembly.Module): Promise<unknown>;
  export function builtin_target_options(category?: string): unknown;
  export function detect_source(
    patchName: string,
    toc: Uint8Array,
    gpu: Uint8Array,
    stream: Uint8Array,
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
}
