import { describe, expect, test } from "bun:test";
import {
  appendManualMergeBatch,
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
