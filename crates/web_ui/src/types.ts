export interface TargetOption {
  hash: string;
  name: string;
}

export interface PatchFiles {
  name: string;
  toc: Uint8Array;
  gpu: Uint8Array;
  stream: Uint8Array;
}

// 仅供 UI 展示用的轻量元数据；patch 的字节存在 ref 里，避免 React DevTools 在 state 里枚举上百 MB 数据。
export interface PatchInfo {
  name: string;
}

export interface AuthorityMappings {
  armorCount: number;
  hasArmor: (name: string) => boolean;
}

export interface MigrateOptions {
  sourceHash: string | null;
  targetHashes: string[];
  patchSuffix: string | null;
  noPadding: boolean;
  experimentalPartialRemap: boolean;
}

export interface MigrationResult {
  zipBytes: Uint8Array;
  summary: MigrationSummary;
}

export interface MigrationSummary {
  migratedCount: number;
  warningCount: number;
  reports: MigrationReportRow[];
}

export interface MigrationReportRow {
  targetHash: string;
  targetName: string;
  fileIdRemapped: number;
  slotIdRemapped: number;
  paddedUnits: number;
  skippedEntries: number;
  warnings: string[];
}

