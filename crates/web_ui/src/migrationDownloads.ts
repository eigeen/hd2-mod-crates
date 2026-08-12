import type {
  MigrationResult,
  MigrationSummary,
  TargetOption,
} from "./types";

export interface NumberedMigrationRequest {
  targetHashes: string[];
  targets: TargetOption[];
  migrateTarget: (
    targetHash: string,
    targetIndex: number,
    targetCount: number,
  ) => Promise<MigrationResult>;
  download: (bytes: Uint8Array, filename: string) => void;
}

/** Migrate and download one target at a time so completed ZIPs do not accumulate in WASM. */
export async function migrateTargetsToNumberedDownloads(
  request: NumberedMigrationRequest,
): Promise<MigrationSummary> {
  let summary = emptyMigrationSummary();
  for (const [index, targetHash] of request.targetHashes.entries()) {
    const result = await request.migrateTarget(targetHash, index, request.targetHashes.length);
    request.download(result.zipBytes, numberedZipFilename(targetHash, index, request));
    summary = appendMigrationSummary(summary, result.summary);
  }
  return summary;
}

export function numberedZipFilename(
  targetHash: string,
  targetIndex: number,
  request: Pick<NumberedMigrationRequest, "targetHashes" | "targets">,
): string {
  const target = request.targets.find((candidate) => candidate.hash === targetHash);
  const targetLabel = sanitizeFilenameSegment(target?.name ?? targetHash);
  const sequence = numberedPrefix(targetIndex, request.targetHashes.length);
  return `hd2-patch-${sequence}${targetLabel}.zip`;
}

function numberedPrefix(targetIndex: number, targetCount: number): string {
  if (targetCount === 1) return "";
  const width = Math.max(3, String(targetCount).length);
  const current = String(targetIndex + 1).padStart(width, "0");
  const total = String(targetCount).padStart(width, "0");
  return `${current}-of-${total}-`;
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
