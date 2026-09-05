import CancelIcon from "@mui/icons-material/Cancel";
import PlayArrowIcon from "@mui/icons-material/PlayArrow";
import { Button, CircularProgress } from "@mui/material";
import { useCallback, useEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { toast, Toaster } from "sonner";
import {
  MappingPreviewAccordion,
  OptionsPanel,
  ResultReportDialog,
  TargetPanel,
  UnitUpdaterPanel,
  UpdateInfoDialog,
  backgroundUrl,
  exceedsWebMappingLimit,
  presentTaskError,
  useI18n,
  useTaskReportHistory,
  useUpdateInfo,
  type EquipmentOption,
  type MigrationMapping,
} from "@hd2-mod-tools/migrator-ui";
import { AppChrome } from "./AppChrome";
import {
  blockerHint,
  type MappingNoticeProps,
  type MigratorAppProps,
  type MigratorPlatform,
  type SelectedGameData,
  type SelectedPatch,
} from "./platform";
import { useMigrationWorkspace } from "./useMigrationWorkspace";

export function MigratorApp<PatchSource, GameDataSource>(
  props: MigratorAppProps<PatchSource, GameDataSource>,
) {
  const { extras, layout, platform } = props;
  const { t } = useI18n();
  const [equipmentOptions, setEquipmentOptions] = useState<Awaited<ReturnType<typeof platform.loadEquipmentOptions>>>([]);
  const [patch, setPatch] = useState<SelectedPatch<PatchSource> | null>(null);
  const [gameData, setGameData] = useState<SelectedGameData<GameDataSource> | null>(null);
  const [busy, setBusy] = useState(false);
  const [cancelling, setCancelling] = useState(false);
  const [progressLabel, setProgressLabel] = useState("");
  const [multiConfirmed, setMultiConfirmed] = useState(false);
  const [multiWarningOpen, setMultiWarningOpen] = useState(false);
  const [repatchWarningOpen, setRepatchWarningOpen] = useState(false);
  const [mappingLimitWarning, setMappingLimitWarning] = useState<{ count: number; max: number } | null>(null);
  const activeTask = useRef<AbortController | null>(null);
  const workspace = useMigrationWorkspace(equipmentOptions);
  const reports = useTaskReportHistory();
  const updateInfo = useUpdateInfo();
  const limits = mappingLimits(props, patch);
  const selectedTargetCount = workspace.configuredMappings.length;
  const activeLimit = workspace.outputAsSinglePatch ? limits.single : limits.separate;
  const limitExceeded = Boolean(
    props.platform.capabilities.mappingLimits
      && exceedsWebMappingLimit(workspace.configuredMappings, activeLimit),
  );
  const canMigrate = Boolean(gameData?.ready && patch && selectedTargetCount && !limitExceeded);
  const canRepatch = Boolean(gameData?.ready && patch);
  const canRun = props.toolMode === "migrate" ? canMigrate : canRepatch;
  const hint = blockerHint({
    gameData: Boolean(gameData?.ready),
    patch: Boolean(patch),
    targetCount: props.toolMode === "migrate" ? selectedTargetCount : 1,
    limitExceeded,
    limit: activeLimit,
  }, props.toolMode, t);

  const reportError = useCallback((error: unknown) => {
    console.error(`[hd2-migrator:${layout}] task failed:`, error);
    const fallback = platform.errorFallbackCode?.(error) ?? "unknown";
    const presentation = presentTaskError(error, t, fallback);
    if (presentation.error.code === "task.cancelled") {
      toast.info(t("task.cancelled"));
      return;
    }
    const canRecommendDesktop = platform.isDesktopRecoverableError?.(error)
      || presentation.error.code === "wasm.runtime";
    const description = canRecommendDesktop
      ? extras?.enhanceErrorDescription?.(presentation.description) ?? presentation.description
      : presentation.description;
    toast.error(presentation.title, {
      description,
      action: {
        label: t("error.copyDiagnostics"),
        onClick: () => copyDiagnostic(platform, presentation.diagnostic, t),
      },
    });
  }, [extras?.enhanceErrorDescription, layout, platform, t]);

  useEffect(() => {
    let disposed = false;
    void platform.loadEquipmentOptions().then((options) => {
      if (!disposed) setEquipmentOptions(options);
    }).catch(reportError);
    return () => { disposed = true; };
  }, [platform, reportError]);

  useEffect(() => {
    if (!platform.restoreGameData) return;
    let disposed = false;
    void platform.restoreGameData().then((selection) => {
      if (!disposed && selection) setGameData(selection);
    }).catch(reportError);
    return () => { disposed = true; };
  }, [platform, reportError]);

  useEffect(() => {
    if (!patch) {
      workspace.clearPatchAnalysis();
      return;
    }
    let disposed = false;
    setBusy(true);
    void platform.inspectPatch(patch.source, gameData?.source ?? null)
      .then((analysis) => { if (!disposed) workspace.applyAnalysis(analysis); })
      .catch((error) => { if (!disposed) reportError(error); })
      .finally(() => { if (!disposed) setBusy(false); });
    return () => { disposed = true; };
  }, [gameData?.key, patch?.key, platform]);

  const runCancellable = useCallback(async (task: (signal: AbortSignal) => Promise<void>) => {
    const controller = new AbortController();
    activeTask.current = controller;
    setBusy(true);
    try {
      await task(controller.signal);
    } catch (error) {
      reportError(error);
    } finally {
      if (activeTask.current === controller) activeTask.current = null;
      setBusy(false);
      setCancelling(false);
      setProgressLabel("");
    }
  }, [reportError]);

  const runMigration = useCallback(async () => {
    if (!patch || !gameData?.ready) return;
    if (limitExceeded) {
      setMappingLimitWarning({ count: selectedTargetCount, max: activeLimit });
      return;
    }
    await runCancellable(async (signal) => {
      const result = await platform.runMigration({
        equipmentOptions,
        gameData: gameData.source,
        noPadding: workspace.noPadding,
        onProgressLabel: setProgressLabel,
        patch,
        signal,
        unitBehavior: workspace.unitBehavior,
        unmatchedUnitPolicy: workspace.unmatchedUnitPolicy,
        variants: workspace.variants,
      });
      if (result) reports.recordReport({ kind: "migration", ...result });
    });
  }, [activeLimit, equipmentOptions, gameData, limitExceeded, patch, platform, reports.recordReport, runCancellable, selectedTargetCount, workspace]);

  const runRepatch = useCallback(async () => {
    if (!patch || !gameData?.ready) return;
    await runCancellable(async (signal) => {
      const result = await platform.runRepatch({
        gameData: gameData.source,
        missingUnitPolicy: workspace.missingUnitPolicy,
        onProgressLabel: setProgressLabel,
        patch,
        signal,
      });
      if (result) reports.recordReport({ kind: "repatch", ...result });
    });
  }, [gameData, patch, platform, reports.recordReport, runCancellable, workspace.missingUnitPolicy]);

  const requestRun = useCallback(() => {
    if (props.toolMode === "migrate") {
      void runMigration();
      return;
    }
    if (platform.capabilities.confirmRepatch) setRepatchWarningOpen(true);
    else void runRepatch();
  }, [platform.capabilities.confirmRepatch, props.toolMode, runMigration, runRepatch]);

  const toggleMultiTarget = useCallback((enabled: boolean) => {
    if (!workspace.multiTargetEligible) return;
    if (!enabled) {
      workspace.disableMultiTarget();
      return;
    }
    workspace.setMultiTarget(true);
    if (platform.capabilities.confirmMultiTarget && !multiConfirmed) {
      setMultiWarningOpen(true);
    }
  }, [multiConfirmed, platform.capabilities.confirmMultiTarget, workspace]);

  const replaceTargets = useCallback((targets: string[]) => {
    const nextCount = configuredMappingCount(workspace, targets);
    const reducesCount = nextCount < selectedTargetCount;
    if (!reducesCount && platform.capabilities.mappingLimits && nextCount > activeLimit) {
      setMappingLimitWarning({ count: nextCount, max: activeLimit });
      return;
    }
    workspace.replaceActiveTargets(targets);
  }, [activeLimit, platform.capabilities.mappingLimits, selectedTargetCount, workspace]);

  const chooseTarget = useCallback((hash: string) => {
    const current = workspace.activeTargets;
    const next = workspace.multiTarget
      ? current.includes(hash) ? current.filter((target) => target !== hash) : [...current, hash]
      : [hash];
    replaceTargets(next);
  }, [replaceTargets, workspace.activeTargets, workspace.multiTarget]);

  const toggleSinglePatch = useCallback((enabled: boolean) => {
    const nextLimit = enabled ? limits.single : limits.separate;
    if (selectedTargetCount > nextLimit) {
      setMappingLimitWarning({ count: selectedTargetCount, max: nextLimit });
      return;
    }
    workspace.setSinglePatch(enabled);
  }, [limits.separate, limits.single, selectedTargetCount, workspace.setSinglePatch]);

  const SetupPanels = props.SetupPanels;
  const mainContent = props.toolMode === "merge"
    ? <props.MergeTool />
    : props.toolMode === "head-merge"
      ? <props.HeadMergeTool />
      : (
        <>
          <SetupPanels
            gameData={gameData}
            onGameDataChange={setGameData}
            onPatchChange={setPatch}
            patch={patch}
            reportError={reportError}
          />
          <MigrationWorkspacePanels
            activeLimit={activeLimit}
            chooseTarget={chooseTarget}
            equipmentOptions={equipmentOptions}
            limits={limits}
            mappingNotice={props.extras?.mappingNotice}
            patch={patch}
            platform={props.platform}
            replaceTargets={replaceTargets}
            selectedTargetCount={selectedTargetCount}
            toggleMultiTarget={toggleMultiTarget}
            toggleSinglePatch={toggleSinglePatch}
            toolMode={props.toolMode}
            workspace={workspace}
          />
          <ActionRow
            blockerHint={hint}
            busy={busy}
            cancelling={cancelling}
            canRun={canRun}
            onCancel={() => {
              setCancelling(true);
              activeTask.current?.abort();
            }}
            onRun={requestRun}
            progressLabel={progressLabel}
            toolMode={props.toolMode}
            workspace={workspace}
          />
        </>
      );

  return (
    <div style={{ "--hd2-app-background": `url(${backgroundUrl})` } as CSSProperties}>
      <Toaster position="top-center" theme="dark" />
      <ResultReportDialog
        equipmentOptions={equipmentOptions}
        onClose={reports.closeReport}
        onRevealOutput={platform.revealOutput ? (output) => void platform.revealOutput?.(output).catch(reportError) : undefined}
        report={reports.activeReport}
      />
      <UpdateInfoDialog controller={updateInfo} />
      {extras?.overlay?.(busy)}
      <AppChrome
        afterHeader={props.extras?.afterHeader}
        footer={props.extras?.footer}
        headerAction={props.extras?.headerAction}
        layout={props.layout}
        onClearReports={reports.clearHistory}
        onOpenReport={reports.openReport}
        onOpenUpdateInfo={updateInfo.openLatest}
        onToolModeChange={props.onToolModeChange}
        reports={reports.history}
        toolMode={props.toolMode}
        updateInfoAvailable={updateInfo.available}
      >
        {mainContent}
      </AppChrome>
      {props.extras?.guidanceDialogs?.({
        mappingLimit: mappingLimitWarning,
        multiTargetOpen: multiWarningOpen,
        onCloseMappingLimit: () => setMappingLimitWarning(null),
        onConfirmMultiTarget: () => { setMultiConfirmed(true); setMultiWarningOpen(false); },
        onConfirmRepatch: () => { setRepatchWarningOpen(false); void runRepatch(); },
        onRejectMultiTarget: () => { setMultiWarningOpen(false); workspace.disableMultiTarget(); },
        onRejectRepatch: () => setRepatchWarningOpen(false),
        repatchOpen: repatchWarningOpen,
      })}
    </div>
  );
}

function mappingLimits<PatchSource, GameDataSource>(
  props: MigratorAppProps<PatchSource, GameDataSource>,
  patch: SelectedPatch<PatchSource> | null,
) {
  const configured = props.platform.capabilities.mappingLimits;
  return configured ? {
    separate: configured.separateOutputLimit(patch?.byteLength ?? 0),
    single: configured.singlePatchLimit,
  } : { separate: Number.MAX_SAFE_INTEGER, single: Number.MAX_SAFE_INTEGER };
}

function configuredMappingCount(
  workspace: ReturnType<typeof useMigrationWorkspace>,
  activeTargets: string[],
): number {
  if (!workspace.activeSource) return workspace.configuredMappings.length;
  const otherCount = Object.entries(workspace.targetsBySource)
    .filter(([sourceId]) => sourceId !== workspace.activeSource?.id)
    .reduce((count, [, targets]) => count + targets.length, 0);
  return otherCount + activeTargets.length;
}

interface WorkspacePanelsProps<PatchSource, GameDataSource> {
  activeLimit: number;
  chooseTarget: (hash: string) => void;
  equipmentOptions: EquipmentOption[];
  limits: { separate: number; single: number };
  mappingNotice?: (props: MappingNoticeProps) => ReactNode;
  patch: SelectedPatch<PatchSource> | null;
  platform: MigratorPlatform<PatchSource, GameDataSource>;
  replaceTargets: (targets: string[]) => void;
  selectedTargetCount: number;
  toggleMultiTarget: (enabled: boolean) => void;
  toggleSinglePatch: (enabled: boolean) => void;
  toolMode: "migrate" | "repatch";
  workspace: ReturnType<typeof useMigrationWorkspace>;
}

function MigrationWorkspacePanels<PatchSource, GameDataSource>(props: WorkspacePanelsProps<PatchSource, GameDataSource>) {
  if (props.toolMode === "repatch") return null;
  const workspace = props.workspace;
  const showLimits = Boolean(props.platform.capabilities.mappingLimits);
  const showNotice = showLimits && props.selectedTargetCount >= Math.ceil(props.activeLimit / 2)
    && props.selectedTargetCount <= props.activeLimit;
  return (
    <div className="border-t border-hd2-border">
      <TargetPanel
        activeSourceId={workspace.activeSourceId}
        equipmentOptions={props.equipmentOptions}
        multiTarget={workspace.multiTarget}
        multiTargetEligible={workspace.multiTargetEligible}
        onBatchSelect={props.platform.capabilities.batchTargetSelection ? props.replaceTargets : undefined}
        onMultiTargetChange={props.toggleMultiTarget}
        onResolveSource={workspace.resolveSource}
        onSinglePatchChange={props.toggleSinglePatch}
        onSourceChange={workspace.setActiveSourceId}
        onTargetChange={props.chooseTarget}
        selectedTargets={workspace.activeTargets}
        separateOutputMappingLimit={props.limits.separate}
        showOutputLimits={showLimits}
        singlePatch={workspace.outputAsSinglePatch}
        singlePatchMappingLimit={props.limits.single}
        singlePatchRequired={workspace.singlePatchRequired}
        sources={workspace.sources}
        targetOptions={workspace.targetOptions}
        targetSelectionEnabled={Boolean(workspace.activeSource?.resolvedHash)}
        targetsBySource={workspace.targetsBySource}
      />
      {showNotice && props.mappingNotice?.({ limit: props.activeLimit, selectedTargetCount: props.selectedTargetCount })}
      <MappingPreviewAccordion
        contextKey={props.patch?.key ?? "empty"}
        loadPreviews={(mappings: MigrationMapping[]) => {
          if (!props.patch) return Promise.reject(new Error("Patch is not loaded"));
          return props.platform.previewMappings(props.patch.source, mappings);
        }}
        mappings={workspace.configuredMappings}
        patchGraph={workspace.equipmentGraph}
        unitBehavior={workspace.unitBehavior}
        unmatchedUnitPolicy={workspace.unmatchedUnitPolicy}
        onUnitBehaviorChange={workspace.setUnitBehavior}
      />
    </div>
  );
}

interface ActionRowProps {
  blockerHint: string;
  busy: boolean;
  cancelling: boolean;
  canRun: boolean;
  onCancel: () => void;
  onRun: () => void;
  progressLabel: string;
  toolMode: "migrate" | "repatch";
  workspace: ReturnType<typeof useMigrationWorkspace>;
}

function ActionRow(props: ActionRowProps) {
  const { t } = useI18n();
  return (
    <div className="flex flex-col items-stretch gap-3 border-t border-hd2-border bg-hd2-pit px-5 py-3 min-[51.25rem]:flex-row min-[51.25rem]:items-center min-[51.25rem]:gap-4">
      <div className="flex flex-wrap items-center gap-x-4 gap-y-2 min-[51.25rem]:shrink-0">
        {props.toolMode === "migrate" ? (
          <OptionsPanel noPadding={props.workspace.noPadding} setNoPadding={props.workspace.setNoPadding} setUnmatchedUnitPolicy={props.workspace.setUnmatchedUnitPolicy} unmatchedUnitPolicy={props.workspace.unmatchedUnitPolicy} />
        ) : (
          <UnitUpdaterPanel missingUnitPolicy={props.workspace.missingUnitPolicy} onMissingUnitPolicyChange={props.workspace.setMissingUnitPolicy} />
        )}
      </div>
      <div className="flex min-w-0 flex-1 items-center gap-4">
        <div aria-atomic="true" aria-live="polite" className="flex min-w-0 flex-1 items-center justify-end gap-2">
          <span className="flex h-5 w-5 shrink-0 items-center justify-center">{props.busy && <CircularProgress size="1.25rem" />}</span>
          <span className="min-w-0 truncate text-xs text-hd2-muted" title={props.busy ? props.progressLabel : props.blockerHint}>{props.busy ? props.progressLabel : props.blockerHint}</span>
        </div>
        {props.busy ? (
          <Button disabled={props.cancelling} onClick={props.onCancel} startIcon={<CancelIcon />} variant="outlined">{props.cancelling ? t("task.cancelling") : t("task.cancel")}</Button>
        ) : (
          <Button disabled={!props.canRun} onClick={props.onRun} startIcon={<PlayArrowIcon />} variant="contained">{props.toolMode === "migrate" ? t("app.run") : t("repatch.run")}</Button>
        )}
      </div>
    </div>
  );
}

function copyDiagnostic<PatchSource, GameDataSource>(
  platform: MigratorPlatform<PatchSource, GameDataSource>,
  diagnostic: string,
  t: ReturnType<typeof useI18n>["t"],
) {
  void platform.copyDiagnostic(diagnostic)
    .then(() => toast.success(t("error.diagnosticsCopied")))
    .catch(() => toast.error(t("error.diagnosticsCopyFailed")));
}
