import { describe, expect, test } from "bun:test";
import {
  appendManualMergeBatch,
  loadManualMergePatch,
  ManualMergeInputError,
  moveManualMergeRecord,
  recordsFromMergeInputFiles,
  validateManualMergeRecords,
} from "../src/manualMergeInputs";
import { TaskError, type Translate } from "@hd2-mod-tools/migrator-ui";

describe("manual MOD merge inputs", () => {
  test("creates one record per main patch and leaves sidecars unset", () => {
    const records = appendManualMergeBatch([], [file("a.patch_1"), file("a.patch_2")], ids());
    expect(records.map((record) => record.name)).toEqual(["a.patch_1", "a.patch_2"]);
    expect(records[0].gpu).toBeUndefined();
    expect(records[0].stream).toBeUndefined();
  });

  test("groups matching files only within the same submission", () => {
    const records = appendManualMergeBatch(
      [],
      [file("a.patch_1.stream"), file("a.patch_1"), file("a.patch_1.gpu_resources")],
      ids(),
    );
    expect(records).toHaveLength(1);
    expect(records[0].gpu?.name).toBe("a.patch_1.gpu_resources");
    expect(records[0].stream?.name).toBe("a.patch_1.stream");
  });

  test("same named patches from separate submissions remain separate records", () => {
    const createId = ids();
    const first = appendManualMergeBatch([], [file("a.patch_0")], createId);
    const second = appendManualMergeBatch(first, [file("a.patch_0")], createId);
    expect(second).toHaveLength(2);
    expect(second[0].name).toBe(second[1].name);
    expect(second[0].id).not.toBe(second[1].id);
  });

  test("moves a record to the target position", () => {
    const records = appendManualMergeBatch(
      [],
      [file("a.patch_1"), file("a.patch_2"), file("a.patch_3")],
      ids(),
    );
    expect(moveManualMergeRecord(records, "id-1", "id-3").map((item) => item.id))
      .toEqual(["id-2", "id-3", "id-1"]);
  });

  test("rejects archive input", () => {
    expectBatchError([file("mod.zip")], "unsupported");
  });

  test("rejects a sidecar submitted without its main file", () => {
    expectBatchError([file("a.patch_1.stream")], "missingMain");
  });

  test("rejects a duplicate role inside one submission", () => {
    expectBatchError([file("a.patch_1"), file("a.patch_1", "duplicate")], "duplicate");
  });

  test("keeps normalized wrapper paths when browser Files have no relative path", () => {
    const sameFileNameA = file("same.patch_0");
    const sameFileNameB = file("same.patch_0");
    const records = recordsFromMergeInputFiles([
      { name: sameFileNameA.name, relativePath: "A/same.patch_0", source: sameFileNameA },
      { name: sameFileNameB.name, relativePath: "B/same.patch_0", source: sameFileNameB },
    ], ids());
    expect(records).toHaveLength(2);
  });

  test("reports a missing required sidecar during import", async () => {
    const records = appendManualMergeBatch([], [file("mod.patch_0", "toc")], ids());
    await expect(validateManualMergeRecords(records, translate, async () => ({ gpu: "4", stream: "0" })))
      .rejects.toMatchObject({ code: "patch.inspectFailed" });
  });

  test("checks sidecar size without reading its contents", async () => {
    const records = appendManualMergeBatch([], [
      file("mod.patch_0", "toc"),
      file("mod.patch_0.gpu_resources", "x"),
    ], ids());
    try {
      await validateManualMergeRecords(records, translate, async () => ({ gpu: "4", stream: "0" }));
      throw new Error("expected sidecar validation to fail");
    } catch (error) {
      expect(error).toBeInstanceOf(TaskError);
      expect((error as TaskError).message).toContain("mod.patch_0.gpu_resources needs 4, found 1");
    }
  });

  test("merge execution stops before loading sidecars when cancelled during the TOC read", async () => {
    const controller = new AbortController();
    const toc = file("mod.patch_0");
    const gpu = file("mod.patch_0.gpu_resources");
    let gpuReads = 0;
    toc.arrayBuffer = async () => {
      controller.abort();
      return new ArrayBuffer(0);
    };
    gpu.arrayBuffer = async () => { gpuReads += 1; return new ArrayBuffer(0); };
    const [record] = appendManualMergeBatch([], [toc, gpu], ids());
    await expect(loadManualMergePatch(record, controller.signal))
      .rejects.toMatchObject({ code: "task.cancelled" });
    expect(gpuReads).toBe(0);
  });

  test("checks a batch one file at a time instead of overlapping metadata tasks", async () => {
    const records = appendManualMergeBatch([], [file("a.patch_0"), file("b.patch_0")], ids());
    let active = 0;
    let peak = 0;
    await validateManualMergeRecords(records, translate, async () => {
      active += 1;
      peak = Math.max(peak, active);
      await new Promise((resolve) => setTimeout(resolve, 0));
      active -= 1;
      return { gpu: "0", stream: "0" };
    });
    expect(peak).toBe(1);
  });

  test("cancelling a batch prevents inspection of remaining files", async () => {
    const records = appendManualMergeBatch([], [file("a.patch_0"), file("b.patch_0")], ids());
    const controller = new AbortController();
    let inspected = 0;
    await expect(validateManualMergeRecords(records, translate, async () => {
      inspected += 1;
      controller.abort();
      return { gpu: "0", stream: "0" };
    }, { signal: controller.signal })).rejects.toMatchObject({ code: "task.cancelled" });
    expect(inspected).toBe(1);
  });

  test("passes actual stream length to shared repair validation without reading the stream", async () => {
    for (const stream of [undefined, file("mod.patch_0.stream", "stream")]) {
      const records = appendManualMergeBatch([], [
        file("mod.patch_0", "toc"),
        ...(stream ? [stream] : []),
      ], ids());
      if (stream) stream.arrayBuffer = async () => { throw new Error("must not read stream"); };
      let inspectedLength: number | undefined;
      await validateManualMergeRecords(records, translate, async (toc, streamLength) => {
        expect(new TextDecoder("utf-8").decode(toc)).toBe("toc");
        inspectedLength = streamLength;
        return { gpu: "0", stream: "0" };
      });
      expect(inspectedLength).toBe(stream?.size ?? 0);
    }
  });
});

function file(name: string, contents = "") {
  return new File([contents], name);
}

function ids() {
  let next = 0;
  return () => `id-${++next}`;
}

function expectBatchError(files: File[], code: ManualMergeInputError["code"]) {
  try {
    appendManualMergeBatch([], files, ids());
    throw new Error("expected batch validation to fail");
  } catch (error) {
    expect(error).toBeInstanceOf(ManualMergeInputError);
    expect((error as ManualMergeInputError).code).toBe(code);
  }
}

const translate: Translate = (key, values) => {
  if (key === "patch.missingIntro") return "missing sidecars";
  if (key === "patch.missingAction") return "select sidecars";
  if (key === "patch.missingSidecar") return `${values?.filename} missing ${values?.expected}`;
  if (key === "patch.shortSidecar") return `${values?.filename} needs ${values?.expected}, found ${values?.actual}`;
  return key;
};
