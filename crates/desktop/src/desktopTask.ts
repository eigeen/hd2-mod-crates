import type { DesktopTask } from "./desktopClient";

/** Bridge AbortSignal to the native cooperative cancellation command. */
export async function waitForDesktopTask<Result>(
  task: DesktopTask<Result>,
  signal: AbortSignal,
): Promise<Result> {
  const cancel = () => { void task.cancel(); };
  signal.addEventListener("abort", cancel, { once: true });
  if (signal.aborted) cancel();
  try {
    return await task.result;
  } finally {
    signal.removeEventListener("abort", cancel);
  }
}
