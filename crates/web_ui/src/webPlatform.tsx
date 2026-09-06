import { useCallback, useEffect, useRef } from "react";
import {
  MAX_WEB_SINGLE_PATCH_MAPPINGS,
  PatchPanel,
  copyTaskErrorDiagnostic,
  createMigrationProgressCounter,
  maxWebSeparateOutputsForPatch,
  throwIfTaskCancelled,
  useI18n,
  type MigrationSummary,
  type TaskErrorCode,
  type Translate,
} from "@hd2-mod-tools/migrator-ui";
import {
  DEFAULT_REPATCH_OUTPUT,
  type MigratorPlatform,
  type PlatformViewExtras,
  type SelectedGameData,
  type SelectedPatch,
  type SetupPanelsProps,
} from "@hd2-mod-tools/migrator-app";
import {
  DesktopDownloadEntry,
  DesktopErrorRecommendation,
  DesktopRecommendation,
  WebMappingLimitDialog,
} from "./DesktopGuidance";
import { isDesktopRecoverableWebError } from "./desktopGuidancePolicy";
import {
  downloadRepatchedPatch,
  downloadZip,
  loadPatchFiles,
  patchFileSourcesFromList,
  type PatchFileSources,
} from "./fileInputs";
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
  type MigrationTaskContext,
} from "./wasmClient";
export const webPlatform: MigratorPlatform<PatchFileSources, GameDirSelection> = {
  resourceLimits: {
    separateOutputLimit: maxWebSeparateOutputsForPatch,
    singlePatchLimit: MAX_WEB_SINGLE_PATCH_MAPPINGS,
  },
  copyDiagnostic: copyTaskErrorDiagnostic,
  errorFallbackCode: (error) => isWasmRuntimeTrapError(error) ? "wasm.runtime" : "unknown",
  inspectPatch: async (patch, gameData, context) => analyzeEquipmentContents(
    patch,
    gameData ? new GameDataSource(gameData.handle) : undefined,
    context,
  ),
  isDesktopRecoverableError: isDesktopRecoverableWebError,
  loadEquipmentOptions: builtinEquipmentOptions,
  previewMappings: previewEquipmentMappings,
  runMigration: runWebMigration,
  runRepatch: runWebRepatch,
};

export function WebSetupPanels(
  props: SetupPanelsProps<PatchFileSources, GameDirSelection> & {
    onDirectoryAccessAborted?: () => void;
  },
) {
  const { t } = useI18n();
  const activeImport = useRef<AbortController | null>(null);
  useEffect(() => () => activeImport.current?.abort(), []);
  const importPatchFiles = useCallback(async (
    files: FileList | File[] | null,
    originalName?: string,
  ) => {
    activeImport.current?.abort();
    if (!files) return;
    const controller = new AbortController();
    activeImport.current = controller;
    props.onPatchChange(null);
    try {
      const source = await patchFileSourcesFromList(
        files,
        patchFileMessages(t),
        (toc, streamLength) => patchSidecarRequirements(toc, streamLength, { signal: controller.signal }),
        { originalName, signal: controller.signal },
      );
      controller.signal.throwIfAborted();
      props.onPatchChange(selectedWebPatch(source));
    } catch (error) {
      if (controller.signal.aborted) return;
      props.reportError(error);
    } finally {
      if (activeImport.current === controller) activeImport.current = null;
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
    <WebMappingLimitDialog
      count={props.mappingLimit?.count ?? 0}
      max={props.mappingLimit?.max ?? 0}
      onClose={props.onCloseMappingLimit}
      open={props.mappingLimit !== null}
    />
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
  const patch = await loadPatchFiles(request.patch.source, request.signal);
  const result = await migrateEquipmentVariants(
    patch,
    request.options,
    new GameDataSource(request.gameData.handle),
    webMigrationProgress(request),
  );
  throwIfTaskCancelled(request.signal);
  const output = request.suggestedOutputFilename;
  downloadZip(result.zipBlob, output);
  return { output, summary: result.summary };
}

async function runWebRepatch(
  request: Parameters<typeof webPlatform.runRepatch>[0],
) {
  request.onStage("reading_patch");
  const patch = await loadPatchFiles(request.patch.source, request.signal);
  const output = await repatchUnits(
    patch,
    { missingUnitPolicy: request.missingUnitPolicy },
    new GameDataSource(request.gameData.handle),
    { onStage: (_name, stage) => request.onStage(stage), signal: request.signal },
  );
  throwIfTaskCancelled(request.signal);
  const outputName = DEFAULT_REPATCH_OUTPUT;
  request.onStage("saving_output");
  downloadRepatchedPatch(
    patch,
    output.tocBytes,
    outputName,
    output.gpuBytes,
    output.streamBytes,
  );
  return { output: outputName, summary: output.summary };
}

function webMigrationProgress(request: Parameters<typeof webPlatform.runMigration>[0]): MigrationTaskContext {
  const counter = createMigrationProgressCounter(request.progressLabels, request.onProgressLabel);
  return {
    signal: request.signal,
    onTargetStart: () => throwIfTaskCancelled(request.signal),
    onStage: () => throwIfTaskCancelled(request.signal),
    onTargetFinish: (targetName) => {
      throwIfTaskCancelled(request.signal);
      counter.finish(targetName);
    },
  };
}

function selectedWebPatch(source: PatchFileSources): SelectedPatch<PatchFileSources> {
  return {
    byteLength: source.toc.byteLength + (source.gpu?.size ?? 0) + (source.stream?.size ?? 0),
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
