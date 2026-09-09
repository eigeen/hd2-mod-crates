import { expect, test } from "bun:test";
import type { PatchFiles, PatchMergeSummary } from "@hd2-mod-tools/migrator-ui";
import { runWasmWorkerTask, type TaskWorker } from "../src/worker/taskWorkerClient";
import { mergePatches, mergePatchesToZip } from "../src/wasmClient";

test("runs a minimal merge in the real WASM task Worker", async () => {
  const patch = emptyPatch("source.patch_0");
  const result = await runWasmWorkerTask<{
    patch: PatchFiles;
    summary: PatchMergeSummary;
  }>("mergePatches", {
    inputs: [patch],
    outputName: "merged.patch_0",
  }, {
    workerFactory: () => new Worker(
      new URL("../src/worker/wasmTask.worker.ts", import.meta.url),
      { type: "module" },
    ) as unknown as TaskWorker,
  });

  expect(result.patch.name).toBe("merged.patch_0");
  expect(result.patch.toc.byteLength).toBe(72);
  expect(result.summary).toMatchObject({ inputCount: 1, resourceCount: 0 });
  expect(patch.toc.byteLength).toBe(72);
}, 30_000);

test("streams a normal merge ZIP with the buffered bytes and replacement order", async () => {
  const inputs = [entryPatch("low.patch_0", [1, 2]), entryPatch("high.patch_0", [3, 4])];
  const buffered = await mergePatches(inputs, "merged.patch_0");
  const streamed = await mergePatchesToZip(inputs, "merged.patch_0");
  const entries = await storedZipEntries(streamed.zipBlob);

  expect(entries.get("merged.patch_0")).toEqual(buffered.patch.toc);
  expect(entries.get("merged.patch_0.gpu_resources")).toEqual(buffered.patch.gpu);
  expect(entries.get("merged.patch_0.stream")).toEqual(buffered.patch.stream);
  expect(entries.get("merged.patch_0.gpu_resources")).toHaveLength(0);
  expect(entries.get("merged.patch_0.stream")).toHaveLength(0);
  expect(streamed.summary).toEqual(buffered.summary);
  expect(streamed.summary).toMatchObject({
    conflictCount: 1,
    inputCount: 2,
    resourceCount: 1,
    sources: [{ replacedResourceCount: 0 }, { replacedResourceCount: 1 }],
  });
  expect(buffered.patch.toc.slice(184, 186)).toEqual(new Uint8Array([3, 4]));
}, 30_000);

function emptyPatch(name: string): PatchFiles {
  const toc = new Uint8Array(72);
  new DataView(toc.buffer).setUint32(0, 0xf0000011, true);
  return {
    name,
    toc,
    gpu: new Uint8Array(),
    stream: new Uint8Array(),
  };
}

function entryPatch(name: string, body: number[]): PatchFiles {
  const toc = new Uint8Array(256);
  const view = new DataView(toc.buffer);
  view.setUint32(0, 0xf0000011, true);
  view.setUint32(4, 1, true);
  view.setUint32(8, 1, true);
  view.setBigUint64(80, 1n, true);
  view.setBigUint64(88, 1n, true);
  view.setUint32(96, 16, true);
  view.setUint32(100, 64, true);
  view.setBigUint64(104, 7n, true);
  view.setBigUint64(112, 1n, true);
  view.setBigUint64(120, 184n, true);
  view.setUint32(160, body.length, true);
  view.setUint32(172, 16, true);
  view.setUint32(176, 64, true);
  view.setUint32(180, 1, true);
  toc.set(body, 184);
  return { name, toc, gpu: new Uint8Array(), stream: new Uint8Array() };
}

async function storedZipEntries(blob: Blob): Promise<Map<string, Uint8Array>> {
  const bytes = new Uint8Array(await blob.arrayBuffer());
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const decoder = new TextDecoder();
  const entries = new Map<string, Uint8Array>();
  let offset = 0;
  while (offset + 30 <= bytes.length && view.getUint32(offset, true) === 0x04034b50) {
    const size = view.getUint32(offset + 18, true);
    const nameLength = view.getUint16(offset + 26, true);
    const extraLength = view.getUint16(offset + 28, true);
    const nameStart = offset + 30;
    const dataStart = nameStart + nameLength + extraLength;
    const name = decoder.decode(bytes.subarray(nameStart, nameStart + nameLength));
    entries.set(name, bytes.slice(dataStart, dataStart + size));
    offset = dataStart + size;
  }
  return entries;
}
