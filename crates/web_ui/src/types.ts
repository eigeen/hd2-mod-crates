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

export interface MetadataState {
  json: string;
  source: string;
  targetCount: number;
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

export interface DirectoryArchiveInput {
  hash: string;
  name: string;
  toc: Uint8Array;
}

export interface FileSystemFileHandle {
  getFile(): Promise<File>;
}

export interface FileSystemDirectoryHandle {
  getFileHandle(name: string): Promise<FileSystemFileHandle>;
}

export interface WindowWithDirectoryPicker extends Window {
  showDirectoryPicker?: () => Promise<FileSystemDirectoryHandle>;
}
