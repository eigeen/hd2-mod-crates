import { expect, test } from "bun:test";
import {
  createCompletedTaskReport,
  loadTaskReportHistory,
  prependTaskReport,
  type TaskReportStorage,
  type TaskReportInput,
} from "../src/taskReportHistory";

const input: TaskReportInput = {
  kind: "migration",
  output: "output.zip",
  summary: { migratedCount: 1, warningCount: 0, reports: [] },
};

test("new reports are ordered first and trimmed to the history limit", () => {
  const first = createCompletedTaskReport(input, "first", 1);
  const second = createCompletedTaskReport(input, "second", 2);
  const history = prependTaskReport([first], second, 1);

  expect(history.map((report) => report.id)).toEqual(["second"]);
  expect(first.completedAt).toBe(1);
});

test("loads valid versioned history and ignores corrupt entries", () => {
  const valid = createCompletedTaskReport(input, "valid", 2);
  const storage = memoryStorage(JSON.stringify({
    version: 1,
    reports: [{ kind: "migration", id: "broken" }, valid],
  }));

  expect(loadTaskReportHistory(storage, 5)).toEqual([valid]);
});

test("ignores malformed and unknown-version history", () => {
  expect(loadTaskReportHistory(memoryStorage("not json"), 5)).toEqual([]);
  expect(loadTaskReportHistory(memoryStorage('{"version":2,"reports":[]}'), 5)).toEqual([]);
});

function memoryStorage(value: string | null): TaskReportStorage {
  return {
    getItem: () => value,
    removeItem: () => undefined,
    setItem: () => undefined,
  };
}
