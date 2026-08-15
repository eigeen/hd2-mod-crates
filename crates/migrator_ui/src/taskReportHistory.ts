import { useCallback, useMemo, useState } from "react";
import type { MigrationSummary, UnitRepatchSummary } from "./types";

export type TaskReportInput =
  | { kind: "migration"; output: string; summary: MigrationSummary }
  | { kind: "repatch"; output: string; summary: UnitRepatchSummary };

export type CompletedTaskReport = TaskReportInput & {
  completedAt: number;
  id: string;
};

export function useTaskReportHistory(limit = 5) {
  const [history, setHistory] = useState<CompletedTaskReport[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const activeReport = useMemo(
    () => history.find((report) => report.id === activeId) ?? null,
    [activeId, history],
  );
  const recordReport = useCallback((input: TaskReportInput) => {
    const report = createCompletedTaskReport(input, crypto.randomUUID(), Date.now());
    setHistory((current) => prependTaskReport(current, report, limit));
    setActiveId(report.id);
  }, [limit]);
  return {
    activeReport,
    closeReport: () => setActiveId(null),
    history,
    openReport: (id: string) => setActiveId(id),
    recordReport,
  };
}

export function createCompletedTaskReport(
  input: TaskReportInput,
  id: string,
  completedAt: number,
): CompletedTaskReport {
  return { ...input, completedAt, id };
}

export function prependTaskReport(
  history: CompletedTaskReport[],
  report: CompletedTaskReport,
  limit: number,
): CompletedTaskReport[] {
  return [report, ...history].slice(0, limit);
}
