import { startTaskWorkerRuntime, type WorkerRuntimeScope } from "./taskWorkerRuntime";

startTaskWorkerRuntime(globalThis as unknown as WorkerRuntimeScope);
