export const GAME_DATA_METHODS = [
  "readRange",
  "readFull",
  "exists",
  "listBundleChunks",
  "listPackages",
] as const;

export type GameDataMethod = typeof GAME_DATA_METHODS[number];

export type WasmTaskOperation =
  | "migrateEquipmentVariants"
  | "repatchUnits"
  | "mergePatches"
  | "mergePatchesToZip"
  | "mergeHeadIntoArmor";

export interface TaskStartMessage {
  type: "task.start";
  taskId: string;
  operation: WasmTaskOperation;
  payload: unknown;
}

export interface GameDataResponseMessage {
  type: "gameData.response";
  taskId: string;
  requestId: number;
  result?: unknown;
  error?: SerializedTaskError;
}

export type WorkerInputMessage = TaskStartMessage | GameDataResponseMessage;

export interface TaskSuccessMessage {
  type: "task.success";
  taskId: string;
  result: unknown;
}

export interface TaskFailureMessage {
  type: "task.failure";
  taskId: string;
  error: SerializedTaskError;
}

export interface TaskProgressMessage {
  type: "task.progress";
  taskId: string;
  event: MigrationProgressEvent;
}

export interface GameDataRequestMessage {
  type: "gameData.request";
  taskId: string;
  requestId: number;
  method: GameDataMethod;
  args: unknown[];
}

export type WorkerOutputMessage =
  | TaskSuccessMessage
  | TaskFailureMessage
  | TaskProgressMessage
  | GameDataRequestMessage;

export type MigrationProgressEvent =
  | { kind: "targetStart"; targetName: string; targetHash: string }
  | { kind: "stage"; targetName: string; stage: string }
  | { kind: "targetFinish"; targetName: string };

export interface SerializedTaskError {
  code: string;
  message: string;
}
