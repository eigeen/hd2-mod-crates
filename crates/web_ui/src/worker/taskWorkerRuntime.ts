import { StoreZipBuilder } from "../fileInputs";
import { inspectPatchMetadata, type PatchMetadataRequest } from "./patchMetadata";
import type {
  GameDataMethod,
  GameDataResponseMessage,
  MigrationProgressEvent,
  SerializedTaskError,
  TaskStartMessage,
  WasmTaskOperation,
  WorkerInputMessage,
  WorkerOutputMessage,
} from "./protocol";

type WasmModule = typeof import("../wasm/hd2_migrator_wasm/hd2_migrator_wasm.js");

export interface WorkerRuntimeScope {
  addEventListener(type: "message", listener: (event: MessageEvent<WorkerInputMessage>) => void): void;
  postMessage(message: WorkerOutputMessage, transfer?: Transferable[]): void;
}

export type WasmLoader = () => Promise<WasmModule>;

interface PendingRequest {
  resolve: (value: unknown) => void;
  reject: (error: unknown) => void;
}

interface RuntimeState {
  taskId: string | null;
  nextRequestId: number;
  pendingRequests: Map<number, PendingRequest>;
  rpcFailure: SerializedTaskError | null;
}

interface GameDataRpcContext {
  scope: WorkerRuntimeScope;
  state: RuntimeState;
  taskId: string;
}

export function startTaskWorkerRuntime(
  scope: WorkerRuntimeScope,
  loadWasm: WasmLoader = loadTaskWasm,
): void {
  const state: RuntimeState = {
    taskId: null,
    nextRequestId: 1,
    pendingRequests: new Map(),
    rpcFailure: null,
  };
  scope.addEventListener("message", (event) => {
    if (event.data.type === "gameData.response") {
      resolveGameDataResponse(state, event.data);
      return;
    }
    void startTask(scope, state, event.data, loadWasm);
  });
}

async function startTask(
  scope: WorkerRuntimeScope,
  state: RuntimeState,
  request: TaskStartMessage,
  loadWasm: WasmLoader,
): Promise<void> {
  if (state.taskId !== null) {
    postFailure(scope, request.taskId, {
      code: "task.conflict",
      message: "This Worker is already running a task",
    });
    return;
  }
  state.taskId = request.taskId;
  try {
    const wasm = await loadWasm();
    const result = await executeTask(scope, state, wasm, request);
    scope.postMessage(
      { type: "task.success", taskId: request.taskId, result },
      collectTransferables(result),
    );
  } catch (error) {
    postFailure(scope, request.taskId, taskFailure(state, error, request.operation));
  } finally {
    rejectPendingRequests(state);
  }
}

async function executeTask(
  scope: WorkerRuntimeScope,
  state: RuntimeState,
  wasm: WasmModule,
  request: TaskStartMessage,
): Promise<unknown> {
  if (request.operation === "mergePatches") return mergePatches(wasm, request.payload);
  if (request.operation === "mergePatchesToZip") {
    return mergePatchesToZip(wasm, request.payload);
  }
  if (request.operation === "mergeHeadIntoArmor") {
    return mergeHeadIntoArmor(wasm, request.payload);
  }
  const dataSource = remoteGameDataSource(scope, state, request.taskId);
  if (request.operation === "patchMetadata") {
    return inspectPatchMetadata(wasm, request.payload as PatchMetadataRequest, dataSource);
  }
  const progress = progressCallbacks(scope, request.taskId);
  if (request.operation === "migrateEquipmentVariants") {
    return migrateEquipmentVariants(wasm, request.payload, dataSource, progress);
  }
  if (request.operation === "repatchUnits") return repatchUnits(wasm, request.payload, dataSource, progress);
  throw new Error(`Unsupported task operation: ${request.operation}`);
}

function mergePatches(wasm: WasmModule, payload: unknown): unknown {
  const input = payload as { inputs: unknown; outputName: string };
  return wasm.merge_patches(input.inputs, input.outputName);
}

function mergePatchesToZip(wasm: WasmModule, payload: unknown): unknown {
  const input = payload as { inputs: unknown; outputName: string };
  const zip = new StoreZipBuilder();
  const result = wasm.merge_patches_stream(
    input.inputs,
    input.outputName,
    archiveCallbacks(zip),
  ) as { summary: unknown };
  return { zipBlob: zip.finish(), summary: result.summary };
}

function mergeHeadIntoArmor(wasm: WasmModule, payload: unknown): unknown {
  const input = payload as { armorPatch: unknown; headPatch: unknown; options: unknown };
  const zip = new StoreZipBuilder();
  const result = wasm.merge_head_into_armor_stream(
    input.armorPatch,
    input.headPatch,
    input.options,
    archiveCallbacks(zip),
  ) as { summary: unknown };
  return { zipBlob: zip.finish(), summary: result.summary };
}

async function migrateEquipmentVariants(
  wasm: WasmModule,
  payload: unknown,
  dataSource: object,
  progress: ReturnType<typeof progressCallbacks>,
): Promise<unknown> {
  const input = payload as { patch: unknown; options: unknown };
  const zip = new StoreZipBuilder();
  const result = await wasm.migrate_equipment_variants(input.patch, input.options, dataSource, {
    ...progress,
    onFile: (path: string, bytes: Uint8Array, crc32: number) => zip.add(path, bytes, crc32),
  }) as { summary: unknown };
  return { zipBlob: zip.finish(), summary: result.summary };
}

async function repatchUnits(
  wasm: WasmModule,
  payload: unknown,
  dataSource: object,
  progress: ReturnType<typeof progressCallbacks>,
): Promise<unknown> {
  const input = payload as { patch: unknown; options: unknown };
  return wasm.repatch_units(input.patch, input.options, dataSource, progress);
}

function archiveCallbacks(zip: StoreZipBuilder) {
  return {
    onFileStart: (path: string, size: number, crc32: number) => zip.beginFile(path, size, crc32),
    onFileChunk: (bytes: Uint8Array) => zip.addFileChunk(bytes),
    onFileEnd: () => zip.endFile(),
  };
}

function progressCallbacks(scope: WorkerRuntimeScope, taskId: string) {
  const post = (event: MigrationProgressEvent) => {
    scope.postMessage({ type: "task.progress", taskId, event });
  };
  return {
    onTargetStart: (targetName: string, targetHash: string) => {
      post({ kind: "targetStart", targetName, targetHash });
    },
    onStage: (targetName: string, stage: string) => post({ kind: "stage", targetName, stage }),
    onTargetFinish: (targetName: string) => post({ kind: "targetFinish", targetName }),
  };
}

function remoteGameDataSource(
  scope: WorkerRuntimeScope,
  state: RuntimeState,
  taskId: string,
) {
  const rpc = { scope, state, taskId };
  return {
    readFull: (path: string) => requestGameData(rpc, "readFull", [path]),
    readRange: (path: string, offset: number, len: number) =>
      requestGameData(rpc, "readRange", [path, offset, len]),
    exists: (path: string) => requestGameData(rpc, "exists", [path]),
    listBundleChunks: () => requestGameData(rpc, "listBundleChunks", []),
    listPackages: () => requestGameData(rpc, "listPackages", []),
  };
}

function requestGameData(
  context: GameDataRpcContext,
  method: GameDataMethod,
  args: unknown[],
): Promise<unknown> {
  const requestId = context.state.nextRequestId;
  context.state.nextRequestId += 1;
  return new Promise((resolve, reject) => {
    context.state.pendingRequests.set(requestId, { resolve, reject });
    context.scope.postMessage({
      type: "gameData.request",
      taskId: context.taskId,
      requestId,
      method,
      args,
    });
  });
}

function resolveGameDataResponse(state: RuntimeState, response: GameDataResponseMessage): void {
  if (response.taskId !== state.taskId) return;
  const pending = state.pendingRequests.get(response.requestId);
  if (!pending) return;
  state.pendingRequests.delete(response.requestId);
  if (!response.error) {
    pending.resolve(response.result);
    return;
  }
  state.rpcFailure = response.error;
  pending.reject(response.error);
}

function rejectPendingRequests(state: RuntimeState): void {
  const error = new Error("Task Worker stopped before game data arrived");
  for (const pending of state.pendingRequests.values()) pending.reject(error);
  state.pendingRequests.clear();
}

function postFailure(
  scope: WorkerRuntimeScope,
  taskId: string,
  error: SerializedTaskError,
): void {
  scope.postMessage({ type: "task.failure", taskId, error });
}

function normalizeWorkerError(
  error: unknown,
  operation: WasmTaskOperation,
): SerializedTaskError {
  if (isWasmRuntimeTrap(error)) {
    return { code: "wasm.runtime", message: "WebAssembly execution stopped unexpectedly" };
  }
  if (error && typeof error === "object") {
    const candidate = error as { code?: unknown; message?: unknown };
    if (typeof candidate.code === "string" && typeof candidate.message === "string") {
      return { code: candidate.code, message: candidate.message };
    }
    if (typeof candidate.message === "string") {
      return { code: fallbackCode(operation), message: candidate.message };
    }
  }
  return { code: fallbackCode(operation), message: String(error) };
}

function taskFailure(
  state: RuntimeState,
  error: unknown,
  operation: WasmTaskOperation,
): SerializedTaskError {
  if (isWasmRuntimeTrap(error)) return normalizeWorkerError(error, operation);
  return state.rpcFailure ?? normalizeWorkerError(error, operation);
}

function fallbackCode(operation: WasmTaskOperation): string {
  if (operation === "patchMetadata") return "patch.inspectFailed";
  if (operation === "mergePatches" || operation === "mergePatchesToZip") return "merge.failed";
  if (operation === "mergeHeadIntoArmor") return "headMerge.failed";
  if (operation === "repatchUnits") return "repatch.failed";
  return "migration.failed";
}

function isWasmRuntimeTrap(error: unknown): boolean {
  if (error instanceof WebAssembly.RuntimeError) return true;
  if (!(error instanceof Error) || error.name !== "RuntimeError") return false;
  return error.stack?.includes(".wasm") ?? false;
}

function collectTransferables(value: unknown): Transferable[] {
  const buffers = new Set<ArrayBuffer>();
  collectBuffers(value, buffers, new WeakSet<object>());
  return [...buffers];
}

function collectBuffers(
  value: unknown,
  buffers: Set<ArrayBuffer>,
  visited: WeakSet<object>,
): void {
  if (value instanceof Uint8Array) {
    buffers.add(value.buffer as ArrayBuffer);
    return;
  }
  if (!value || typeof value !== "object" || value instanceof Blob || visited.has(value)) return;
  visited.add(value);
  for (const child of Object.values(value)) collectBuffers(child, buffers, visited);
}

let wasmReady: Promise<WasmModule> | null = null;

function loadTaskWasm(): Promise<WasmModule> {
  if (!wasmReady) {
    wasmReady = import("../wasm/hd2_migrator_wasm/hd2_migrator_wasm.js").then(async (module) => {
      await module.default();
      return module;
    });
  }
  return wasmReady;
}
