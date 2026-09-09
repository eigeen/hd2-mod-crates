import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { StrictMode, act, type ReactNode } from "react";
import { createRoot, type Root } from "react-dom/client";
import {
  TaskControllerProvider,
  useTaskController,
  useTaskPhase,
  useTaskProgressLabel,
  type TaskController,
  type TaskPhase,
} from "../src/taskController";
import { installDomTestHarness } from "./domTestHarness";

interface RenderCounts {
  controller: number;
  phase: number;
  progress: number;
  staticPage: number;
}

interface RenderObservations {
  controller: TaskController | null;
  controllerReferences: TaskController[];
  counts: RenderCounts;
  phase: TaskPhase;
  progressLabel: string;
}

let cleanupDom: (() => void) | undefined;
let root: Root | undefined;
let container: HTMLElement;

beforeEach(() => {
  const harness = installDomTestHarness();
  container = harness.container;
  cleanupDom = harness.cleanup;
});

afterEach(async () => {
  try {
    if (root) await act(() => root?.unmount());
  } finally {
    root = undefined;
    cleanupDom?.();
    cleanupDom = undefined;
  }
});

describe("TaskControllerProvider render isolation", () => {
  test("frequent progress commits update only the progress consumer", async () => {
    const observations = createRenderObservations();
    await renderTaskConsumers(observations);
    const initialReferences = observations.controllerReferences.length;
    const taskCompletion = deferred<string>();
    let publishProgress: (label: string) => void = () => undefined;
    let runningTask!: Promise<string | null>;

    await act(() => {
      runningTask = observations.controller!.run({
        execute: (_signal, setProgressLabel) => {
          publishProgress = setProgressLabel;
          return taskCompletion.promise;
        },
        fallbackErrorCode: "migration.failed",
      });
    });
    expect(observations.phase).toBe("preparing");

    await act(() => publishProgress("item 0"));
    expect(observations.phase).toBe("running");
    const progressRendersBeforeCalibration = observations.counts.progress;
    await act(() => publishProgress("item 1"));
    const progressRendersPerCommit = observations.counts.progress - progressRendersBeforeCalibration;
    expect(progressRendersPerCommit).toBeGreaterThan(0);
    const runningCounts = { ...observations.counts };

    for (let index = 2; index <= 1001; index += 1) {
      await act(() => publishProgress(`item ${index}`));
    }

    expect(observations.progressLabel).toBe("item 1001");
    expect(observations.counts.progress - runningCounts.progress).toBe(1000 * progressRendersPerCommit);
    expect(observations.counts.controller).toBe(runningCounts.controller);
    expect(observations.counts.phase).toBe(runningCounts.phase);
    expect(observations.counts.staticPage).toBe(runningCounts.staticPage);

    const countsBeforeDuplicate = { ...observations.counts };
    await act(() => publishProgress("item 1001"));
    expect(observations.counts).toEqual(countsBeforeDuplicate);

    const stableRun = observations.controller!.run;
    const stableCancel = observations.controller!.cancel;
    taskCompletion.resolve("finished");
    await act(async () => { await runningTask; });

    expect(observations.phase).toBe("succeeded");
    expect(observations.progressLabel).toBe("");
    expect(observations.controller!.busy).toBe(false);
    expect(observations.controller!.run).toBe(stableRun);
    expect(observations.controller!.cancel).toBe(stableCancel);
    expect(observations.controllerReferences.slice(initialReferences).every((value) =>
      value.run === stableRun && value.cancel === stableCancel,
    )).toBe(true);
  });

  test("cancel publishes each visible state without changing controller functions", async () => {
    const observations = createRenderObservations();
    await renderTaskConsumers(observations);
    const taskCompletion = deferred<string>();
    let taskSignal: AbortSignal | undefined;
    let publishProgress: (label: string) => void = () => undefined;
    let runningTask!: Promise<string | null>;

    await act(() => {
      runningTask = observations.controller!.run({
        execute: (signal, setProgressLabel) => {
          taskSignal = signal;
          publishProgress = setProgressLabel;
          setProgressLabel("working");
          return taskCompletion.promise;
        },
        fallbackErrorCode: "migration.failed",
      });
    });
    const stableRun = observations.controller!.run;
    const stableCancel = observations.controller!.cancel;
    const controllerRendersWhileBusy = observations.counts.controller;

    await act(() => observations.controller!.cancel());
    expect(taskSignal?.aborted).toBe(true);
    expect(observations.phase).toBe("cancelling");
    expect(observations.progressLabel).toBe("");
    expect(observations.controller!.busy).toBe(true);
    expect(observations.counts.controller).toBe(controllerRendersWhileBusy);
    const countsAfterCancel = { ...observations.counts };
    await act(() => publishProgress("late progress"));
    expect(observations.counts).toEqual(countsAfterCancel);

    taskCompletion.resolve("discarded");
    await act(async () => { expect(await runningTask).toBeNull(); });
    expect(observations.phase).toBe("cancelled");
    expect(observations.controller!.busy).toBe(false);
    expect(observations.controller!.run).toBe(stableRun);
    expect(observations.controller!.cancel).toBe(stableCancel);
  });
});

function createRenderObservations(): RenderObservations {
  return {
    controller: null,
    controllerReferences: [],
    counts: { controller: 0, phase: 0, progress: 0, staticPage: 0 },
    phase: "idle",
    progressLabel: "",
  };
}

async function renderTaskConsumers(observations: RenderObservations) {
  root = createRoot(container);
  await act(() => root!.render(
    <StrictMode>
      <TaskControllerProvider recordReport={() => undefined} reportError={() => undefined}>
        <StaticPageProbe observations={observations}>
          <ControllerProbe observations={observations} />
          <PhaseProbe observations={observations} />
          <ProgressProbe observations={observations} />
        </StaticPageProbe>
      </TaskControllerProvider>
    </StrictMode>,
  ));
  expect(observations.controller).not.toBeNull();
  expect(observations.phase).toBe("idle");
}

function StaticPageProbe({ children, observations }: { children: ReactNode; observations: RenderObservations }) {
  observations.counts.staticPage += 1;
  return <main>{children}</main>;
}

function ControllerProbe({ observations }: { observations: RenderObservations }) {
  const controller = useTaskController();
  observations.counts.controller += 1;
  observations.controller = controller;
  observations.controllerReferences.push(controller);
  return <span data-busy={controller.busy} />;
}

function PhaseProbe({ observations }: { observations: RenderObservations }) {
  const phase = useTaskPhase();
  observations.counts.phase += 1;
  observations.phase = phase;
  return <span data-phase={phase} />;
}

function ProgressProbe({ observations }: { observations: RenderObservations }) {
  const progressLabel = useTaskProgressLabel();
  observations.counts.progress += 1;
  observations.progressLabel = progressLabel;
  return <span>{progressLabel}</span>;
}

function deferred<Value>() {
  let resolve!: (value: Value) => void;
  const promise = new Promise<Value>((done) => { resolve = done; });
  return { promise, resolve };
}
