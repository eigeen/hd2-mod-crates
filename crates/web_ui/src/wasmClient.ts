import type {
  DirectoryArchiveInput,
  MigrateOptions,
  MigrationResult,
  PatchFiles,
  TargetOption,
} from "./types";

let ready: Promise<typeof import("./wasm/hd2_migrator_wasm/hd2_migrator_wasm.js")> | null = null;

export function loadWasm() {
  if (!ready) {
    ready = import("./wasm/hd2_migrator_wasm/hd2_migrator_wasm.js").then(async (module) => {
      await module.default();
      return module;
    });
  }
  return ready;
}

export async function builtinTargetOptions(category = "Armor") {
  const wasm = await loadWasm();
  return wasm.builtin_target_options(category) as TargetOption[];
}

export async function buildMetadataJson(category: string, archives: DirectoryArchiveInput[]) {
  const wasm = await loadWasm();
  return wasm.build_metadata(category, archives);
}

export async function listTargets(metadataJson: string) {
  const wasm = await loadWasm();
  return wasm.list_targets(metadataJson) as TargetOption[];
}

export async function detectSource(metadataJson: string, patch: PatchFiles) {
  const wasm = await loadWasm();
  return wasm.detect_source(
    metadataJson,
    patch.name,
    patch.toc,
    patch.gpu,
    patch.stream,
  ) as TargetOption | null;
}

export async function migrate(metadataJson: string, patch: PatchFiles, options: MigrateOptions) {
  const wasm = await loadWasm();
  const fn = options.targetHashes.length === 1 ? wasm.migrate_one : wasm.migrate_many;
  return fn(
    metadataJson,
    patch.name,
    patch.toc,
    patch.gpu,
    patch.stream,
    options,
  ) as MigrationResult;
}
