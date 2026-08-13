import { describe, expect, test } from "bun:test";
import {
  batchZipFilename,
  migrateTargetsToBatchDownloads,
  migrateVariantsToBatchDownloads,
  migrationBatchSize,
  planMigrationBatches,
  planMigrationVariantBatches,
} from "../src/migrationDownloads";
import type { MigrationResult, MigrationVariant, TargetOption } from "../src/types";

const MIB = 1024 * 1024;
const targets = makeTargets(25);

describe("migrationBatchSize", () => {
  test.each([
    [25, 20],
    [50, 10],
    [100, 5],
    [151, 3],
    [200, 2],
    [600, 1],
  ])("uses %i MiB patches in batches of %i", (patchMiB, expected) => {
    expect(migrationBatchSize(patchMiB * MIB)).toBe(expected);
  });
});

test("partitions targets without reordering them", () => {
  const hashes = targets.slice(0, 23).map((target) => target.hash);
  const batches = planMigrationBatches(hashes, 50 * MIB);

  expect(batches.map((batch) => batch.targetHashes.length)).toEqual([10, 10, 3]);
  expect(batches.flatMap((batch) => batch.targetHashes)).toEqual(hashes);
  expect(batches.map((batch) => batch.targetOffset)).toEqual([0, 10, 20]);
});

test("splits the measured 151 MiB patch into safe three-variant batches", () => {
  const hashes = targets.slice(0, 7).map((target) => target.hash);
  const batches = planMigrationBatches(hashes, 158_659_188);

  expect(batches.map((batch) => batch.targetHashes.length)).toEqual([3, 3, 1]);
});

test("partitions combined variants without changing their order", () => {
  const variants = makeVariants(5);
  const batches = planMigrationVariantBatches(variants, 256 * MIB);

  expect(batches.map((batch) => batch.variants.length)).toEqual([2, 2, 1]);
  expect(batches.flatMap((batch) => batch.variants)).toEqual(variants);
});

describe("batchZipFilename", () => {
  test("combines the original and target names for one output", () => {
    const [batch] = planMigrationBatches([targets[1].hash], 50 * MIB);
    expect(batchZipFilename(batch, targets, "Original Mod")).toBe("Original Mod_Target_1.zip");
  });

  test("sanitizes package names and removes an existing zip suffix", () => {
    const [batch] = planMigrationBatches([targets[1].hash], 50 * MIB);
    expect(batchZipFilename(batch, targets, "Original:Mod.zip")).toBe("Original_Mod_Target_1.zip");
  });

  test("uses the combined filename when every target fits one batch", () => {
    const [batch] = planMigrationBatches(targets.slice(0, 2).map((target) => target.hash), 50 * MIB);
    expect(batchZipFilename(batch, targets, "Original Mod")).toBe("hd2-migrated-patch.zip");
  });

  test("numbers output batches rather than individual targets", () => {
    const batches = planMigrationBatches(targets.slice(0, 5).map((target) => target.hash), 256 * MIB);
    expect(batches.map((batch) => batchZipFilename(batch, targets, "Original Mod"))).toEqual([
      "hd2-patch-part-001-of-003.zip",
      "hd2-patch-part-002-of-003.zip",
      "hd2-patch-part-003-of-003.zip",
    ]);
  });
});

test("migrates, downloads, and summarizes batches sequentially", async () => {
  const events: string[] = [];
  const hashes = targets.slice(0, 5).map((target) => target.hash);
  const summary = await migrateTargetsToBatchDownloads({
    patchByteLength: 256 * MIB,
    sourceName: "Original Mod",
    targetHashes: hashes,
    targets,
    migrateBatch: async (batch) => {
      events.push(`start:${batch.targetHashes.join(",")}`);
      await Promise.resolve();
      events.push(`finish:${batch.targetHashes.join(",")}`);
      return migrationResult(batch.targetHashes);
    },
    onMultipleDownloads: (downloadCount) => events.push(`multiple:${downloadCount}`),
    download: (_bytes, filename) => events.push(`download:${filename}`),
  });

  expect(events).toEqual([
    "start:hash-0,hash-1",
    "finish:hash-0,hash-1",
    "multiple:3",
    "download:hd2-patch-part-001-of-003.zip",
    "start:hash-2,hash-3",
    "finish:hash-2,hash-3",
    "download:hd2-patch-part-002-of-003.zip",
    "start:hash-4",
    "finish:hash-4",
    "download:hd2-patch-part-003-of-003.zip",
  ]);
  expect(summary.migratedCount).toBe(5);
  expect(summary.warningCount).toBe(1);
  expect(summary.reports.map((report) => report.targetHash)).toEqual(hashes);
});

test("migrates combined variants in numbered memory-bounded batches", async () => {
  const downloads: string[] = [];
  const multipleDownloadNotices: number[] = [];
  const variants = makeVariants(5);
  const summary = await migrateVariantsToBatchDownloads({
    patchByteLength: 256 * MIB,
    variants,
    migrateBatch: async (batch) => migrationResult(
      batch.variants.map((variant) => variant.mappings[0].targetHash),
    ),
    onMultipleDownloads: (downloadCount) => multipleDownloadNotices.push(downloadCount),
    download: (_bytes, filename) => downloads.push(filename),
  });

  expect(downloads).toEqual([
    "hd2-patch-part-001-of-003.zip",
    "hd2-patch-part-002-of-003.zip",
    "hd2-patch-part-003-of-003.zip",
  ]);
  expect(multipleDownloadNotices).toEqual([3]);
  expect(summary.migratedCount).toBe(5);
});

test("does not show a multiple-download notice for one output file", async () => {
  const multipleDownloadNotices: number[] = [];
  const variants = makeVariants(1);

  await migrateVariantsToBatchDownloads({
    patchByteLength: 25 * MIB,
    variants,
    migrateBatch: async () => migrationResult([variants[0].mappings[0].targetHash]),
    onMultipleDownloads: (downloadCount) => multipleDownloadNotices.push(downloadCount),
    download: () => undefined,
  });

  expect(multipleDownloadNotices).toEqual([]);
});

function makeTargets(count: number): TargetOption[] {
  return Array.from({ length: count }, (_, index) => ({
    category: "Armor",
    hash: `hash-${index}`,
    name: index === 1 ? "Target/1" : `Target ${index}`,
    excluded: false,
  }));
}

function makeVariants(count: number): MigrationVariant[] {
  return Array.from({ length: count }, (_, index) => ({
    mappings: [{
      category: "Armor",
      sourceHash: "source",
      targetHash: `hash-${index}`,
    }],
  }));
}

function migrationResult(targetHashes: string[]): MigrationResult {
  const reports = targetHashes.map((targetHash) => ({
    targetHash,
    targetName: targetHash,
    fileIdRemapped: 1,
    slotIdRemapped: 0,
    paddedUnits: 0,
    skippedEntries: 0,
    warnings: targetHash === "hash-4" ? ["warning"] : [],
    mappings: [],
  }));
  return {
    zipBytes: new Uint8Array([1]),
    summary: {
      migratedCount: reports.length,
      warningCount: reports.flatMap((report) => report.warnings).length,
      reports,
    },
  };
}
