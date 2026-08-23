import { useCallback, useMemo, useState } from "react";
import type { MigrationSummary, UnitRepatchSummary } from "./types";

const HISTORY_STORAGE_KEY = "hd2-migrator-task-report-history-v1";
const HISTORY_STORAGE_VERSION = 1;

export type TaskReportInput =
  | { kind: "migration"; output: string; summary: MigrationSummary }
  | { kind: "repatch"; output: string; summary: UnitRepatchSummary };

export type CompletedTaskReport = TaskReportInput & {
  completedAt: number;
  id: string;
};

export function useTaskReportHistory(limit = 5) {
  const [history, setHistory] = useState<CompletedTaskReport[]>(
    () => loadTaskReportHistory(window.localStorage, limit),
  );
  const [activeId, setActiveId] = useState<string | null>(null);
  const activeReport = useMemo(
    () => history.find((report) => report.id === activeId) ?? null,
    [activeId, history],
  );
  const recordReport = useCallback((input: TaskReportInput) => {
    const report = createCompletedTaskReport(input, crypto.randomUUID(), Date.now());
    setHistory((current) => persistTaskReportHistory(
      window.localStorage,
      prependTaskReport(current, report, limit),
    ));
    setActiveId(report.id);
  }, [limit]);
  const clearHistory = useCallback(() => {
    removeTaskReportHistory(window.localStorage);
    setHistory([]);
    setActiveId(null);
  }, []);
  return {
    activeReport,
    clearHistory,
    closeReport: () => setActiveId(null),
    history,
    openReport: (id: string) => setActiveId(id),
    recordReport,
  };
}

export interface TaskReportStorage {
  getItem(key: string): string | null;
  removeItem(key: string): void;
  setItem(key: string, value: string): void;
}

export function loadTaskReportHistory(
  storage: TaskReportStorage,
  limit: number,
): CompletedTaskReport[] {
  try {
    const stored = JSON.parse(storage.getItem(HISTORY_STORAGE_KEY) ?? "null");
    if (!isHistoryEnvelope(stored)) return [];
    return stored.reports.filter(isCompletedTaskReport).slice(0, limit);
  } catch {
    return [];
  }
}

function persistTaskReportHistory(
  storage: TaskReportStorage,
  reports: CompletedTaskReport[],
): CompletedTaskReport[] {
  try {
    storage.setItem(HISTORY_STORAGE_KEY, JSON.stringify({
      version: HISTORY_STORAGE_VERSION,
      reports,
    }));
  } catch {
    // History is optional; a full or unavailable store must not fail a completed task.
  }
  return reports;
}

function removeTaskReportHistory(storage: TaskReportStorage): void {
  try {
    storage.removeItem(HISTORY_STORAGE_KEY);
  } catch {
    // Ignore unavailable browser storage.
  }
}

function isHistoryEnvelope(value: unknown): value is { version: 1; reports: unknown[] } {
  if (!isRecord(value)) return false;
  return value.version === HISTORY_STORAGE_VERSION && Array.isArray(value.reports);
}

function isCompletedTaskReport(value: unknown): value is CompletedTaskReport {
  if (!isRecord(value) || typeof value.id !== "string" || typeof value.output !== "string") return false;
  if (!Number.isFinite(value.completedAt)) return false;
  if (value.kind === "migration") return isMigrationSummary(value.summary);
  return value.kind === "repatch" && isRepatchSummary(value.summary);
}

function isMigrationSummary(value: unknown): value is MigrationSummary {
  if (!isRecord(value) || !hasNumbers(value, ["migratedCount", "warningCount"])) return false;
  return Array.isArray(value.reports) && value.reports.every(isMigrationReport);
}

function isMigrationReport(value: unknown): boolean {
  if (!isRecord(value) || typeof value.targetHash !== "string" || typeof value.targetName !== "string") return false;
  if (!hasNumbers(value, ["fileIdRemapped", "slotIdRemapped", "paddedUnits", "skippedEntries", "unmatchedUnits"])) return false;
  if (value.unmatchedUnitPolicy !== "keep" && value.unmatchedUnitPolicy !== "drop") return false;
  return isStringArray(value.warnings) && Array.isArray(value.mappings) && value.mappings.every(isMapping);
}

function isMapping(value: unknown): boolean {
  if (!isRecord(value) || (value.category !== "Armor" && value.category !== "Helmet")) return false;
  return typeof value.sourceHash === "string" && typeof value.targetHash === "string";
}

function isRepatchSummary(value: unknown): value is UnitRepatchSummary {
  if (!isRecord(value)) return false;
  const numbers = ["unitCount", "updatedUnits", "alreadyCurrentUnits", "removedUnits", "scannedArchives"];
  return hasNumbers(value, numbers) && isStringArray(value.warnings);
}

function hasNumbers(value: Record<string, unknown>, keys: string[]): boolean {
  return keys.every((key) => typeof value[key] === "number" && Number.isFinite(value[key]));
}

function isStringArray(value: unknown): value is string[] {
  return Array.isArray(value) && value.every((item) => typeof item === "string");
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object";
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
