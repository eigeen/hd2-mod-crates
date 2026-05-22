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
  return callWasm("builtin_target_options", () => wasm.builtin_target_options(category)) as Promise<TargetOption[]>;
}

export async function buildMetadataJson(category: string, archives: DirectoryArchiveInput[]) {
  const wasm = await loadWasm();
  return callWasm("build_metadata", () => wasm.build_metadata(category, archives));
}

export async function listTargets(metadataJson: string) {
  const wasm = await loadWasm();
  return callWasm("list_targets", () => wasm.list_targets(metadataJson)) as Promise<TargetOption[]>;
}

export async function detectSource(metadataJson: string, patch: PatchFiles) {
  const wasm = await loadWasm();
  return callWasm("detect_source", () =>
    wasm.detect_source(metadataJson, patch.name, patch.toc, patch.gpu, patch.stream),
  ) as Promise<TargetOption | null>;
}

export async function migrate(metadataJson: string, patch: PatchFiles, options: MigrateOptions) {
  const wasm = await loadWasm();
  const fnName = options.targetHashes.length === 1 ? "migrate_one" : "migrate_many";
  const fn = options.targetHashes.length === 1 ? wasm.migrate_one : wasm.migrate_many;
  return callWasm(fnName, () =>
    fn(metadataJson, patch.name, patch.toc, patch.gpu, patch.stream, options),
  ) as Promise<MigrationResult>;
}

async function callWasm<T>(label: string, fn: () => T | Promise<T>): Promise<T> {
  try {
    return await fn();
  } catch (error) {
    console.error(`[hd2-migrator] wasm.${label} threw:`, error);
    throw normalizeWasmError(error);
  }
}

function normalizeWasmError(error: unknown): Error {
  if (error instanceof Error) {
    return error;
  }
  if (typeof error === "string") {
    return new Error(error);
  }
  if (error && typeof error === "object") {
    const message = (error as { message?: unknown }).message;
    if (typeof message === "string") {
      return new Error(message);
    }
    try {
      return new Error(JSON.stringify(error));
    } catch {
      return new Error(String(error));
    }
  }
  return new Error(String(error));
}
