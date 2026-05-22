/// <reference types="vite/client" />

declare module "./wasm/hd2_migrator_wasm/hd2_migrator_wasm.js" {
  export default function init(input?: RequestInfo | URL | Response | BufferSource | WebAssembly.Module): Promise<unknown>;
  export function builtin_target_options(category?: string): unknown;
  export function build_metadata(category: string, archives: unknown): string;
  export function list_targets(metadataJson: string): unknown;
  export function detect_source(
    metadataJson: string,
    patchName: string,
    toc: Uint8Array,
    gpu: Uint8Array,
    stream: Uint8Array,
  ): unknown;
  export function migrate_one(
    metadataJson: string,
    patchName: string,
    toc: Uint8Array,
    gpu: Uint8Array,
    stream: Uint8Array,
    options: unknown,
  ): unknown;
  export function migrate_many(
    metadataJson: string,
    patchName: string,
    toc: Uint8Array,
    gpu: Uint8Array,
    stream: Uint8Array,
    options: unknown,
  ): unknown;
}
