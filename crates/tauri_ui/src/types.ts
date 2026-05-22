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

export interface MigrationTargetOption {
  hash: string;
  name: string;
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

export interface SvdPackRequest {
  inputDir: string;
  baseVariant: string;
  outputDir: string;
  packagePath: string | null;
  compressionLevel: number;
  jobs: number | null;
}

export interface SvdPackSummary {
  outputDir: string;
  packagePath: string | null;
}

export interface SvdPackageSummary {
  modName: string | null;
  baseVariant: string;
  variants: string[];
}

export interface SvdExportRequest {
  packagePath: string;
  outputZip: string;
  allVariants: boolean;
  variants: string[];
  jobs: number | null;
}

export interface SvdExportSummary {
  outputZip: string;
  variantCount: number;
}
