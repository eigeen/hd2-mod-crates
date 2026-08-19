import { describe, expect, test } from "bun:test";
import {
  patchFilesFromList,
  type PatchFileMessages,
  type PatchSidecarRequirements,
} from "../src/fileInputs";

const messages: PatchFileMessages = {
  noToc: "no TOC",
  multipleToc: "multiple TOCs",
  missingIntro: "missing sidecars",
  missingAction: (toc, gpu, stream) => `select ${toc}, ${gpu}, ${stream}`,
  missingSidecar: (filename, expected) => `${filename} missing ${expected}`,
  shortSidecar: (filename, expected, actual) => `${filename} needs ${expected}, found ${actual}`,
};

describe("Patch file loading", () => {
  test("accepts exact sidecar lengths from the shared validator", async () => {
    const patch = await patchFilesFromList(
      patchFiles(3, 5),
      messages,
      requirements({ gpu: "3", stream: "5" }),
    );

    expect(patch.gpu).toHaveLength(3);
    expect(patch.stream).toHaveLength(5);
  });

  test("reports a sidecar shorter than the shared requirement", async () => {
    const result = patchFilesFromList(
      patchFiles(3, 5),
      messages,
      requirements({ gpu: "3", stream: "6" }),
    );

    expect(result).rejects.toThrow("example.patch_0.stream needs 6, found 5");
  });

  test("rejects multiple main TOC files like Desktop", async () => {
    const result = patchFilesFromList(
      [file("first.patch_0"), file("second.patch_0")],
      messages,
      requirements({ gpu: "0", stream: "0" }),
    );

    expect(result).rejects.toThrow("multiple TOCs");
  });
});

function patchFiles(gpuLength: number, streamLength: number): File[] {
  return [
    file("example.patch_0"),
    file("example.patch_0.gpu_resources", gpuLength),
    file("example.patch_0.stream", streamLength),
  ];
}

function file(name: string, length = 0): File {
  return new File([new Uint8Array(length)], name);
}

function requirements(value: PatchSidecarRequirements) {
  return async () => value;
}
