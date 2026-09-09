import type { MigrationMapping } from "@hd2-mod-tools/migrator-ui";

type WasmModule = typeof import("../wasm/hd2_migrator_wasm/hd2_migrator_wasm.js");

export type PatchMetadataRequest =
  | { method: "sidecars"; toc: Uint8Array; streamLength: number }
  | { method: "detect" | "inspect"; name: string; toc: Uint8Array; category: string }
  | { method: "equipment" | "analysis"; name: string; toc: Uint8Array; withGameData: boolean }
  | { method: "preview"; name: string; toc: Uint8Array; mapping: MigrationMapping }
  | { method: "previews"; name: string; toc: Uint8Array; mappings: MigrationMapping[] };

/** Inspect only TOC metadata in a disposable Worker; sidecar buffers never enter this protocol. */
export function inspectPatchMetadata(wasm: WasmModule, input: PatchMetadataRequest, source: object): unknown {
  switch (input.method) {
    case "sidecars": return wasm.patch_sidecar_requirements(input.toc, input.streamLength);
    case "detect": return wasm.detect_source(input.name, input.toc, input.category);
    case "inspect": return wasm.inspect_patch(input.name, input.toc, input.category);
    case "equipment": return input.withGameData
      ? wasm.inspect_equipment_with_source(input.name, input.toc, source)
      : wasm.inspect_equipment(input.name, input.toc);
    case "analysis": return input.withGameData
      ? wasm.analyze_equipment_patch_with_source(input.name, input.toc, source)
      : wasm.analyze_equipment_patch(input.name, input.toc);
    case "preview": return wasm.preview_equipment_mapping(input.name, input.toc, input.mapping);
    case "previews": return wasm.preview_equipment_mappings(input.name, input.toc, input.mappings);
    default: throw new Error("Unsupported patch metadata request");
  }
}
