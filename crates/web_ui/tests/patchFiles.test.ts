import { describe, expect, test } from "bun:test";
import {
  loadPatchFiles,
  patchFileSourcesFromList,
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

  test("retains large sidecars without reading their contents", async () => {
    let sidecarReads = 0;
    const gpu = sidecarFile("example.patch_0.gpu_resources", 4_000_000_000, () => {
      sidecarReads += 1;
    });
    const stream = sidecarFile("example.patch_0.stream", 3_000_000_000, () => {
      sidecarReads += 1;
    });

    const source = await patchFileSourcesFromList(
      [file("example.patch_0"), gpu, stream],
      messages,
      requirements({ gpu: "4000000000", stream: "3000000000" }),
    );

    expect(source.gpu).toBe(gpu);
    expect(source.stream).toBe(stream);
    expect(sidecarReads).toBe(0);
  });

  test("rejects a truncated sidecar before reading it", async () => {
    let sidecarReads = 0;
    const gpu = sidecarFile("example.patch_0.gpu_resources", 2, () => {
      sidecarReads += 1;
    });

    const result = patchFilesFromList(
      [file("example.patch_0"), gpu],
      messages,
      requirements({ gpu: "3", stream: "0" }),
    );

    expect(result).rejects.toThrow("example.patch_0.gpu_resources needs 3, found 2");
    await result.catch(() => undefined);
    expect(sidecarReads).toBe(0);
  });

  test("does not start another sidecar read after cancellation", async () => {
    const controller = new AbortController();
    let streamReads = 0;
    const result = loadPatchFiles({
      name: "example.patch_0",
      toc: new Uint8Array(),
      gpu: sidecarFile("example.patch_0.gpu_resources", 1, () => controller.abort()),
      stream: sidecarFile("example.patch_0.stream", 1, () => {
        streamReads += 1;
      }),
    }, controller.signal);

    expect(result).rejects.toMatchObject({ code: "task.cancelled" });
    await result.catch(() => undefined);
    expect(streamReads).toBe(0);
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

function sidecarFile(name: string, size: number, onRead: () => void): File {
  return {
    name,
    size,
    arrayBuffer: async () => {
      onRead();
      return new ArrayBuffer(0);
    },
  } as File;
}

function requirements(value: PatchSidecarRequirements) {
  return async () => value;
}
