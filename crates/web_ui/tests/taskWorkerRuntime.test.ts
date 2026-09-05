import { expect, test } from "bun:test";
import {
  startTaskWorkerRuntime,
  type WasmLoader,
  type WorkerRuntimeScope,
} from "../src/worker/taskWorkerRuntime";
import type { WorkerInputMessage, WorkerOutputMessage } from "../src/worker/protocol";

test("preserves a structured RPC failure through the WASM task boundary", async () => {
  const scope = new FakeRuntimeScope();
  scope.onOutput = (message) => {
    if (message.type !== "gameData.request") return;
    scope.emit({
      type: "gameData.response",
      taskId: message.taskId,
      requestId: message.requestId,
      error: { code: "gameData.discoveryFailed", message: "package read failed" },
    });
  };
  startTaskWorkerRuntime(scope, fakeWasmLoader({
    repatch_units: async (_patch: unknown, _options: unknown, source: RemoteSource) => {
      await source.readFull("broken.bin");
    },
  }));

  scope.start("repatchUnits");
  await waitForMicrotasks();

  expect(scope.outputs.at(-1)).toMatchObject({
    type: "task.failure",
    error: { code: "gameData.discoveryFailed", message: "package read failed" },
  });
});

test("normalizes a caught WASM runtime trap", async () => {
  const scope = new FakeRuntimeScope();
  startTaskWorkerRuntime(scope, fakeWasmLoader({
    merge_patches: () => { throw new WebAssembly.RuntimeError("unreachable"); },
  }));

  scope.start("mergePatches");
  await waitForMicrotasks();

  expect(scope.outputs.at(-1)).toMatchObject({
    type: "task.failure",
    error: { code: "wasm.runtime" },
  });
});

interface RemoteSource {
  readFull(path: string): Promise<unknown>;
}

class FakeRuntimeScope implements WorkerRuntimeScope {
  readonly outputs: WorkerOutputMessage[] = [];
  onOutput: ((message: WorkerOutputMessage) => void) | null = null;
  private listener: ((event: MessageEvent<WorkerInputMessage>) => void) | null = null;

  addEventListener(
    _type: "message",
    listener: (event: MessageEvent<WorkerInputMessage>) => void,
  ): void {
    this.listener = listener;
  }

  postMessage(message: WorkerOutputMessage): void {
    this.outputs.push(message);
    this.onOutput?.(message);
  }

  start(operation: "mergePatches" | "repatchUnits"): void {
    this.emit({ type: "task.start", taskId: "task-1", operation, payload: {} });
  }

  emit(message: WorkerInputMessage): void {
    if (!this.listener) throw new Error("Worker runtime was not started");
    this.listener({ data: message } as MessageEvent<WorkerInputMessage>);
  }
}

function fakeWasmLoader(methods: Record<string, unknown>): WasmLoader {
  return async () => methods as Awaited<ReturnType<WasmLoader>>;
}

async function waitForMicrotasks(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 0));
}
