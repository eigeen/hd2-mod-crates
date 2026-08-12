import { describe, expect, test } from "bun:test";
import {
  migrateTargetsToNumberedDownloads,
  numberedZipFilename,
} from "../src/migrationDownloads";
import type { MigrationResult, TargetOption } from "../src/types";

const targets: TargetOption[] = [
  { hash: "hash-a", name: "Alpha", excluded: false },
  { hash: "hash-b", name: "Bravo/Invalid", excluded: false },
];

describe("numberedZipFilename", () => {
  test("keeps the readable single-target filename", () => {
    expect(numberedZipFilename("hash-a", 0, { targetHashes: ["hash-a"], targets }))
      .toBe("hd2-patch-Alpha.zip");
  });

  test("adds stable numbering and sanitizes target names", () => {
    expect(numberedZipFilename("hash-b", 1, {
      targetHashes: ["hash-a", "hash-b"],
      targets,
    })).toBe("hd2-patch-002-of-002-Bravo_Invalid.zip");
  });
});

test("migrates, downloads, and summarizes targets sequentially", async () => {
  const events: string[] = [];
  const downloaded: string[] = [];
  const result = await migrateTargetsToNumberedDownloads({
    targetHashes: ["hash-a", "hash-b"],
    targets,
    migrateTarget: async (hash) => {
      events.push(`start:${hash}`);
      await Promise.resolve();
      events.push(`finish:${hash}`);
      return migrationResult(hash);
    },
    download: (_bytes, filename) => {
      events.push(`download:${filename}`);
      downloaded.push(filename);
    },
  });

  expect(events).toEqual([
    "start:hash-a",
    "finish:hash-a",
    "download:hd2-patch-001-of-002-Alpha.zip",
    "start:hash-b",
    "finish:hash-b",
    "download:hd2-patch-002-of-002-Bravo_Invalid.zip",
  ]);
  expect(downloaded).toHaveLength(2);
  expect(result.migratedCount).toBe(2);
  expect(result.warningCount).toBe(1);
  expect(result.reports.map((report) => report.targetHash)).toEqual(["hash-a", "hash-b"]);
});

function migrationResult(targetHash: string): MigrationResult {
  const warning = targetHash === "hash-b" ? ["warning"] : [];
  return {
    zipBytes: new Uint8Array([1]),
    summary: {
      migratedCount: 1,
      warningCount: warning.length,
      reports: [{
        targetHash,
        targetName: targetHash,
        fileIdRemapped: 1,
        slotIdRemapped: 0,
        paddedUnits: 0,
        skippedEntries: 0,
        warnings: warning,
      }],
    },
  };
}
