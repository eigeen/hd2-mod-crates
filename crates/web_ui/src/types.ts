export type EquipmentCategory = "Armor" | "Helmet";

export interface EquipmentOption {
  category: EquipmentCategory;
  hash: string;
  name: string;
  excluded: boolean;
}

export type TargetOption = EquipmentOption;
export type MigrationCategory = EquipmentCategory;

export interface PatchInspection {
  sources: DetectedSource[];
}

export interface DetectedSource {
  id: string;
  category: EquipmentCategory;
  unitHits: number;
  candidates: EquipmentOption[];
  resolvedHash: string | null;
}

export interface PatchFiles {
  name: string;
  originalName?: string;
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

export interface MigrationMapping {
  category: EquipmentCategory;
  sourceHash: string;
  targetHash: string;
}

export interface MigrationVariant {
  mappings: MigrationMapping[];
}

export interface UnifiedMigrateOptions {
  variants: MigrationVariant[];
  patchSuffix: string | null;
  noPadding: boolean;
  unmatchedUnitPolicy: UnmatchedUnitPolicy;
}

export type UnmatchedUnitPolicy = "drop" | "keep";

export interface MigrationResult {
  zipBlob: Blob;
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
  mappings: MigrationMapping[];
}

export type MissingUnitPolicy = "drop" | "keep" | "fail";

export interface UnitRepatchOptions {
  missingUnitPolicy: MissingUnitPolicy;
}

export interface UnitRepatchResult {
  tocBytes: Uint8Array;
  summary: UnitRepatchSummary;
}

export interface UnitRepatchSummary {
  unitCount: number;
  updatedUnits: number;
  alreadyCurrentUnits: number;
  removedUnits: number;
  failedUnits: number;
  scannedArchives: number;
  warnings: string[];
}
