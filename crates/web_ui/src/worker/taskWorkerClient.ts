import { TaskError, type TaskErrorCode } from "@hd2-mod-tools/migrator-ui";
import {
  executeGameDataRequest,
  type GameDataSourcePort,
} from "../gameDataSource";
import type {
  GameDataRequestMessage,
  MigrationProgressEvent,
  SerializedTaskError,
  WasmTaskOperation,
  WorkerInputMessage,
  WorkerOutputMessage,
} from "./protocol";

export interface TaskWorker {
  addEventListener(type: "message", listener: (event: MessageEvent<WorkerOutputMessage>) => void): void;
  addEventListener(type: "error", listener: (event: ErrorEvent) => void): void;
  removeEventListener(type: "message", listener: (event: MessageEvent<WorkerOutputMessage>) => void): void;
  removeEventListener(type: "error", listener: (event: ErrorEvent) => void): void;
  postMessage(message: WorkerInputMessage, transfer?: Transferable[]): void;
  terminate(): void;
}

export type TaskWorkerFactory = () => TaskWorker;

export interface WorkerTaskOptions {
  signal?: AbortSignal;
  gameData?: GameDataSourcePort;
  onProgress?: (event: MigrationProgressEvent) => void;
  workerFactory?: TaskWorkerFactory;
}

interface ActiveTask<T> {
  worker: TaskWorker;
  taskId: string;
  options: WorkerTaskOptions;
  resolve: (result: T) => void;
  reject: (error: unknown) => void;
  settled: boolean;
}

export function runWasmWorkerTask<T>(
  operation: WasmTaskOperation,
  payload: unknown,
  options: WorkerTaskOptions = {},
): Promise<T> {
  if (options.signal?.aborted) return Promise.reject(cancelledError());
  return new Promise<T>((resolve, reject) => {
    const worker = (options.workerFactory ?? createTaskWorker)();
    const task: ActiveTask<T> = {
      worker,
      taskId: crypto.randomUUID(),
      options,
      resolve,
      reject,
      settled: false,
    };
    const listeners = createTaskListeners(task);
    worker.addEventListener("message", listeners.message);
    worker.addEventListener("error", listeners.error);
    options.signal?.addEventListener("abort", listeners.abort, { once: true });
    if (options.signal?.aborted) {
      listeners.abort();
      return;
    }
    try {
      worker.postMessage({
        type: "task.start",
        taskId: task.taskId,
        operation,
        payload,
      });
    } catch (error) {
      listeners.fail(error);
    }
  });
}

function createTaskWorker(): TaskWorker {
  return new Worker(new URL("./wasmTask.worker.ts", import.meta.url), {
    type: "module",
  });
}

function createTaskListeners<T>(task: ActiveTask<T>) {
  const cleanup = () => {
    task.worker.removeEventListener("message", message);
    task.worker.removeEventListener("error", error);
    task.options.signal?.removeEventListener("abort", abort);
    task.worker.terminate();
  };
  const settle = (outcome: { result: T } | { error: unknown }) => {
    if (task.settled) return;
    task.settled = true;
    cleanup();
    if ("result" in outcome) task.resolve(outcome.result);
    else task.reject(outcome.error);
  };
  const abort = () => settle({ error: cancelledError() });
  const error = (event: ErrorEvent) => settle({ error: workerExecutionError(event) });
  const fail = (failure: unknown) => settle({ error: normalizeClientError(failure) });
  const message = (event: MessageEvent<WorkerOutputMessage>) => {
    if (task.settled || event.data.taskId !== task.taskId) return;
    void handleWorkerMessage(task, event.data, settle).catch(fail);
  };
  return { abort, error, fail, message };
}

async function handleWorkerMessage<T>(
  task: ActiveTask<T>,
  message: WorkerOutputMessage,
  settle: (outcome: { result: T } | { error: unknown }) => void,
): Promise<void> {
  if (message.type === "task.success") {
    settle({ result: message.result as T });
    return;
  }
  if (message.type === "task.failure") {
    settle({ error: taskError(message.error) });
    return;
  }
  if (message.type === "task.progress") {
    task.options.onProgress?.(message.event);
    return;
  }
  await serveGameDataRequest(task, message, settle);
}

async function serveGameDataRequest<T>(
  task: ActiveTask<T>,
  request: GameDataRequestMessage,
  settle: (outcome: { result: T } | { error: unknown }) => void,
): Promise<void> {
  if (!task.options.gameData) {
    respondToGameDataRequest(task, request, undefined, {
      code: "gameData.invalid",
      message: "Game data is not available for this task",
    });
    return;
  }
  let result: unknown;
  let error: SerializedTaskError | undefined;
  try {
    result = await executeGameDataRequest(task.options.gameData, request.method, request.args);
  } catch (failure) {
    error = serializeError(failure);
  }
  if (!task.settled) respondToGameDataRequest(task, request, result, error);
  if (task.options.signal?.aborted) settle({ error: cancelledError() });
}

function respondToGameDataRequest<T>(
  task: ActiveTask<T>,
  request: GameDataRequestMessage,
  result?: unknown,
  error?: SerializedTaskError,
): void {
  const transferable = transferableResult(result);
  const response: WorkerInputMessage = {
    type: "gameData.response",
    taskId: task.taskId,
    requestId: request.requestId,
    result: transferable.result,
    error,
  };
  task.worker.postMessage(response, transferable.transfer);
}

function transferableResult(result: unknown): { result: unknown; transfer: Transferable[] } {
  if (!(result instanceof Uint8Array)) return { result, transfer: [] };
  const bytes = exactTransferableBytes(result);
  return { result: bytes, transfer: [bytes.buffer as ArrayBuffer] };
}

function exactTransferableBytes(bytes: Uint8Array): Uint8Array {
  if (bytes.byteOffset === 0 && bytes.byteLength === bytes.buffer.byteLength) return bytes;
  return bytes.slice();
}

function serializeError(error: unknown): SerializedTaskError {
  if (error && typeof error === "object") {
    const candidate = error as { code?: unknown; message?: unknown };
    if (typeof candidate.message === "string") {
      return {
        code: typeof candidate.code === "string" ? candidate.code : "gameData.discoveryFailed",
        message: candidate.message,
      };
    }
  }
  return { code: "gameData.discoveryFailed", message: String(error) };
}

function taskError(error: SerializedTaskError): TaskError {
  return new TaskError(error.code as TaskErrorCode, error.message);
}

function cancelledError(): TaskError {
  return new TaskError("task.cancelled", "Task was cancelled");
}

function runtimeTrapError(): TaskError {
  return new TaskError("wasm.runtime", "WebAssembly execution stopped unexpectedly");
}

function workerExecutionError(event: ErrorEvent): TaskError {
  if (isRuntimeTrap(event.error)) return runtimeTrapError();
  const location = event.filename
    ? ` (${event.filename}${event.lineno ? `:${event.lineno}` : ""})`
    : "";
  return new TaskError("unknown", `${event.message || "Worker execution failed"}${location}`);
}

function normalizeClientError(error: unknown): TaskError {
  if (error instanceof TaskError) return error;
  if (error instanceof Error) return new TaskError("unknown", error.message);
  return new TaskError("unknown", String(error));
}

function isRuntimeTrap(error: unknown): boolean {
  if (error instanceof WebAssembly.RuntimeError) return true;
  if (!(error instanceof Error) || error.name !== "RuntimeError") return false;
  return error.stack?.includes(".wasm") ?? false;
}
