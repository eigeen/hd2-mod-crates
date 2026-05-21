export type PathField = "patchPath" | "dataDir" | "outDir";

export interface PathState {
  patchPath: string;
  dataDir: string;
  outDir: string;
}

export interface MigrationRequest extends PathState {
  targetFilter: string;
  noPadding: boolean;
  experimentalPartialRemap: boolean;
}

export interface MigrationSummary {
  migratedCount: number;
  warningCount: number;
  reports: MigrationReportRow[];
}

export interface MigrationReportRow {
  targetName: string;
  fileIdRemapped: number;
  slotIdRemapped: number;
  paddedUnits: number;
  warnings: string[];
}

export interface MigrationProgressEvent {
  status: string;
}

export interface GameDataDiscovery {
  dataDir: string | null;
  candidates: string[];
}
