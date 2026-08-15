import { expect, test } from "bun:test";
import {
  TaskError,
  normalizeTaskError,
  throwIfTaskCancelled,
} from "../src/taskError";

test("preserves structured desktop command errors", () => {
  const error = normalizeTaskError({
    code: "migration.failed",
    message: "failed to parse patch",
  });

  expect(error).toBeInstanceOf(TaskError);
  expect(error.code).toBe("migration.failed");
  expect(error.message).toBe("failed to parse patch");
});

test("uses the caller fallback code for unstructured WASM errors", () => {
  const error = normalizeTaskError(new Error("bad archive"), "migration.failed");

  expect(error.code).toBe("migration.failed");
  expect(error.message).toBe("bad archive");
});

test("aborted tasks throw the shared cancellation code", () => {
  const controller = new AbortController();
  controller.abort();

  expect(() => throwIfTaskCancelled(controller.signal)).toThrow(
    expect.objectContaining({ code: "task.cancelled" }),
  );
});
