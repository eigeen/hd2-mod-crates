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

test("loads persisted patch and head merge reports", () => {
  const merge = createCompletedTaskReport({
    kind: "merge",
    output: "merged.zip",
    summary: {
      conflictCount: 1,
      duplicateCount: 2,
      inputCount: 2,
      repairedMetadataCount: 0,
      resourceCount: 3,
      sources: [],
    },
  }, "merge", 3);
  const headMerge = createCompletedTaskReport({
    kind: "headMerge",
    output: "head.zip",
    summary: {
      armorHash: "armor",
      armorName: "Armor",
      copiedResources: 2,
      helmetHash: "helmet",
      helmetName: "Helmet",
      mergedBodyUnits: 1,
      mergedLods: 2,
      mergedParts: [],
      remappedConflicts: 0,
      removedHelmetUnits: 1,
      skippedParts: [],
      warnings: [],
    },
  }, "head", 4);
  const storage = memoryStorage(JSON.stringify({ version: 1, reports: [headMerge, merge] }));
  expect(loadTaskReportHistory(storage, 5)).toEqual([headMerge, merge]);
});

test("rejects a persisted merge report with malformed source details", () => {
  const storage = memoryStorage(JSON.stringify({
    version: 1,
    reports: [{
      completedAt: 1,
      id: "bad-merge",
      kind: "merge",
      output: "bad.zip",
      summary: {
        conflictCount: 0,
        duplicateCount: 0,
        inputCount: 1,
        repairedMetadataCount: 0,
        resourceCount: 1,
        sources: [{}],
      },
    }],
  }));
  expect(loadTaskReportHistory(storage, 5)).toEqual([]);
});

function memoryStorage(value: string | null): TaskReportStorage {
  return {
    getItem: () => value,
    removeItem: () => undefined,
    setItem: () => undefined,
  };
}
