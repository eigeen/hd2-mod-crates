import { describe, expect, test } from "bun:test";
import { createTaskControllerCore, type TaskSnapshot } from "../src/taskController";

describe("task controller", () => {
  test("cancellation discards a late uncommitted result and ignores late progress", async () => {
    const snapshots: TaskSnapshot[] = [];
    const errors: Error[] = [];
    const completion = deferred<string>();
    let progress: (label: string) => void = () => undefined;
    let succeeded = false;
    const controller = createTaskControllerCore(
      (snapshot) => snapshots.push(snapshot),
      (error) => errors.push(error),
    );
    const running = controller.run({
      execute: (_signal, setProgress) => {
        progress = setProgress;
        return completion.promise;
      },
      fallbackErrorCode: "merge.failed",
      onSuccess: () => { succeeded = true; },
    });

    controller.cancel();
    progress("late progress");
    completion.resolve("uncommitted");

    expect(await running).toBeNull();
    expect(succeeded).toBe(false);
    expect(errors).toEqual([]);
    expect(snapshots.map((snapshot) => snapshot.phase)).toEqual(["preparing", "cancelling", "cancelled"]);
  });

  test("an explicitly committed result wins over a late cancellation", async () => {
    const snapshots: TaskSnapshot[] = [];
    const completion = deferred<string>();
    let succeeded = "";
    const controller = createTaskControllerCore(
      (snapshot) => snapshots.push(snapshot),
      () => undefined,
    );
    const running = controller.run({
      acceptResultAfterCancel: true,
      execute: () => completion.promise,
      fallbackErrorCode: "migration.failed",
      onSuccess: (result) => { succeeded = result; },
    });

    controller.cancel();
    completion.resolve("saved.zip");

    expect(await running).toBe("saved.zip");
    expect(succeeded).toBe("saved.zip");
    expect(snapshots.at(-1)?.phase).toBe("succeeded");
  });

  test("a structured cancellation is silent and never records success", async () => {
    const errors: Error[] = [];
    let succeeded = false;
    const controller = createTaskControllerCore(() => undefined, (error) => errors.push(error));
    await controller.run({
      execute: async () => { throw { code: "task.cancelled", message: "cancelled" }; },
      fallbackErrorCode: "headMerge.failed",
      onSuccess: () => { succeeded = true; },
    });
    expect(errors).toEqual([]);
    expect(succeeded).toBe(false);
  });

  test("effect cleanup and setup leave a fresh controller generation usable", async () => {
    const snapshots: TaskSnapshot[] = [];
    const controller = createTaskControllerCore((snapshot) => snapshots.push(snapshot), () => undefined);
    controller.activate();
    controller.dispose();
    controller.activate();
    expect(await controller.run({
      execute: async () => "ready",
      fallbackErrorCode: "migration.failed",
    })).toBe("ready");
    expect(snapshots.at(-1)?.phase).toBe("succeeded");
  });

  test("dispose suppresses completion callbacks and reports from the old generation", async () => {
    const completion = deferred<{ output: string; summary: never }>();
    let successCount = 0;
    let reportCount = 0;
    const controller = createTaskControllerCore(
      () => undefined,
      () => undefined,
      () => { reportCount += 1; },
    );
    const running = controller.run({
      acceptResultAfterCancel: true,
      execute: () => completion.promise,
      fallbackErrorCode: "merge.failed",
      onSuccess: () => { successCount += 1; },
      report: (result) => ({ kind: "merge", ...result }),
    });
    controller.dispose();
    completion.resolve({ output: "late.zip", summary: undefined as never });
    expect(await running).toBeNull();
    expect(successCount).toBe(0);
    expect(reportCount).toBe(0);
  });

  test("a rejected task from an old generation cannot overwrite a new task", async () => {
    const snapshots: TaskSnapshot[] = [];
    const errors: Error[] = [];
    const oldCompletion = rejectedDeferred();
    const controller = createTaskControllerCore(
      (snapshot) => snapshots.push(snapshot),
      (error) => errors.push(error),
    );
    const oldRun = controller.run({
      execute: () => oldCompletion.promise,
      fallbackErrorCode: "merge.failed",
    });
    controller.dispose();
    controller.activate();
    expect(await controller.run({
      execute: async () => "new result",
      fallbackErrorCode: "merge.failed",
    })).toBe("new result");
    oldCompletion.reject(new Error("late failure"));
    expect(await oldRun).toBeNull();
    expect(snapshots.at(-1)?.phase).toBe("succeeded");
    expect(errors).toEqual([]);
  });
});

function deferred<Value>() {
  let resolve!: (value: Value) => void;
  const promise = new Promise<Value>((done) => { resolve = done; });
  return { promise, resolve };
}

function rejectedDeferred() {
  let reject!: (error: unknown) => void;
  const promise = new Promise<string>((_resolve, fail) => { reject = fail; });
  return { promise, reject };
}
