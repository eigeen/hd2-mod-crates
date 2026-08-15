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

export interface TaskErrorPresentation {
  error: TaskError;
  title: string;
  description: string;
  diagnostic: string;
}

type ErrorTranslate = (key: TaskErrorTranslationKey) => string;
type TaskErrorTranslationKey =
  | "error.equipmentLoadFailed"
  | "error.gameDataDiscoveryFailed"
  | "error.gameDataInvalid"
  | "error.migrationFailed"
  | "error.patchInspectFailed"
  | "error.repatchFailed"
  | "error.taskConflict"
  | "error.taskJoinFailed"
  | "error.wasmRuntime"
  | "error.unknown";

const ERROR_TITLES: Record<Exclude<TaskErrorCode, "task.cancelled">, TaskErrorTranslationKey> = {
  "equipment.loadFailed": "error.equipmentLoadFailed",
  "gameData.discoveryFailed": "error.gameDataDiscoveryFailed",
  "gameData.invalid": "error.gameDataInvalid",
  "migration.failed": "error.migrationFailed",
  "patch.inspectFailed": "error.patchInspectFailed",
  "repatch.failed": "error.repatchFailed",
  "task.conflict": "error.taskConflict",
  "task.joinFailed": "error.taskJoinFailed",
  "wasm.runtime": "error.wasmRuntime",
  unknown: "error.unknown",
};

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

/** Builds the same user-facing summary and copyable diagnostic for both backends. */
export function presentTaskError(
  error: unknown,
  translate: ErrorTranslate,
  fallbackCode: TaskErrorCode = "unknown",
): TaskErrorPresentation {
  const taskError = normalizeTaskError(error, fallbackCode);
  if (taskError.code === "task.cancelled") {
    return {
      error: taskError,
      title: taskError.message,
      description: taskError.message,
      diagnostic: formatTaskErrorDiagnostic(taskError),
    };
  }
  return {
    error: taskError,
    title: translate(ERROR_TITLES[taskError.code]),
    description: taskError.message,
    diagnostic: formatTaskErrorDiagnostic(taskError),
  };
}

export async function copyTaskErrorDiagnostic(diagnostic: string): Promise<void> {
  await navigator.clipboard.writeText(diagnostic);
}

function formatTaskErrorDiagnostic(error: TaskError): string {
  return `[${error.code}] ${error.message}`;
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
