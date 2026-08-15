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
} from "../../web_ui/src/types";

import type {
  DetectedSource,
  UnifiedMigrateOptions,
  UnitRepatchOptions,
} from "../../web_ui/src/types";

export interface PatchDescriptor {
  path: string;
  name: string;
  originalName: string | null;
  byteLength: number;
}

export interface InspectPatchResult {
  patch: PatchDescriptor;
  inspection: { sources: DetectedSource[] };
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

export interface MigrationProgressEvent {
  targetName: string;
  targetHash: string;
  stage: string;
  kind: "targetStart" | "stage" | "targetFinish";
}
