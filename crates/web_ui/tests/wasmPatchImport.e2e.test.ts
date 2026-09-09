import { beforeAll, expect, test } from "bun:test";
import { readFile } from "node:fs/promises";
import {
  initSync,
  merge_patches,
  patch_sidecar_requirements,
} from "../src/wasm/hd2_migrator_wasm/hd2_migrator_wasm.js";
import { appendManualMergeBatch, validateManualMergeRecords } from "../src/manualMergeInputs";

beforeAll(async () => {
  const bytes = await readFile(new URL(
    "../src/wasm/hd2_migrator_wasm/hd2_migrator_wasm_bg.wasm", import.meta.url,
  ));
  initSync({ module: bytes });
});

test("imports poisoned stream metadata and repairs it in the WASM merge output", async () => {
  const toc = poisonedUnitToc();
  const name = "9ba626afa44a3aa3.patch_0";
  const records = appendManualMergeBatch([], [new File([toc], name)]);
  await validateManualMergeRecords(records, (key) => key, async (bytes, streamLength) =>
    patch_sidecar_requirements(bytes, streamLength));

  const result = merge_patches([
    { name, toc, gpu: new Uint8Array(), stream: new Uint8Array() },
  ], "merged.patch_0");
  expect(result.summary.repairedMetadataCount).toBe(1);
  expect(new DataView(result.patch.toc.buffer).getUint32(104 + 60, true)).toBe(0);
  expect(new DataView(toc.buffer).getUint32(104 + 60, true)).toBe(3_752_217_746);
});

test("WASM import still rejects missing GPU data when stream metadata is repairable", async () => {
  const records = appendManualMergeBatch([], [new File([poisonedUnitToc(4)], "mod.patch_0")]);
  await expect(validateManualMergeRecords(records, (key) => key, async (bytes, streamLength) =>
    patch_sidecar_requirements(bytes, streamLength)))
    .rejects.toMatchObject({ code: "patch.inspectFailed" });
});

/** One Unit entry with the exact stream size reported in the failed import. */
function poisonedUnitToc(gpuSize = 0): Uint8Array {
  const toc = new Uint8Array(256);
  const view = new DataView(toc.buffer);
  const unitType = 16_187_218_042_980_615_487n;
  view.setUint32(0, 0xF0000011, true);
  view.setUint32(4, 1, true);
  view.setUint32(8, 1, true);
  view.setBigUint64(72 + 8, unitType, true);
  view.setBigUint64(72 + 16, 1n, true);
  view.setBigUint64(104, 42n, true);
  view.setBigUint64(104 + 8, unitType, true);
  view.setBigUint64(104 + 16, 184n, true);
  view.setUint32(104 + 60, 0xDFA64C92, true);
  view.setUint32(104 + 64, gpuSize, true);
  return toc;
}
