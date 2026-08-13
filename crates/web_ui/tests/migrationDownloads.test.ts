import { describe, expect, test } from "bun:test";
import {
  batchZipFilename,
  migrateTargetsToBatchDownloads,
  migrationBatchSize,
  planMigrationBatches,
} from "../src/migrationDownloads";
import type { MigrationResult, TargetOption } from "../src/types";

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

describe("batchZipFilename", () => {
  test("keeps the readable single-target filename", () => {
    const [batch] = planMigrationBatches([targets[1].hash], 50 * MIB);
    expect(batchZipFilename(batch, targets)).toBe("hd2-patch-Target_1.zip");
  });

  test("uses the combined filename when every target fits one batch", () => {
    const [batch] = planMigrationBatches(targets.slice(0, 2).map((target) => target.hash), 50 * MIB);
    expect(batchZipFilename(batch, targets)).toBe("hd2-migrated-patch.zip");
  });

  test("numbers output batches rather than individual targets", () => {
    const batches = planMigrationBatches(targets.slice(0, 5).map((target) => target.hash), 256 * MIB);
    expect(batches.map((batch) => batchZipFilename(batch, targets))).toEqual([
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
    targetHashes: hashes,
    targets,
    migrateBatch: async (batch) => {
      events.push(`start:${batch.targetHashes.join(",")}`);
      await Promise.resolve();
      events.push(`finish:${batch.targetHashes.join(",")}`);
      return migrationResult(batch.targetHashes);
    },
    download: (_bytes, filename) => events.push(`download:${filename}`),
  });

  expect(events).toEqual([
    "start:hash-0,hash-1",
    "finish:hash-0,hash-1",
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

function makeTargets(count: number): TargetOption[] {
  return Array.from({ length: count }, (_, index) => ({
    hash: `hash-${index}`,
    name: index === 1 ? "Target/1" : `Target ${index}`,
    excluded: false,
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
