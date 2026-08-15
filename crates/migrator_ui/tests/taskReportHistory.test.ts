import { expect, test } from "bun:test";
import {
  createCompletedTaskReport,
  prependTaskReport,
  type TaskReportInput,
} from "../src/taskReportHistory";

const input: TaskReportInput = {
  kind: "migration",
  output: "output.zip",
  summary: { migratedCount: 1, warningCount: 0, reports: [] },
};

test("new reports are ordered first and trimmed to the session limit", () => {
  const first = createCompletedTaskReport(input, "first", 1);
  const second = createCompletedTaskReport(input, "second", 2);
  const history = prependTaskReport([first], second, 1);

  expect(history.map((report) => report.id)).toEqual(["second"]);
  expect(first.completedAt).toBe(1);
});
