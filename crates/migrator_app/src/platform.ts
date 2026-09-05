import type { ComponentType, ReactNode } from "react";
import type {
  EquipmentMappingPreview,
  EquipmentOption,
  EquipmentPartGraph,
  MigrationMapping,
  MigrationSummary,
  MissingUnitPolicy,
  PatchInfo,
  PatchInspection,
  TaskErrorCode,
  Translate,
  UnitRepatchSummary,
  UnifiedMigrateOptions,
} from "@hd2-mod-tools/migrator-ui";

export type ToolMode = "migrate" | "repatch" | "merge" | "head-merge";

export interface SelectedPatch<PatchSource> {
  byteLength: number;
  info: PatchInfo;
  key: string;
  originalName?: string;
  source: PatchSource;
}

export interface SelectedGameData<GameDataSource> {
  key: string;
  label: string;
  ready: boolean;
  source: GameDataSource;
}

export interface PatchAnalysis {
  equipmentGraph: EquipmentPartGraph;
  inspection: PatchInspection;
}

export interface MappingLimits {
  separateOutputLimit: (patchByteLength: number) => number;
  singlePatchLimit: number;
}

export interface MigrationRunRequest<PatchSource, GameDataSource> {
  gameData: GameDataSource;
  options: UnifiedMigrateOptions;
  patch: SelectedPatch<PatchSource>;
  progressLabels: string[];
  signal: AbortSignal;
  suggestedOutputFilename: string;
  onProgressLabel: (label: string) => void;
}

export interface RepatchRunRequest<PatchSource, GameDataSource> {
  gameData: GameDataSource;
  missingUnitPolicy: MissingUnitPolicy;
  patch: SelectedPatch<PatchSource>;
  signal: AbortSignal;
  onProgressLabel: (label: string) => void;
}

export interface CompletedRun<Summary> {
  output: string;
  summary: Summary;
}

export interface MigratorPlatform<PatchSource, GameDataSource> {
  copyDiagnostic: (diagnostic: string) => Promise<void>;
  errorFallbackCode?: (error: unknown) => TaskErrorCode;
  inspectPatch: (
    patch: PatchSource,
    gameData: GameDataSource | null,
  ) => Promise<PatchAnalysis>;
  isDesktopRecoverableError?: (error: unknown) => boolean;
  loadEquipmentOptions: () => Promise<EquipmentOption[]>;
  previewMappings: (
    patch: PatchSource,
    mappings: MigrationMapping[],
  ) => Promise<EquipmentMappingPreview[]>;
  resourceLimits?: MappingLimits;
  restoreGameData?: () => Promise<SelectedGameData<GameDataSource> | null>;
  revealOutput?: (output: string) => Promise<void>;
  runMigration: (
    request: MigrationRunRequest<PatchSource, GameDataSource>,
  ) => Promise<CompletedRun<MigrationSummary> | null>;
  runRepatch: (
    request: RepatchRunRequest<PatchSource, GameDataSource>,
  ) => Promise<CompletedRun<UnitRepatchSummary> | null>;
}

export interface SetupPanelsProps<PatchSource, GameDataSource> {
  gameData: SelectedGameData<GameDataSource> | null;
  onGameDataChange: (gameData: SelectedGameData<GameDataSource> | null) => void;
  onPatchChange: (patch: SelectedPatch<PatchSource> | null) => void;
  patch: SelectedPatch<PatchSource> | null;
  reportError: (error: unknown) => void;
}

export interface MappingNoticeProps {
  limit: number;
  selectedTargetCount: number;
}

export interface GuidanceDialogProps {
  mappingLimit: { count: number; max: number } | null;
  onCloseMappingLimit: () => void;
}

export interface PlatformViewExtras {
  afterHeader?: ReactNode;
  enhanceErrorDescription?: (description: string) => ReactNode;
  footer?: ReactNode;
  guidanceDialogs?: (props: GuidanceDialogProps) => ReactNode;
  headerAction?: ReactNode;
  mappingNotice?: (props: MappingNoticeProps) => ReactNode;
  overlay?: (taskBusy: boolean) => ReactNode;
}

export interface MigratorAppProps<PatchSource, GameDataSource> {
  HeadMergeTool: ComponentType;
  MergeTool: ComponentType;
  SetupPanels: ComponentType<SetupPanelsProps<PatchSource, GameDataSource>>;
  extras?: PlatformViewExtras;
  layout: "web" | "desktop";
  onToolModeChange: (mode: ToolMode) => void;
  platform: MigratorPlatform<PatchSource, GameDataSource>;
  toolMode: ToolMode;
}

export function blockerHint(
  ready: { gameData: boolean; patch: boolean; targetCount: number; limitExceeded: boolean; limit: number },
  mode: ToolMode,
  t: Translate,
): string {
  if (!ready.gameData) return t("app.blockerGameData");
  if (!ready.patch) return t("app.blockerPatch");
  if (mode === "migrate" && !ready.targetCount) return t("app.blockerTarget");
  if (mode === "migrate" && ready.limitExceeded) {
    return t("app.blockerWebMappingLimit", { count: ready.targetCount, max: ready.limit });
  }
  return "";
}
