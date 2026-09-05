import { expect, test } from "bun:test";
import { waitForDesktopTask } from "../src/desktopTask";

test("desktop task bridge forwards cancellation and still returns a committed native result", async () => {
  const controller = new AbortController();
  const completion = deferred<string>();
  let cancelCount = 0;
  const waiting = waitForDesktopTask({
    cancel: async () => { cancelCount += 1; return true; },
    id: "task",
    result: completion.promise,
  }, controller.signal);

  controller.abort();
  completion.resolve("saved.zip");

  expect(await waiting).toBe("saved.zip");
  expect(cancelCount).toBe(1);
});

function deferred<Value>() {
  let resolve!: (value: Value) => void;
  const promise = new Promise<Value>((done) => { resolve = done; });
  return { promise, resolve };
}
