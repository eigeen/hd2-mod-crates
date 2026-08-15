export type TaskErrorCode =
  | "equipment.loadFailed"
  | "gameData.discoveryFailed"
  | "gameData.invalid"
  | "migration.failed"
  | "patch.inspectFailed"
  | "repatch.failed"
  | "task.cancelled"
  | "task.conflict"
  | "task.joinFailed"
  | "wasm.runtime"
  | "unknown";

export interface StructuredTaskError {
  code: TaskErrorCode;
  message: string;
}

export class TaskError extends Error implements StructuredTaskError {
  constructor(public readonly code: TaskErrorCode, message: string) {
    super(message);
    this.name = "TaskError";
  }
}

export function cancelledTaskError(): TaskError {
  return new TaskError("task.cancelled", "Task was cancelled");
}

export function throwIfTaskCancelled(signal: AbortSignal): void {
  if (signal.aborted) throw cancelledTaskError();
}

export function normalizeTaskError(
  error: unknown,
  fallbackCode: TaskErrorCode = "unknown",
): TaskError {
  if (error instanceof TaskError) return error;
  const structured = structuredError(error);
  if (structured) return new TaskError(structured.code, structured.message);
  return new TaskError(fallbackCode, errorMessage(error));
}

function structuredError(error: unknown): StructuredTaskError | null {
  if (!error || typeof error !== "object") return null;
  const candidate = error as { code?: unknown; message?: unknown };
  if (typeof candidate.code !== "string" || typeof candidate.message !== "string") return null;
  return { code: candidate.code as TaskErrorCode, message: candidate.message };
}

function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  return String(error);
}
