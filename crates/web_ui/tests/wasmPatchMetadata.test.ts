import { expect, test } from "bun:test";
import type { PatchFiles } from "@hd2-mod-tools/migrator-ui";
import type { GameDataSource } from "../src/gameDataSource";
import {
  analyzeEquipmentContents,
  inspectEquipmentContents,
  patchSidecarRequirements,
  previewEquipmentMappings,
} from "../src/wasmClient";

test("inspects reusable TOC metadata in real Workers without cloning sidecars or opening game indexes", async () => {
  const patch = emptyPatch();
  Object.defineProperties(patch, {
    gpu: { get: () => { throw new Error("Inspection must not access GPU data"); } },
    stream: { get: () => { throw new Error("Inspection must not access stream data"); } },
  });
  let gameReads = 0;
  const unexpectedRead = async () => {
    gameReads += 1;
    throw new Error("An unambiguous patch does not need game data");
  };
  const source = {
    readFull: unexpectedRead, readRange: unexpectedRead, exists: unexpectedRead,
    listBundleChunks: unexpectedRead, listPackages: unexpectedRead,
  } as unknown as GameDataSource;

  for (let attempt = 0; attempt < 3; attempt += 1) {
    expect(await patchSidecarRequirements(patch.toc, 0)).toEqual({ gpu: "0", stream: "0" });
    const analysis = await analyzeEquipmentContents(patch, source);
    expect(analysis.inspection.sources).toEqual([]);
    expect(analysis.equipmentGraph.patch).toMatchObject({ name: patch.name, unitCount: 0 });
  }
  expect(await inspectEquipmentContents(patch)).toMatchObject({ sources: [] });
  expect(await previewEquipmentMappings(patch, [])).toEqual([]);
  expect(gameReads).toBe(0);
  expect(patch.toc).toHaveLength(72);
}, 30_000);

test("metadata Worker failures remain recoverable for the next import", async () => {
  await expect(patchSidecarRequirements(new Uint8Array([1, 2, 3]), 0))
    .rejects.toMatchObject({ code: "patch.inspectFailed" });
  expect(await patchSidecarRequirements(emptyPatch().toc, 0)).toEqual({ gpu: "0", stream: "0" });
}, 30_000);

test("a cancelled metadata request cannot start another import task", async () => {
  const controller = new AbortController();
  controller.abort();
  await expect(analyzeEquipmentContents(emptyPatch(), undefined, { signal: controller.signal }))
    .rejects.toMatchObject({ code: "task.cancelled" });
});

function emptyPatch(): PatchFiles {
  const toc = new Uint8Array(72);
  new DataView(toc.buffer).setUint32(0, 0xf0000011, true);
  return { name: "empty.patch_0", toc, gpu: new Uint8Array(), stream: new Uint8Array() };
}
