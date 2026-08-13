import type {
  MigrationResult,
  MigrationSummary,
  TargetOption,
} from "./types";

const MIB = 1024 * 1024;
export const SAFE_BATCH_BYTES = 512 * MIB;
export const MAX_VARIANTS_PER_BATCH = 20;

export interface MigrationBatch {
  targetHashes: string[];
  targetOffset: number;
  targetCount: number;
  batchIndex: number;
  batchCount: number;
}

export interface BatchedMigrationRequest {
  patchByteLength: number;
  targetHashes: string[];
  targets: TargetOption[];
  migrateBatch: (batch: MigrationBatch) => Promise<MigrationResult>;
  download: (bytes: Uint8Array, filename: string) => void;
}

/** Migrate targets in memory-bounded batches and download each completed ZIP immediately. */
export async function migrateTargetsToBatchDownloads(
  request: BatchedMigrationRequest,
): Promise<MigrationSummary> {
  const batches = planMigrationBatches(request.targetHashes, request.patchByteLength);
  let summary = emptyMigrationSummary();
  for (const batch of batches) {
    const result = await request.migrateBatch(batch);
    request.download(result.zipBytes, batchZipFilename(batch, request.targets));
    summary = appendMigrationSummary(summary, result.summary);
  }
  return summary;
}

/** Limit each batch to 512 MiB of source-size-equivalent work and at most 20 variants. */
export function migrationBatchSize(patchByteLength: number): number {
  const safeLength = Math.max(1, Math.floor(patchByteLength));
  const sizeBound = Math.floor(SAFE_BATCH_BYTES / safeLength);
  return Math.max(1, Math.min(MAX_VARIANTS_PER_BATCH, sizeBound));
}

export function planMigrationBatches(
  targetHashes: string[],
  patchByteLength: number,
): MigrationBatch[] {
  const batchSize = migrationBatchSize(patchByteLength);
  const batchCount = Math.ceil(targetHashes.length / batchSize);
  return Array.from({ length: batchCount }, (_, batchIndex) => {
    const targetOffset = batchIndex * batchSize;
    return {
      targetHashes: targetHashes.slice(targetOffset, targetOffset + batchSize),
      targetOffset,
      targetCount: targetHashes.length,
      batchIndex,
      batchCount,
    };
  });
}

export function batchZipFilename(batch: MigrationBatch, targets: TargetOption[]): string {
  if (batch.targetCount === 1) {
    return singleTargetFilename(batch.targetHashes[0], targets);
  }
  if (batch.batchCount === 1) return "hd2-migrated-patch.zip";
  const current = paddedBatchNumber(batch.batchIndex + 1, batch.batchCount);
  const total = paddedBatchNumber(batch.batchCount, batch.batchCount);
  return `hd2-patch-part-${current}-of-${total}.zip`;
}

function singleTargetFilename(targetHash: string, targets: TargetOption[]): string {
  const target = targets.find((candidate) => candidate.hash === targetHash);
  const targetLabel = sanitizeFilenameSegment(target?.name ?? targetHash);
  return `hd2-patch-${targetLabel}.zip`;
}

function paddedBatchNumber(value: number, batchCount: number): string {
  return String(value).padStart(Math.max(3, String(batchCount).length), "0");
}

function sanitizeFilenameSegment(value: string): string {
  const sanitized = value.replace(/[\\/:*?"<>|]/g, "_").trim();
  return sanitized || "target";
}

function emptyMigrationSummary(): MigrationSummary {
  return { migratedCount: 0, warningCount: 0, reports: [] };
}

function appendMigrationSummary(
  current: MigrationSummary,
  next: MigrationSummary,
): MigrationSummary {
  return {
    migratedCount: current.migratedCount + next.migratedCount,
    warningCount: current.warningCount + next.warningCount,
    reports: [...current.reports, ...next.reports],
  };
}
