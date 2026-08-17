import { describe, expect, test } from "bun:test";
import {
  appendManualMergeBatch,
  ManualMergeInputError,
  moveManualMergeRecord,
} from "../src/manualMergeInputs";

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
