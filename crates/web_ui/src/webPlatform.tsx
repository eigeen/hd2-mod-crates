import { useCallback } from "react";
import {
  MAX_WEB_SINGLE_PATCH_MAPPINGS,
  PatchPanel,
  copyTaskErrorDiagnostic,
  createMigrationProgressCounter,
  maxWebSeparateOutputsForPatch,
  migrationMappingLabel,
  throwIfTaskCancelled,
  uniqueOutputFilename,
  useI18n,
  type MigrationMapping,
  type MigrationSummary,
  type TaskErrorCode,
  type Translate,
  type UnifiedMigrateOptions,
} from "@hd2-mod-tools/migrator-ui";
import type {
  MigratorPlatform,
  PlatformViewExtras,
  SelectedGameData,
  SelectedPatch,
  SetupPanelsProps,
} from "@hd2-mod-tools/migrator-app";
import {
  DesktopDownloadEntry,
  DesktopErrorRecommendation,
  DesktopRecommendation,
  MultiTargetGuidanceDialog,
  RepatchGuidanceDialog,
  WebMappingLimitDialog,
} from "./DesktopGuidance";
import { isDesktopRecoverableWebError } from "./desktopGuidancePolicy";
import { downloadRepatchedPatch, downloadZip, patchFilesFromList } from "./fileInputs";
import { GameDataDirPanel, type GameDirSelection } from "./GameDataDirPanel";
import { GameDataSource } from "./gameDataSource";
import {
  analyzeEquipmentContents,
  builtinEquipmentOptions,
  isWasmRuntimeTrapError,
  migrateEquipmentVariants,
  patchSidecarRequirements,
  previewEquipmentMappings,
  repatchUnits,
  type MigrationProgressSink,
} from "./wasmClient";
import type { PatchFiles } from "@hd2-mod-tools/migrator-ui";

const PATCH_SUFFIX = "9ba626afa44a3aa3.patch_0";

export const webPlatform: MigratorPlatform<PatchFiles, GameDirSelection> = {
  capabilities: {
    batchTargetSelection: false,
    confirmMultiTarget: true,
    confirmRepatch: true,
    mappingLimits: {
      separateOutputLimit: maxWebSeparateOutputsForPatch,
      singlePatchLimit: MAX_WEB_SINGLE_PATCH_MAPPINGS,
    },
    recommendDesktop: true,
  },
  copyDiagnostic: copyTaskErrorDiagnostic,
  errorFallbackCode: (error) => isWasmRuntimeTrapError(error) ? "wasm.runtime" : "unknown",
  inspectPatch: async (patch, gameData) => analyzeEquipmentContents(
    patch,
    gameData ? new GameDataSource(gameData.handle) : undefined,
  ),
  isDesktopRecoverableError: isDesktopRecoverableWebError,
  loadEquipmentOptions: builtinEquipmentOptions,
  previewMappings: previewEquipmentMappings,
  runMigration: runWebMigration,
  runRepatch: runWebRepatch,
};

export function WebSetupPanels(
  props: SetupPanelsProps<PatchFiles, GameDirSelection> & { onDirectoryAccessAborted?: () => void },
) {
  const { t } = useI18n();
  const importPatchFiles = useCallback(async (
    files: FileList | File[] | null,
    originalName?: string,
  ) => {
    if (!files) return;
    try {
      const source = await patchFilesFromList(
        files,
        patchFileMessages(t),
        patchSidecarRequirements,
        originalName,
      );
      props.onPatchChange(selectedWebPatch(source));
    } catch (error) {
      props.reportError(error);
    }
  }, [props.onPatchChange, props.reportError, t]);

  return (
    <div className="flex flex-col min-[51.25rem]:flex-row">
      <div className="flex min-w-0 flex-1">
        <GameDataDirPanel
          onChange={(selection) => props.onGameDataChange(
            selection ? selectedWebGameData(selection) : null,
          )}
          onDirectoryAccessAborted={props.onDirectoryAccessAborted ?? (() => undefined)}
          selection={props.gameData?.source ?? null}
        />
      </div>
      <div className="flex min-w-0 flex-1 border-t border-hd2-border min-[51.25rem]:border-t-0 min-[51.25rem]:border-l">
        <PatchPanel onPatchFiles={importPatchFiles} patch={props.patch?.info ?? null} />
      </div>
    </div>
  );
}

export const webViewExtras: Omit<PlatformViewExtras, "footer"> = {
  afterHeader: <DesktopDownloadEntry />,
  enhanceErrorDescription: (description) => (
    <DesktopErrorRecommendation>{description}</DesktopErrorRecommendation>
  ),
  guidanceDialogs: (props) => (
    <>
      <MultiTargetGuidanceDialog
        onCancel={props.onRejectMultiTarget}
        onContinue={props.onConfirmMultiTarget}
        open={props.multiTargetOpen}
      />
      <RepatchGuidanceDialog
        onCancel={props.onRejectRepatch}
        onContinue={props.onConfirmRepatch}
        open={props.repatchOpen}
      />
      <WebMappingLimitDialog
        count={props.mappingLimit?.count ?? 0}
        max={props.mappingLimit?.max ?? 0}
        onClose={props.onCloseMappingLimit}
        open={props.mappingLimit !== null}
      />
    </>
  ),
  mappingNotice: ({ limit, selectedTargetCount }) => (
    <DesktopRecommendation
      body="desktopGuide.largeMigrationBody"
      title="desktopGuide.largeMigrationTitle"
      values={{ count: selectedTargetCount, max: limit }}
    />
  ),
};

async function runWebMigration(
  request: Parameters<typeof webPlatform.runMigration>[0],
): Promise<{ output: string; summary: MigrationSummary }> {
  const options: UnifiedMigrateOptions = {
    variants: request.variants,
    patchSuffix: PATCH_SUFFIX,
    noPadding: request.noPadding,
    unmatchedUnitPolicy: request.unmatchedUnitPolicy,
    unitBehavior: request.unitBehavior,
  };
  const mappings = request.variants.flatMap((variant) => variant.mappings);
  const result = await migrateEquipmentVariants(
    request.patch.source,
    options,
    new GameDataSource(request.gameData.handle),
    webMigrationProgress(mappings, request),
  );
  throwIfTaskCancelled(request.signal);
  const output = migrationOutputFilename(request.patch, request.variants, request.equipmentOptions);
  downloadZip(result.zipBlob, output);
  return { output, summary: result.summary };
}

async function runWebRepatch(
  request: Parameters<typeof webPlatform.runRepatch>[0],
) {
  request.onProgressLabel("1/1");
  const output = await repatchUnits(
    request.patch.source,
    { missingUnitPolicy: request.missingUnitPolicy },
    new GameDataSource(request.gameData.handle),
    { onStage: () => throwIfTaskCancelled(request.signal) },
  );
  throwIfTaskCancelled(request.signal);
  const outputName = "hd2-repatched-mod.zip";
  downloadRepatchedPatch(
    request.patch.source,
    output.tocBytes,
    outputName,
    output.gpuBytes,
    output.streamBytes,
  );
  return { output: outputName, summary: output.summary };
}

function webMigrationProgress(
  mappings: MigrationMapping[],
  request: Parameters<typeof webPlatform.runMigration>[0],
): MigrationProgressSink {
  const labels = mappings.map((mapping) => migrationMappingLabel(mapping, request.equipmentOptions));
  const counter = createMigrationProgressCounter(labels, request.onProgressLabel);
  return {
    onTargetStart: () => throwIfTaskCancelled(request.signal),
    onStage: () => throwIfTaskCancelled(request.signal),
    onTargetFinish: () => {
      throwIfTaskCancelled(request.signal);
      counter.advance();
    },
  };
}

function migrationOutputFilename(
  patch: SelectedPatch<PatchFiles>,
  variants: Parameters<typeof webPlatform.runMigration>[0]["variants"],
  equipmentOptions: Parameters<typeof webPlatform.runMigration>[0]["equipmentOptions"],
) {
  const mapping = variants.length === 1 && variants[0].mappings.length === 1
    ? variants[0].mappings[0]
    : null;
  if (!mapping) return "hd2-migrated-patch.zip";
  return uniqueOutputFilename(
    patch.originalName ?? patch.info.name,
    mapping.targetHash,
    equipmentOptions,
  );
}

function selectedWebPatch(source: PatchFiles): SelectedPatch<PatchFiles> {
  return {
    byteLength: source.toc.byteLength + source.gpu.byteLength + source.stream.byteLength,
    info: { name: source.name },
    key: crypto.randomUUID(),
    originalName: source.originalName,
    source,
  };
}

function selectedWebGameData(
  source: GameDirSelection,
): SelectedGameData<GameDirSelection> {
  return {
    key: crypto.randomUUID(),
    label: source.handle.name,
    ready: source.status.kind !== "empty",
    source,
  };
}

function patchFileMessages(t: Translate) {
  return {
    noToc: t("patch.noToc"),
    multipleToc: t("patch.multipleToc"),
    missingIntro: t("patch.missingIntro"),
    missingAction: (toc: string, gpu: string, stream: string) =>
      t("patch.missingAction", { toc, gpu, stream }),
    missingSidecar: (filename: string, expected: string) =>
      t("patch.missingSidecar", { filename, expected }),
    shortSidecar: (filename: string, expected: string, actual: number) =>
      t("patch.shortSidecar", { filename, expected, actual }),
  };
}
