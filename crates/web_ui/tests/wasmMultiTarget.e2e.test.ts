import { expect, test } from "bun:test";
import { open, readdir, readFile, stat } from "node:fs/promises";
import path from "node:path";
import {
  builtin_equipment_options,
  initSync,
  migrate_equipment_variants,
} from "../src/wasm/hd2_migrator_wasm/hd2_migrator_wasm.js";
import { StoreZipBuilder } from "../src/fileInputs";
import type { EquipmentOption, UnifiedMigrateOptions } from "../src/types";

const runE2e = process.env.HD2_WASM_E2E === "1";
const gameDataDir = process.env.HD2_TEST_DATA_DIR
  ?? String.raw`C:\Program Files (x86)\Steam\steamapps\common\Helldivers 2\data`;
const fixtureDir = path.resolve(
  import.meta.dir,
  "../../../test_files/SSD'S Stylized Dune 15086 0.1 2026-08-13T05-50Z IzUPRhJHc",
);
const patchName = "9ba626afa44a3aa3.patch_0";
const sourceHash = "5fb542484cb2c2a5";

(runE2e ? test : test.skip)(
  "streams the large fixture through WASM without retaining completed variants",
  async () => {
    await initializeWasm();
    const variantCount = Number(process.env.HD2_E2E_VARIANT_COUNT ?? 10);
    const singlePatch = process.env.HD2_E2E_SINGLE_PATCH === "1";
    const patch = await loadPatch();
    const options = migrationOptions(variantCount, singlePatch);
    let outputCount = 0;
    let maxOutputBytes = 0;
    let totalOutputBytes = 0;
    let outputFingerprint = 0xcbf29ce484222325n;
    const hashOutput = process.env.HD2_E2E_HASH_OUTPUT !== "0";
    const zip = process.env.HD2_E2E_BUILD_ZIP === "1" ? new StoreZipBuilder() : null;

    const result = await migrate_equipment_variants(
      patchName,
      patch.toc,
      patch.gpu,
      patch.stream,
      options,
      nativeDataSource(gameDataDir),
      {
        onTargetStart: () => undefined,
        onFile: (outputPath: string, bytes: Uint8Array, crc32: number) => {
          outputCount += 1;
          maxOutputBytes = Math.max(maxOutputBytes, bytes.byteLength);
          totalOutputBytes += bytes.byteLength;
          if (hashOutput) {
            outputFingerprint = mixFingerprint(outputFingerprint, outputPath, bytes);
          }
          zip?.add(outputPath, bytes, crc32);
        },
      },
    ) as { summary: { migratedCount: number } };

    expect(result.summary.migratedCount).toBe(singlePatch ? 1 : variantCount);
    expect(outputCount).toBe((singlePatch ? 1 : variantCount) * 3);
    expect(maxOutputBytes).toBeGreaterThan(0);
    expect(totalOutputBytes).toBeGreaterThan(maxOutputBytes);
    if (zip) expect(zip.finish().size).toBeGreaterThan(totalOutputBytes);
    console.info({
      maxOutputBytes,
      outputFingerprint: hashOutput ? outputFingerprint.toString(16) : "disabled",
      totalOutputBytes,
      variantCount,
      singlePatch,
    });
  },
  300_000,
);

async function initializeWasm(): Promise<void> {
  const wasmPath = path.resolve(
    import.meta.dir,
    "../src/wasm/hd2_migrator_wasm/hd2_migrator_wasm_bg.wasm",
  );
  initSync({ module: await readFile(wasmPath) });
}

function migrationOptions(
  variantCount: number,
  singlePatch: boolean,
): UnifiedMigrateOptions {
  const targets = (builtin_equipment_options() as EquipmentOption[])
    .filter((target) => target.category === "Armor")
    .filter((target) => !target.excluded && target.hash !== sourceHash)
    .slice(0, variantCount);
  expect(targets).toHaveLength(variantCount);
  const mappings = targets.map((target) => ({
    category: "Armor" as const,
    sourceHash,
    targetHash: target.hash,
  }));
  return {
    variants: singlePatch ? [{ mappings }] : mappings.map((mapping) => ({ mappings: [mapping] })),
    patchSuffix: patchName,
    noPadding: false,
    unmatchedUnitPolicy: "keep",
  };
}

async function loadPatch() {
  return {
    toc: new Uint8Array(await readFile(path.join(fixtureDir, patchName))),
    gpu: new Uint8Array(await readFile(path.join(fixtureDir, `${patchName}.gpu_resources`))),
    stream: new Uint8Array(await readFile(path.join(fixtureDir, `${patchName}.stream`))),
  };
}

function nativeDataSource(base: string) {
  return {
    readFull: async (relativePath: string) =>
      new Uint8Array(await readFile(path.join(base, relativePath))),
    readRange: async (relativePath: string, offset: number, length: number) => {
      const file = await open(path.join(base, relativePath), "r");
      try {
        const bytes = new Uint8Array(length);
        const result = await file.read(bytes, 0, length, offset);
        if (result.bytesRead !== length) throw new Error("short range read");
        return bytes;
      } finally {
        await file.close();
      }
    },
    exists: async (relativePath: string) => {
      try {
        return (await stat(path.join(base, relativePath))).isFile();
      } catch {
        return false;
      }
    },
    listBundleChunks: async () => (await readdir(base))
      .filter((name) => /^bundles\.\d{2}\.nxa$/.test(name))
      .sort(),
    listPackages: async () => (await readdir(base))
      .filter((name) => /^[0-9a-f]{16}$/i.test(name))
      .sort(),
  };
}

function mixFingerprint(current: bigint, outputPath: string, bytes: Uint8Array): bigint {
  const fileHash = Bun.hash(bytes) ^ Bun.hash(outputPath);
  return BigInt.asUintN(64, (current ^ fileHash) * 0x100000001b3n);
}
