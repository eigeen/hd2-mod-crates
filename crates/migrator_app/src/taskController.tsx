import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import {
  normalizeTaskError,
  type TaskError,
  type TaskErrorCode,
  type TaskReportInput,
} from "@hd2-mod-tools/migrator-ui";

export type TaskPhase = "idle" | "preparing" | "running" | "cancelling" | "succeeded" | "cancelled" | "failed";

export interface TaskSnapshot {
  phase: TaskPhase;
  progressLabel: string;
}

export interface TaskExecution<Result> {
  acceptResultAfterCancel?: boolean;
  execute: (signal: AbortSignal, setProgressLabel: (label: string) => void) => Promise<Result | null>;
  fallbackErrorCode: TaskErrorCode;
  onSuccess?: (result: Result) => void;
  report?: (result: Result) => TaskReportInput;
}

export interface TaskController {
  busy: boolean;
  cancel: () => void;
  run: <Result>(execution: TaskExecution<Result>) => Promise<Result | null>;
  snapshot: TaskSnapshot;
}

type ReportError = (error: TaskError) => void;

export interface TaskControllerCore {
  activate: () => void;
  cancel: () => void;
  dispose: () => void;
  run: <Result>(execution: TaskExecution<Result>) => Promise<Result | null>;
}

/** Own one application task so navigation cannot discard its cancellation state. */
export function createTaskControllerCore(
  publish: (snapshot: TaskSnapshot) => void,
  reportError: ReportError,
  recordReport: (report: TaskReportInput) => void = () => undefined,
): TaskControllerCore {
  let active: AbortController | null = null;
  let disposed = false;
  let generation = 0;
  const publishWhileMounted = (snapshot: TaskSnapshot) => {
    if (!disposed) publish(snapshot);
  };
  return {
    activate: () => {
      disposed = false;
      generation += 1;
    },
    cancel: () => {
      if (!active) return;
      publishWhileMounted({ phase: "cancelling", progressLabel: "" });
      active.abort();
    },
    dispose: () => {
      disposed = true;
      generation += 1;
      active?.abort();
      active = null;
    },
    run: async <Result,>(execution: TaskExecution<Result>) => {
      if (disposed) return null;
      if (active) {
        reportError(normalizeTaskError({ code: "task.conflict", message: "Another task is already running" }));
        return null;
      }
      const controller = new AbortController();
      const runGeneration = generation;
      active = controller;
      const setProgressLabel = (progressLabel: string) => {
        if (active === controller && generation === runGeneration && !controller.signal.aborted) {
          publishWhileMounted({ phase: "running", progressLabel });
        }
      };
      publishWhileMounted({ phase: "preparing", progressLabel: "" });
      try {
        const result = await execution.execute(controller.signal, setProgressLabel);
        if (disposed || generation !== runGeneration || active !== controller) return null;
        if (controller.signal.aborted && !execution.acceptResultAfterCancel) {
          publishWhileMounted({ phase: "cancelled", progressLabel: "" });
          return null;
        }
        publishWhileMounted({ phase: result === null ? "cancelled" : "succeeded", progressLabel: "" });
        if (result !== null) {
          execution.onSuccess?.(result);
          if (execution.report) recordReport(execution.report(result));
        }
        return result;
      } catch (error) {
        if (disposed || generation !== runGeneration || active !== controller) return null;
        const normalized = normalizeTaskError(error, execution.fallbackErrorCode);
        publishWhileMounted({ phase: normalized.code === "task.cancelled" ? "cancelled" : "failed", progressLabel: "" });
        if (!disposed && generation === runGeneration && normalized.code !== "task.cancelled") reportError(normalized);
        return null;
      } finally {
        if (active === controller) active = null;
      }
    },
  };
}

const TaskContext = createContext<TaskController | null>(null);

export function TaskControllerProvider({ children, recordReport, reportError }: {
  children: ReactNode;
  recordReport: (report: TaskReportInput) => void;
  reportError: ReportError;
}) {
  const [snapshot, setSnapshot] = useState<TaskSnapshot>({ phase: "idle", progressLabel: "" });
  const reportErrorRef = useRef(reportError);
  reportErrorRef.current = reportError;
  const recordReportRef = useRef(recordReport);
  recordReportRef.current = recordReport;
  const core = useMemo(() => createTaskControllerCore(
    setSnapshot,
    (error) => reportErrorRef.current(error),
    (report) => recordReportRef.current(report),
  ), []);
  useEffect(() => {
    core.activate();
    return () => core.dispose();
  }, [core]);
  const cancel = useCallback(() => core.cancel(), [core]);
  const run = useCallback(<Result,>(execution: TaskExecution<Result>) => core.run(execution), [core]);
  const busy = snapshot.phase === "preparing" || snapshot.phase === "running" || snapshot.phase === "cancelling";
  const value = useMemo(() => ({ busy, cancel, run, snapshot }), [busy, cancel, run, snapshot]);
  return <TaskContext.Provider value={value}>{children}</TaskContext.Provider>;
}

export function useTaskController(): TaskController {
  const controller = useContext(TaskContext);
  if (!controller) throw new Error("TaskControllerProvider is missing");
  return controller;
}
