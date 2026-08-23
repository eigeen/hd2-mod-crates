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

export interface EquipmentPatchAnalysis {
  inspection: PatchInspection;
  equipmentGraph: EquipmentPartGraph;
}

export interface EquipmentMappingPreview {
  schemaVersion: number;
  sourceEquipment: GraphEquipment;
  targetEquipment: GraphEquipment;
  units: MappingPreviewUnit[];
  mappings: UnitMappingPreview[];
  summary: MappingPreviewSummary;
}

export interface MappingPreviewUnit {
  id: string;
  fileId: string;
  presentInPatch: boolean;
  sourceRoles: EquipmentPartRole[];
  targetRoles: EquipmentPartRole[];
}

export interface UnitMappingPreview {
  id: string;
  sourceUnitId: string;
  targetUnitId: string;
  role: EquipmentPartRole;
  action: "replace" | "reuse";
}

export interface MappingPreviewSummary {
  mappedUnitCount: number;
  replacedUnitCount: number;
  unchangedUnitCount: number;
  reusedSourceUnitCount: number;
}

export interface EquipmentPartGraph {
  schemaVersion: number;
  patch: EquipmentGraphSummary;
  equipments: GraphEquipment[];
  components: GraphComponent[];
  relations: EquipmentPartRelation[];
  diagnostics: EquipmentGraphDiagnostic[];
}

export interface EquipmentGraphSummary {
  name: string;
  unitCount: number;
  mappedUnitCount: number;
  unmappedUnitCount: number;
  equipmentCount: number;
  relationCount: number;
}

export interface GraphEquipment {
  id: string;
  category: EquipmentCategory;
  hash: string | null;
  name: string;
}

export interface GraphComponent {
  id: string;
  fileId: string;
  kind: "unit";
  presentInPatch: boolean;
}

export type EquipmentPartRole =
  | "slimWaist"
  | "stockyRightArm"
  | "slimRightArm"
  | "stockyBody"
  | "stockyWaist"
  | "slimLeftArm"
  | "stockyLeftArm"
  | "slimBody"
  | "leftLeg"
  | "rightLeg"
  | "helmet";

export interface EquipmentPartRelation {
  id: string;
  equipmentId: string;
  componentId: string;
  role: EquipmentPartRole;
}

export interface EquipmentGraphDiagnostic {
  code: "unmappedUnit";
  componentId: string;
  fileId: string;
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
  unitBehavior: UnitBehaviorOptions;
}

export type UnmatchedUnitPolicy = "drop" | "keep";

export interface UnitBehaviorOptions {
  disabledMappings: UnitMappingBehaviorKey[];
  exportOverrides: UnitExportOverride[];
  conflictResolutions: UnitConflictResolution[];
}

export interface UnitMappingBehaviorKey {
  sourceFileId: string;
  targetFileId: string;
}

export interface UnitExportOverride {
  fileId: string;
  export: boolean;
}

export interface UnitConflictResolution {
  targetFileId: string;
  preferredSourceFileId: string;
}

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
  unmatchedUnits: number;
  unmatchedUnitPolicy: UnmatchedUnitPolicy;
  warnings: string[];
  mappings: MigrationMapping[];
}

export type MissingUnitPolicy = "drop" | "keep" | "fail";

export interface UnitRepatchOptions {
  missingUnitPolicy: MissingUnitPolicy;
}

export interface UnitRepatchResult {
  tocBytes: Uint8Array;
  gpuBytes: Uint8Array | null;
  streamBytes: Uint8Array | null;
  summary: UnitRepatchSummary;
}

export interface UnitRepatchSummary {
  unitCount: number;
  updatedUnits: number;
  convertedFormats?: number;
  alreadyCurrentUnits: number;
  removedUnits: number;
  scannedArchives: number;
  warnings: string[];
}
