import type { GameDataSource } from "./gameDataSource";
import type {
  EquipmentOption,
  MigrationResult,
  PatchFiles,
  PatchInspection,
  TargetOption,
  UnitRepatchOptions,
  UnitRepatchResult,
  UnifiedMigrateOptions,
} from "./types";

export interface MigrationProgressSink {
  onTargetStart?: (targetName: string, targetHash: string) => void;
  onStage?: (targetName: string, stage: string) => void;
  onTargetFinish?: (targetName: string) => void;
}

export class WasmRuntimeTrapError extends Error {
  constructor() {
    super("WebAssembly execution stopped unexpectedly");
    this.name = "WasmRuntimeTrapError";
  }
}

export function isWasmRuntimeTrapError(error: unknown): error is WasmRuntimeTrapError {
  return error instanceof WasmRuntimeTrapError;
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

export async function builtinEquipmentOptions() {
  const wasm = await loadWasm();
  return callWasm("builtin_equipment_options", () =>
    wasm.builtin_equipment_options(),
  ) as Promise<EquipmentOption[]>;
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

export async function inspectEquipmentContents(
  patch: PatchFiles,
  dataSource?: GameDataSource,
) {
  const wasm = await loadWasm();
  if (dataSource) {
    return callWasm("inspect_equipment_with_source", () =>
      wasm.inspect_equipment_with_source(patch.name, patch.toc, dataSource),
    ) as Promise<PatchInspection>;
  }
  return callWasm("inspect_equipment", () =>
    wasm.inspect_equipment(patch.name, patch.toc),
  ) as Promise<PatchInspection>;
}

export async function migrateEquipmentVariants(
  patch: PatchFiles,
  options: UnifiedMigrateOptions,
  dataSource: GameDataSource,
  progress: MigrationProgressSink | null,
): Promise<MigrationResult> {
  const wasm = await loadWasm();
  return callWasm("migrate_equipment_variants", () =>
    wasm.migrate_equipment_variants(
      patch.name,
      patch.toc,
      patch.gpu,
      patch.stream,
      options,
      dataSource,
      progress ?? null,
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
    return await runWithWasmTrapBoundary(fn);
  } catch (error) {
    console.error(`[hd2-migrator] wasm.${label} threw:`, error);
    throw normalizeWasmError(error);
  }
}

/** Convert async WASM traps, which can bypass Promise rejection, into a normal failure. */
function runWithWasmTrapBoundary<T>(fn: () => T | Promise<T>): Promise<T> {
  if (typeof window === "undefined") return Promise.resolve().then(fn);
  return new Promise<T>((resolve, reject) => {
    const cleanup = () => {
      window.removeEventListener("error", onWindowError);
      window.removeEventListener("unhandledrejection", onUnhandledRejection);
    };
    const rejectTrap = (error: unknown, event: Event) => {
      if (!isWasmRuntimeTrap(error)) return;
      event.preventDefault();
      cleanup();
      reject(new WasmRuntimeTrapError());
    };
    const onWindowError = (event: ErrorEvent) => {
      rejectTrap(event.error, event);
    };
    const onUnhandledRejection = (event: PromiseRejectionEvent) => {
      rejectTrap(event.reason, event);
    };
    window.addEventListener("error", onWindowError);
    window.addEventListener("unhandledrejection", onUnhandledRejection);
    Promise.resolve().then(fn).then(
      (value) => { cleanup(); resolve(value); },
      (error) => { cleanup(); reject(error); },
    );
  });
}

function isWasmRuntimeTrap(error: unknown): boolean {
  if (error instanceof WebAssembly.RuntimeError) return true;
  if (!(error instanceof Error) || error.name !== "RuntimeError") return false;
  return error.stack?.includes(".wasm") ?? false;
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
