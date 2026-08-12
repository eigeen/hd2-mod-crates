import type { GameDataSource } from "./gameDataSource";
import type {
  MigrateOptions,
  MigrationResult,
  PatchFiles,
  PatchInspection,
  TargetOption,
  UnitRepatchOptions,
  UnitRepatchResult,
} from "./types";

export interface MigrationProgressSink {
  onTargetStart?: (targetName: string, targetHash: string) => void;
  onStage?: (targetName: string, stage: string) => void;
  onTargetFinish?: (targetName: string) => void;
}

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

export async function detectSource(patch: PatchFiles, category = "Armor") {
  const wasm = await loadWasm();
  // 只传 toc，gpu/stream 不参与来源识别；避免把数百 MB 数据拷贝进 WASM 线性内存触发 OOM。
  return callWasm("detect_source", () =>
    wasm.detect_source(patch.name, patch.toc, category),
  ) as Promise<TargetOption | null>;
}

export async function inspectPatchContents(patch: PatchFiles, category = "Armor") {
  const wasm = await loadWasm();
  return callWasm("inspect_patch", () =>
    wasm.inspect_patch(patch.name, patch.toc, category),
  ) as Promise<PatchInspection>;
}

export async function migrate(patch: PatchFiles, options: MigrateOptions, category = "Armor") {
  const wasm = await loadWasm();
  const fnName = options.targetHashes.length === 1 ? "migrate_one" : "migrate_many";
  const fn = options.targetHashes.length === 1 ? wasm.migrate_one : wasm.migrate_many;
  return callWasm(fnName, () =>
    fn(patch.name, patch.toc, patch.gpu, patch.stream, options, category),
  ) as Promise<MigrationResult>;
}

export async function migrateCrossArchive(
  patch: PatchFiles,
  options: MigrateOptions,
  dataSource: GameDataSource,
  progress: MigrationProgressSink | null,
  category = "Armor",
): Promise<MigrationResult> {
  const wasm = await loadWasm();
  return callWasm("migrate_cross_archive", () =>
    wasm.migrate_cross_archive(
      patch.name,
      patch.toc,
      patch.gpu,
      patch.stream,
      options,
      dataSource,
      progress ?? null,
      category,
    ),
  ) as Promise<MigrationResult>;
}

export async function repatchUnits(
  patch: PatchFiles,
  options: UnitRepatchOptions,
  dataSource: GameDataSource,
): Promise<UnitRepatchResult> {
  const wasm = await loadWasm();
  // Sidecars stay in JS and are reused verbatim in the output ZIP.
  return callWasm("repatch_units", () =>
    wasm.repatch_units(patch.name, patch.toc, options, dataSource),
  ) as Promise<UnitRepatchResult>;
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
