import { describe, expect, test } from "bun:test";
import { groupMergeInputFiles, type MergeInputFile } from "../src/mergeInputs";

describe("shared merge input grouping", () => {
  test("keeps same-named patches in different directories separate", () => {
    const groups = groupMergeInputFiles([
      input("mods/a/mod.patch_0", "a-main"),
      input("mods/a/mod.patch_0.stream", "a-stream"),
      input("mods/b/mod.patch_0", "b-main"),
      input("mods/b/mod.patch_0.stream", "b-stream"),
    ], ids());
    expect(groups).toHaveLength(2);
    expect(groups.map((group) => group.toc.source)).toEqual(["a-main", "b-main"]);
    expect(groups.map((group) => group.stream?.source)).toEqual(["a-stream", "b-stream"]);
  });

  test("groups directly selected main and sidecar files", () => {
    const groups = groupMergeInputFiles([
      input("mod.patch_0", "main"),
      input("mod.patch_0.gpu_resources", "gpu"),
    ], ids());
    expect(groups).toHaveLength(1);
    expect(groups[0].toc.source).toBe("main");
    expect(groups[0].gpu?.source).toBe("gpu");
  });

  test("preserves parent directory identity while matching basenames case-insensitively", () => {
    const groups = groupMergeInputFiles([
      input("A/SAME.patch_0", "upper"),
      input("a/same.patch_0", "lower"),
    ], ids());
    expect(groups.map((group) => group.toc.source)).toEqual(["upper", "lower"]);
  });
});

function input(relativePath: string, source: string): MergeInputFile<string> {
  return { name: relativePath.split("/").at(-1)!, relativePath, source };
}

function ids() {
  let next = 0;
  return () => `id-${++next}`;
}
