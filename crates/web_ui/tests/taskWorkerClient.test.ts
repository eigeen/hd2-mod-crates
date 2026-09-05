import { expect, test } from "bun:test";
import { runWasmWorkerTask, type TaskWorker } from "../src/worker/taskWorkerClient";
import type { WorkerInputMessage, WorkerOutputMessage } from "../src/worker/protocol";

test("terminates an active Worker on cancellation and ignores late output", async () => {
  const worker = new FakeWorker();
  const controller = new AbortController();
  const progress: string[] = [];
  const task = runWasmWorkerTask("mergePatches", {}, {
    signal: controller.signal,
    workerFactory: () => worker,
    onProgress: () => progress.push("late"),
  });
  const taskId = worker.startedTaskId();

  controller.abort();
  const messageCountAfterAbort = worker.messages.length;
  worker.emit({
    type: "gameData.request",
    taskId,
    requestId: 4,
    method: "readFull",
    args: ["late.bin"],
  });
  worker.emit({
    type: "task.progress",
    taskId,
    event: { kind: "targetFinish", targetName: "late" },
  });
  worker.emit({ type: "task.success", taskId, result: "late result" });

  await expect(task).rejects.toMatchObject({ code: "task.cancelled" });
  expect(worker.terminated).toBe(true);
  expect(progress).toEqual([]);
  expect(worker.messages).toHaveLength(messageCountAfterAbort);
});

test("uses a fresh Worker without detaching reusable task input", async () => {
  const workers: FakeWorker[] = [];
  const input = new Uint8Array([1, 2, 3]);
  const factory = () => {
    const worker = new FakeWorker();
    workers.push(worker);
    return worker;
  };

  const first = runWasmWorkerTask<number>("mergePatches", { input }, { workerFactory: factory });
  workers[0].succeed(1);
  expect(await first).toBe(1);
  const second = runWasmWorkerTask<number>("mergePatches", { input }, { workerFactory: factory });
  workers[1].succeed(2);

  expect(await second).toBe(2);
  expect(input).toEqual(new Uint8Array([1, 2, 3]));
  expect(workers).toHaveLength(2);
  expect(workers.every((worker) => worker.startTransferCount === 0)).toBe(true);
});

test("drops messages after success and cleans up listeners", async () => {
  const worker = new FakeWorker();
  const progress: string[] = [];
  const task = runWasmWorkerTask<string>("mergePatches", {}, {
    workerFactory: () => worker,
    onProgress: () => progress.push("late"),
  });

  worker.succeed("done");
  expect(await task).toBe("done");
  worker.emit({
    type: "task.progress",
    taskId: worker.startedTaskId(),
    event: { kind: "targetFinish", targetName: "late" },
  });

  expect(progress).toEqual([]);
  expect(worker.listenerCount()).toBe(0);
  expect(worker.terminated).toBe(true);
});

test("serves restricted game data RPC and returns structured read failures", async () => {
  const worker = new FakeWorker();
  const task = runWasmWorkerTask("repatchUnits", {}, {
    workerFactory: () => worker,
    gameData: {
      readFull: async () => { throw new Error("disk read failed"); },
      readRange: async () => new Uint8Array(),
      exists: async () => false,
      listBundleChunks: async () => [],
      listPackages: async () => [],
    },
  });
  const taskId = worker.startedTaskId();
  worker.emit({
    type: "gameData.request",
    taskId,
    requestId: 7,
    method: "readFull",
    args: ["bad.bin"],
  });
  await waitForMicrotasks();

  const response = worker.messages.find((message) => message.type === "gameData.response");
  expect(response).toMatchObject({
    type: "gameData.response",
    requestId: 7,
    error: { code: "gameData.discoveryFailed", message: "disk read failed" },
  });
  worker.emit({
    type: "task.failure",
    taskId,
    error: (response as Extract<WorkerInputMessage, { type: "gameData.response" }>).error!,
  });
  await expect(task).rejects.toMatchObject({
    code: "gameData.discoveryFailed",
    message: "disk read failed",
  });
});

test("transfers an exact game data read buffer back to the Worker", async () => {
  const worker = new FakeWorker();
  const backing = new Uint8Array([0, 5, 6, 0]);
  const task = runWasmWorkerTask("repatchUnits", {}, {
    workerFactory: () => worker,
    gameData: { ...successfulGameData(), readFull: async () => backing.subarray(1, 3) },
  });
  worker.emit({
    type: "gameData.request",
    taskId: worker.startedTaskId(),
    requestId: 3,
    method: "readFull",
    args: ["data.bin"],
  });
  await waitForMicrotasks();

  const responseIndex = worker.messages.findIndex((message) => message.type === "gameData.response");
  const response = worker.messages[responseIndex];
  expect(response).toMatchObject({ type: "gameData.response", result: new Uint8Array([5, 6]) });
  expect(worker.transferCounts[responseIndex]).toBe(1);
  worker.succeed("done");
  expect(await task).toBe("done");
});

test("classifies an unhandled Worker trap as wasm.runtime", async () => {
  const worker = new FakeWorker();
  const task = runWasmWorkerTask("mergeHeadIntoArmor", {}, {
    workerFactory: () => worker,
  });

  worker.emitError();

  await expect(task).rejects.toMatchObject({ code: "wasm.runtime" });
  expect(worker.terminated).toBe(true);
});

test("cleans up when the initial structured clone fails", async () => {
  const worker = new FakeWorker();
  worker.throwOnPostType = "task.start";

  const task = runWasmWorkerTask("mergePatches", { unsupported: () => undefined }, {
    workerFactory: () => worker,
  });

  await expect(task).rejects.toMatchObject({ code: "unknown", message: "clone failed" });
  expect(worker.listenerCount()).toBe(0);
  expect(worker.terminated).toBe(true);
});

test("settles and cleans up when a progress callback fails", async () => {
  const worker = new FakeWorker();
  const task = runWasmWorkerTask("migrateEquipmentVariants", {}, {
    workerFactory: () => worker,
    onProgress: () => { throw new Error("progress failed"); },
  });

  worker.emit({
    type: "task.progress",
    taskId: worker.startedTaskId(),
    event: { kind: "targetFinish", targetName: "target" },
  });

  await expect(task).rejects.toMatchObject({ code: "unknown", message: "progress failed" });
  expect(worker.listenerCount()).toBe(0);
  expect(worker.terminated).toBe(true);
});

test("settles when posting an RPC response fails", async () => {
  const worker = new FakeWorker();
  const task = runWasmWorkerTask("repatchUnits", {}, {
    workerFactory: () => worker,
    gameData: successfulGameData(),
  });
  worker.throwOnPostType = "gameData.response";
  worker.emit({
    type: "gameData.request",
    taskId: worker.startedTaskId(),
    requestId: 9,
    method: "readFull",
    args: ["data.bin"],
  });

  await expect(task).rejects.toMatchObject({ code: "unknown", message: "clone failed" });
  expect(worker.listenerCount()).toBe(0);
  expect(worker.terminated).toBe(true);
});

class FakeWorker implements TaskWorker {
  readonly messages: WorkerInputMessage[] = [];
  readonly transferCounts: number[] = [];
  terminated = false;
  throwOnPostType: WorkerInputMessage["type"] | null = null;
  private readonly messageListeners = new Set<(event: MessageEvent<WorkerOutputMessage>) => void>();
  private readonly errorListeners = new Set<(event: ErrorEvent) => void>();

  get startTransferCount(): number {
    const index = this.messages.findIndex((message) => message.type === "task.start");
    return index < 0 ? -1 : this.transferCounts[index];
  }

  addEventListener(type: "message" | "error", listener: never): void {
    if (type === "message") this.messageListeners.add(listener);
    else this.errorListeners.add(listener);
  }

  removeEventListener(type: "message" | "error", listener: never): void {
    if (type === "message") this.messageListeners.delete(listener);
    else this.errorListeners.delete(listener);
  }

  postMessage(message: WorkerInputMessage, transfer: Transferable[] = []): void {
    if (message.type === this.throwOnPostType) throw new DOMException("clone failed", "DataCloneError");
    this.messages.push(message);
    this.transferCounts.push(transfer.length);
  }

  terminate(): void {
    this.terminated = true;
  }

  startedTaskId(): string {
    const start = this.messages.find((message) => message.type === "task.start");
    if (!start || start.type !== "task.start") throw new Error("Worker did not receive a task");
    return start.taskId;
  }

  succeed(result: unknown): void {
    this.emit({ type: "task.success", taskId: this.startedTaskId(), result });
  }

  emit(message: WorkerOutputMessage): void {
    const event = { data: message } as MessageEvent<WorkerOutputMessage>;
    for (const listener of this.messageListeners) listener(event);
  }

  emitError(): void {
    const trap = new WebAssembly.RuntimeError("unreachable");
    const event = { error: trap, message: trap.message } as ErrorEvent;
    for (const listener of this.errorListeners) listener(event);
  }

  listenerCount(): number {
    return this.messageListeners.size + this.errorListeners.size;
  }
}

function successfulGameData() {
  return {
    readFull: async () => new Uint8Array([1]),
    readRange: async () => new Uint8Array(),
    exists: async () => true,
    listBundleChunks: async () => [],
    listPackages: async () => [],
  };
}

async function waitForMicrotasks(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
}
