import type {
  MigrationResult,
  MigrationSummary,
  MigrationVariant,
  TargetOption,
} from "./types";

const MIB = 1024 * 1024;
export const SAFE_BATCH_BYTES = 1024 * MIB;
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
  sourceName: string;
  targetHashes: string[];
  targets: TargetOption[];
  migrateBatch: (batch: MigrationBatch) => Promise<MigrationResult>;
  download: (bytes: Uint8Array, filename: string) => void;
  onMultipleDownloads?: (downloadCount: number) => void;
}

export interface MigrationVariantBatch {
  variants: MigrationVariant[];
  variantOffset: number;
  variantCount: number;
  batchIndex: number;
  batchCount: number;
}

interface BatchedVariantMigrationRequest {
  patchByteLength: number;
  variants: MigrationVariant[];
  migrateBatch: (batch: MigrationVariantBatch) => Promise<MigrationResult>;
  download: (bytes: Uint8Array, filename: string) => void;
  onMultipleDownloads?: (downloadCount: number) => void;
}

/** Migrate targets in memory-bounded batches and download each completed ZIP immediately. */
export async function migrateTargetsToBatchDownloads(
  request: BatchedMigrationRequest,
): Promise<MigrationSummary> {
  const batches = planMigrationBatches(request.targetHashes, request.patchByteLength);
  let summary = emptyMigrationSummary();
  for (const batch of batches) {
    const result = await request.migrateBatch(batch);
    notifyMultipleDownloads(request.onMultipleDownloads, batch.batchCount, batch.batchIndex);
    request.download(
      result.zipBytes,
      batchZipFilename(batch, request.targets, request.sourceName),
    );
    summary = appendMigrationSummary(summary, result.summary);
  }
  return summary;
}

/** Migrate combined variants in memory-bounded batches without merging duplicate sources. */
export async function migrateVariantsToBatchDownloads(
  request: BatchedVariantMigrationRequest,
): Promise<MigrationSummary> {
  const batches = planMigrationVariantBatches(request.variants, request.patchByteLength);
  let summary = emptyMigrationSummary();
  for (const batch of batches) {
    const result = await request.migrateBatch(batch);
    notifyMultipleDownloads(request.onMultipleDownloads, batch.batchCount, batch.batchIndex);
    request.download(result.zipBytes, variantBatchFilename(batch));
    summary = appendMigrationSummary(summary, result.summary);
  }
  return summary;
}

function notifyMultipleDownloads(
  notify: ((downloadCount: number) => void) | undefined,
  downloadCount: number,
  downloadIndex: number,
): void {
  if (downloadIndex === 0 && downloadCount > 1) notify?.(downloadCount);
}

/** Limit each batch to 1 GiB of source-size-equivalent work and at most 20 variants. */
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

export function planMigrationVariantBatches(
  variants: MigrationVariant[],
  patchByteLength: number,
): MigrationVariantBatch[] {
  const batchSize = migrationBatchSize(patchByteLength);
  const batchCount = Math.ceil(variants.length / batchSize);
  return Array.from({ length: batchCount }, (_, batchIndex) => {
    const variantOffset = batchIndex * batchSize;
    return {
      variants: variants.slice(variantOffset, variantOffset + batchSize),
      variantOffset,
      variantCount: variants.length,
      batchIndex,
      batchCount,
    };
  });
}

export function batchZipFilename(
  batch: MigrationBatch,
  targets: TargetOption[],
  sourceName: string,
): string {
  if (batch.targetCount === 1) {
    return uniqueOutputFilename(sourceName, batch.targetHashes[0], targets);
  }
  if (batch.batchCount === 1) return "hd2-migrated-patch.zip";
  const current = paddedBatchNumber(batch.batchIndex + 1, batch.batchCount);
  const total = paddedBatchNumber(batch.batchCount, batch.batchCount);
  return `hd2-patch-part-${current}-of-${total}.zip`;
}

function variantBatchFilename(batch: MigrationVariantBatch): string {
  if (batch.batchCount === 1) return "hd2-migrated-patch.zip";
  const current = paddedBatchNumber(batch.batchIndex + 1, batch.batchCount);
  const total = paddedBatchNumber(batch.batchCount, batch.batchCount);
  return `hd2-patch-part-${current}-of-${total}.zip`;
}

export function uniqueOutputFilename(
  sourceName: string,
  targetHash: string,
  targets: TargetOption[],
): string {
  const target = targets.find((candidate) => candidate.hash === targetHash);
  const sourceLabel = sanitizeFilenameSegment(sourceName.replace(/\.zip$/i, ""));
  const targetLabel = sanitizeFilenameSegment(target?.name ?? targetHash);
  return `${sourceLabel}_${targetLabel}.zip`;
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
