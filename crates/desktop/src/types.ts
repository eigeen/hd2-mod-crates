export type {
  DetectedSource,
  EquipmentOption,
  MigrationMapping,
  MigrationSummary,
  MigrationVariant,
  MissingUnitPolicy,
  UnmatchedUnitPolicy,
  UnitRepatchSummary,
  UnifiedMigrateOptions,
  HeadArmorMergeOptions,
  HeadArmorMergeSummary,
} from "@hd2-mod-tools/migrator-ui";

import type {
  DetectedSource,
  EquipmentMappingPreview,
  EquipmentPartGraph,
  HeadArmorMergeOptions,
  UnifiedMigrateOptions,
  UnitRepatchOptions,
} from "@hd2-mod-tools/migrator-ui";

export type { EquipmentMappingPreview };

export interface PatchDescriptor {
  path: string;
  name: string;
  originalName: string | null;
  byteLength: number;
}

export interface InspectPatchResult {
  patch: PatchDescriptor;
  inspection: { sources: DetectedSource[] };
  equipmentGraph: EquipmentPartGraph;
}

export interface GameDataDiscovery {
  dataDir: string | null;
  candidates: string[];
}

export interface MigrateRequest {
  patchPaths: string[];
  dataDir: string;
  outputPath: string;
  options: UnifiedMigrateOptions;
}

export interface RepatchRequest {
  patchPaths: string[];
  dataDir: string;
  outputPath: string;
  options: UnitRepatchOptions;
}

export interface HeadArmorMergeRequest {
  armorPaths: string[];
  dataDir: string | null;
  headPaths: string[] | null;
  outputPath: string;
  options: HeadArmorMergeOptions;
}

export interface MigrationProgressEvent {
  completedBytes: number;
  targetName: string;
  targetHash: string;
  stage: string;
  kind: "outputProgress" | "targetStart" | "stage" | "targetFinish";
  totalBytes: number;
}

export interface AppUpdateMetadata {
  currentVersion: string;
  date: string;
  notes: string | null;
  target: string;
  version: string;
}
