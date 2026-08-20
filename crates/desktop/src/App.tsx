import AccountTreeIcon from "@mui/icons-material/AccountTree";
import CancelIcon from "@mui/icons-material/Cancel";
import GitHubIcon from "@mui/icons-material/GitHub";
import PlayArrowIcon from "@mui/icons-material/PlayArrow";
import { Button, CircularProgress, IconButton, Tab, Tabs, Tooltip } from "@mui/material";
import { getCurrentWebview, type DragDropEvent } from "@tauri-apps/api/webview";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import {
  LanguageMenu,
  MappingPreviewAccordion,
  OptionsPanel,
  ResultReportDialog,
  TaskReportHistoryButton,
  TargetPanel,
  ToolIntro,
  UnitUpdaterPanel,
  UpdateInfoButton,
  UpdateInfoDialog,
  backgroundUrl,
  buildMigrationVariants,
  configuredMappings as collectConfiguredMappings,
  createMigrationProgressCounter,
  emptyUnitBehavior,
  multiTargetEligible as canUseMultiTarget,
  presentTaskError,
  selectTarget,
  singlePatchRequired as mustUseSinglePatch,
  targetsForSource,
  titleUrl,
  uniqueOutputFilename,
  useI18n,
  useTaskReportHistory,
  useUpdateInfo,
  type CompletedTaskReport,
  type CullingPolicy,
  type EquipmentPartGraph,
  type RepatchCullingSummary,
  type MigrationMapping,
  type Translate,
  type UnitBehaviorOptions,
} from "@hd2-mod-tools/migrator-ui";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { toast, Toaster } from "sonner";
import { AppUpdateButton, AppUpdateDialog } from "./AppUpdateDialog";
import { DesktopGameDataPanel, DesktopPatchPanel } from "./DesktopPanels";
import { dropZoneFromPhysicalPosition, type DropZone } from "./dropTarget";
import {
  chooseGameDataDir,
  chooseOutputZip,
  choosePatchPaths,
  detectGameDataDir,
  inspectPatch,
  loadEquipmentOptions,
  previewEquipmentMappings,
  startMigration,
  startRepatch,
  validateGameDataDir,
  type DesktopTask,
} from "./desktopClient";
import type {
  DetectedSource,
  EquipmentOption,
  MigrationProgressEvent,
  MigrationVariant,
  MissingUnitPolicy,
  PatchDescriptor,
  UnmatchedUnitPolicy,
} from "./types";
import { useAppUpdate, type AppUpdateController } from "./useAppUpdate";

const PATCH_SUFFIX = "9ba626afa44a3aa3.patch_0";
const GAME_DATA_STORAGE_KEY = "hd2-migrator-native-game-data";
type ToolMode = "migrate" | "repatch";

function App() {
  const { t } = useI18n();
  const [toolMode, setToolMode] = useState<ToolMode>("migrate");
  const [equipmentOptions, setEquipmentOptions] = useState<EquipmentOption[]>([]);
  const [patchPaths, setPatchPaths] = useState<string[]>([]);
  const [patch, setPatch] = useState<PatchDescriptor | null>(null);
  const [equipmentGraph, setEquipmentGraph] = useState<EquipmentPartGraph | null>(null);
  const [cullingSummary, setCullingSummary] = useState<RepatchCullingSummary | null>(null);
  const [gameDir, setGameDir] = useState<string | null>(null);
  const [sources, setSources] = useState<DetectedSource[]>([]);
  const [activeSourceId, setActiveSourceId] = useState("");
  const [targetsBySource, setTargetsBySource] = useState<Record<string, string[]>>({});
  const [multiTarget, setMultiTarget] = useState(false);
  const [singlePatch, setSinglePatch] = useState(false);
  const [noPadding, setNoPadding] = useState(false);
  const [cullingPolicy, setCullingPolicy] = useState<CullingPolicy>("patch");
  const [unmatchedUnitPolicy, setUnmatchedUnitPolicy] = useState<UnmatchedUnitPolicy>("keep");
  const [unitBehavior, setUnitBehavior] = useState<UnitBehaviorOptions>(emptyUnitBehavior);
  const [missingUnitPolicy, setMissingUnitPolicy] = useState<MissingUnitPolicy>("drop");
  const [busy, setBusy] = useState(false);
  const [cancelling, setCancelling] = useState(false);
  const [progressLabel, setProgressLabel] = useState("");
  const [hoveredDropZone, setHoveredDropZone] = useState<DropZone | null>(null);
  const reportHistory = useTaskReportHistory();
  const updateInfo = useUpdateInfo();
  const appUpdate = useAppUpdate();
  const hoveredDropZoneRef = useRef<DropZone | null>(null);
  const activeTaskRef = useRef<DesktopTask<unknown> | null>(null);

  useEffect(() => {
    void loadEquipmentOptions()
      .then(setEquipmentOptions)
      .catch((error) => showError(error, t));
  }, []);

  useEffect(() => {
    void restoreOrDetectGameDir(setGameDir);
  }, []);

  const activeSource = sources.find((source) => source.id === activeSourceId) ?? null;
  const activeTargets = activeSource ? targetsBySource[activeSource.id] ?? [] : [];
  const configuredMappings = useMemo(
    () => collectConfiguredMappings(sources, targetsBySource),
    [sources, targetsBySource],
  );
  const singlePatchRequired = mustUseSinglePatch(configuredMappings);
  const outputAsSinglePatch = singlePatch || singlePatchRequired;
  const multiTargetEligible = canUseMultiTarget(sources);
  const targetOptions = useMemo(
    () => targetsForSource(activeSource, equipmentOptions),
    [activeSource, equipmentOptions],
  );
  const canMigrate = Boolean(gameDir && patch && configuredMappings.length);
  const canRepatch = Boolean(gameDir && patch);
  const canRun = toolMode === "migrate" ? canMigrate : canRepatch;
  const blockerHint = canRun ? "" : nextBlockerHint(gameDir, patch, toolMode, configuredMappings.length, t);

  const applyInspection = useCallback((result: Awaited<ReturnType<typeof inspectPatch>>) => {
    setPatch(result.patch);
    setEquipmentGraph(result.equipmentGraph);
    setCullingSummary(result.cullingSummary);
    setSources(result.inspection.sources);
    setActiveSourceId(result.inspection.sources[0]?.id ?? "");
    setTargetsBySource({});
    setMultiTarget(canUseMultiTarget(result.inspection.sources));
    setSinglePatch(false);
    setUnitBehavior(emptyUnitBehavior());
  }, []);

  const loadMappingPreviews = useCallback(
    (mappings: MigrationMapping[]) => {
      if (cullingPolicy === "target" && !gameDir) {
        return Promise.reject(new Error("Game data is not loaded"));
      }
      const targetDataDir = cullingPolicy === "target" ? gameDir : null;
      return previewEquipmentMappings(patchPaths, mappings, targetDataDir);
    },
    [cullingPolicy, gameDir, patchPaths],
  );

  useEffect(() => {
    if (!patchPaths.length) return;
    let cancelled = false;
    setBusy(true);
    void inspectPatch(patchPaths, gameDir).then((result) => {
      if (!cancelled) applyInspection(result);
    }).catch((error) => {
      if (!cancelled) showError(error, t);
    }).finally(() => {
      if (!cancelled) setBusy(false);
    });
    return () => {
      cancelled = true;
    };
  }, [applyInspection, gameDir, patchPaths]);

  const importPatchPaths = useCallback(async (selected: string[]) => {
    setEquipmentGraph(null);
    setCullingSummary(null);
    setPatchPaths(selected);
  }, []);

  const selectPatch = useCallback(async () => {
    const selected = await choosePatchPaths();
    if (selected) await importPatchPaths(selected);
  }, [importPatchPaths]);

  const importGameDir = useCallback(async (selected: string) => {
    await runTask(setBusy, t, async () => {
      await validateGameDataDir(selected);
      applyGameDir(selected, setGameDir);
    });
  }, [t]);

  const selectGameDir = useCallback(async () => {
    const selected = await chooseGameDataDir();
    if (selected) await importGameDir(selected);
  }, [importGameDir]);

  useEffect(() => subscribeToDesktopDrop({
    hoveredDropZoneRef,
    importGameDir,
    importPatchPaths,
    setHoveredDropZone,
  }), [importGameDir, importPatchPaths]);

  const clearGameDir = useCallback(() => {
    localStorage.removeItem(GAME_DATA_STORAGE_KEY);
    setGameDir(null);
  }, []);

  const resolveSource = useCallback((sourceId: string, hash: string) => {
    const resolved = sources.map((source) => source.id === sourceId ? { ...source, resolvedHash: hash } : source);
    setSources(resolved);
    setTargetsBySource((current) => ({ ...current, [sourceId]: [] }));
    setMultiTarget(canUseMultiTarget(resolved));
  }, [sources]);

  const chooseTarget = useCallback((hash: string) => {
    if (!activeSource) return;
    const targets = selectTarget(activeTargets, hash, multiTarget);
    setTargetsBySource((current) => ({ ...current, [activeSource.id]: targets }));
  }, [activeSource, activeTargets, multiTarget]);

  const chooseTargetBatch = useCallback((hashes: string[]) => {
    if (!activeSource) return;
    setTargetsBySource((current) => ({ ...current, [activeSource.id]: hashes }));
  }, [activeSource]);

  const toggleMultiTarget = useCallback((enabled: boolean) => {
    if (!multiTargetEligible) return;
    setMultiTarget(enabled);
    if (!enabled) {
      setTargetsBySource((current) => Object.fromEntries(
        Object.entries(current).map(([sourceId, targets]) => [sourceId, targets.slice(0, 1)]),
      ));
    }
  }, [multiTargetEligible]);

  const runMigration = useCallback(async () => {
    if (!gameDir || !patch) return;
    const variants = buildMigrationVariants(configuredMappings, outputAsSinglePatch);
    const outputPath = await chooseOutputZip(outputFilename(patch, variants, equipmentOptions));
    if (!outputPath) return;
    const onProgress = stableDesktopMigrationProgress(
      configuredMappings,
      equipmentOptions,
      setProgressLabel,
    );
    await runTask(setBusy, t, async () => {
      const task = startMigration({
        patchPaths,
        dataDir: gameDir,
        outputPath,
        options: {
          variants,
          patchSuffix: PATCH_SUFFIX,
          noPadding,
          unmatchedUnitPolicy,
          unitBehavior,
          cullingPolicy,
        },
      }, onProgress);
      activeTaskRef.current = task;
      const summary = await task.result;
      reportHistory.recordReport({ kind: "migration", output: outputPath, summary });
    });
    setProgressLabel("");
  }, [configuredMappings, cullingPolicy, equipmentOptions, gameDir, noPadding, outputAsSinglePatch, patch, patchPaths, reportHistory.recordReport, t, unmatchedUnitPolicy, unitBehavior]);

  const runRepatch = useCallback(async () => {
    if (!gameDir || !patch) return;
    const outputPath = await chooseOutputZip("hd2-repatched-mod.zip");
    if (!outputPath) return;
    setProgressLabel(`1/1 ${t("repatch.progress")}`);
    await runTask(setBusy, t, async () => {
      const task = startRepatch({
        patchPaths,
        dataDir: gameDir,
        outputPath,
        options: { missingUnitPolicy, cullingPolicy },
      }, () => {});
      activeTaskRef.current = task;
      const summary = await task.result;
      reportHistory.recordReport({ kind: "repatch", output: outputPath, summary });
    });
    setProgressLabel("");
  }, [cullingPolicy, gameDir, missingUnitPolicy, patch, patchPaths, reportHistory.recordReport, t]);

  const cancelActiveTask = useCallback(async () => {
    const task = activeTaskRef.current;
    if (!task || cancelling) return;
    setCancelling(true);
    try {
      await task.cancel();
    } catch (error) {
      setCancelling(false);
      showError(error, t);
    }
  }, [cancelling, t]);

  useEffect(() => {
    if (busy) return;
    activeTaskRef.current = null;
    setCancelling(false);
  }, [busy]);

  return (
    <div className="min-h-screen bg-hd2-bg">
      <div className="fixed inset-0 z-0 bg-center bg-cover" style={{ backgroundImage: `url(${backgroundUrl})`, filter: "brightness(0.4)" }} />
      <Toaster position="top-center" theme="dark" />
      <ResultReportDialog
        equipmentOptions={equipmentOptions}
        onClose={reportHistory.closeReport}
        onRevealOutput={(output) => void revealItemInDir(output).catch((error) => showError(error, t))}
        report={reportHistory.activeReport}
      />
      <UpdateInfoDialog controller={updateInfo} />
      <AppUpdateDialog controller={appUpdate} taskBusy={busy} />
      <div className="relative z-[1] min-h-screen">
        <main className="min-h-screen w-full">
          <div className="min-h-screen overflow-hidden bg-black/60" data-desktop-shell>
            <Header
              appUpdate={appUpdate}
              onClearReports={reportHistory.clearHistory}
              onOpenReport={reportHistory.openReport}
              onOpenUpdateInfo={updateInfo.openLatest}
              reports={reportHistory.history}
              updateInfoAvailable={updateInfo.available}
            />
            <Tabs centered onChange={(_, value: ToolMode) => setToolMode(value)} value={toolMode}>
              <Tab label={t("mode.migrate")} value="migrate" />
              <Tab label={t("mode.repatch")} value="repatch" />
            </Tabs>
            <ToolIntro mode={toolMode} />
            <div className="flex flex-col min-[51.25rem]:flex-row">
              <div className="flex min-w-0 flex-1">
                <DesktopGameDataPanel
                  dataDir={gameDir}
                  dragging={hoveredDropZone === "gameData"}
                  onChange={() => void selectGameDir()}
                  onClear={clearGameDir}
                />
              </div>
              <div className="flex min-w-0 flex-1 border-t border-hd2-border min-[51.25rem]:border-t-0 min-[51.25rem]:border-l">
                <DesktopPatchPanel
                  dragging={hoveredDropZone === "patch"}
                  onChoose={() => void selectPatch()}
                  patch={patch}
                />
              </div>
            </div>
            <div className="border-t border-hd2-border">
              {toolMode === "migrate" ? <>
                <TargetPanel
                  activeSourceId={activeSourceId}
                  equipmentOptions={equipmentOptions}
                  multiTarget={multiTarget}
                  multiTargetEligible={multiTargetEligible}
                  onBatchSelect={chooseTargetBatch}
                  onMultiTargetChange={toggleMultiTarget}
                  onResolveSource={resolveSource}
                  onSinglePatchChange={setSinglePatch}
                  onSourceChange={setActiveSourceId}
                  onTargetChange={chooseTarget}
                  selectedTargets={activeTargets}
                  separateOutputMappingLimit={0}
                  showOutputLimits={false}
                  singlePatch={outputAsSinglePatch}
                  singlePatchMappingLimit={0}
                  singlePatchRequired={singlePatchRequired}
                  sources={sources}
                  targetOptions={targetOptions}
                  targetSelectionEnabled={Boolean(activeSource?.resolvedHash)}
                  targetsBySource={targetsBySource}
                />
                <MappingPreviewAccordion
                  cullingPolicy={cullingPolicy}
                  contextKey={`${patchPaths.join("|")}:${cullingPolicy}`}
                  loadPreviews={loadMappingPreviews}
                  mappings={configuredMappings}
                  patchGraph={equipmentGraph}
                  unitBehavior={unitBehavior}
                  unmatchedUnitPolicy={unmatchedUnitPolicy}
                  onUnitBehaviorChange={setUnitBehavior}
                />
              </> : (
                <UnitUpdaterPanel
                  cullingPolicy={cullingPolicy}
                  cullingSummary={cullingSummary}
                  missingUnitPolicy={missingUnitPolicy}
                  onCullingPolicyChange={setCullingPolicy}
                  onMissingUnitPolicyChange={setMissingUnitPolicy}
                />
              )}
            </div>
            <ActionRow
              blockerHint={blockerHint}
              busy={busy}
              cancelling={cancelling}
              canRun={canRun}
              cullingPolicy={cullingPolicy}
              noPadding={noPadding}
              onRun={toolMode === "migrate" ? runMigration : runRepatch}
              onCancel={cancelActiveTask}
              progressLabel={progressLabel}
              setNoPadding={setNoPadding}
              setCullingPolicy={setCullingPolicy}
              setUnmatchedUnitPolicy={setUnmatchedUnitPolicy}
              toolMode={toolMode}
              unmatchedUnitPolicy={unmatchedUnitPolicy}
            />
          </div>
        </main>
      </div>
    </div>
  );
}

interface HeaderProps {
  appUpdate: AppUpdateController;
  onClearReports: () => void;
  onOpenReport: (id: string) => void;
  onOpenUpdateInfo: () => void;
  reports: CompletedTaskReport[];
  updateInfoAvailable: boolean;
}

function Header(props: HeaderProps) {
  const { t } = useI18n();
  return (
    <div className="flex flex-col items-center border-b border-hd2-border bg-hd2-surface/70 px-4 py-5">
      <div className="flex w-full items-center gap-3">
        <Tooltip title={t("github.revision", { hash: __GIT_HASH__ })}>
          <div className="flex w-28 shrink-0 items-center gap-1 font-mono text-[0.625rem] tracking-wide text-hd2-faint min-[35rem]:w-32 min-[35rem]:text-[0.6875rem]">
            <AccountTreeIcon sx={{ fontSize: "0.875rem" }} /><span>{__GIT_HASH__}</span>
          </div>
        </Tooltip>
        <div className="flex min-w-0 flex-1 items-center justify-center gap-3">
          <img alt="" className="hidden min-[40rem]:block" draggable={false} src={titleUrl} style={{ height: "2rem", transform: "scaleX(-1)" }} />
          <h1 className="m-0 text-center text-lg font-bold text-hd2-yellow min-[35rem]:text-xl min-[51.25rem]:text-2xl">{t("app.title")}</h1>
          <img alt="" className="hidden min-[40rem]:block" draggable={false} src={titleUrl} style={{ height: "2rem" }} />
        </div>
        <div className="flex w-28 shrink-0 items-center justify-end min-[35rem]:w-32">
          <Tooltip title={t("github.openRepository")}>
            <IconButton className="headerIconBtn" component="a" href="https://github.com/eigeen/hd2-mod-crates" rel="noreferrer" size="small" target="_blank">
              <GitHubIcon fontSize="small" />
            </IconButton>
          </Tooltip>
          <AppUpdateButton controller={props.appUpdate} />
          <UpdateInfoButton available={props.updateInfoAvailable} openLatest={props.onOpenUpdateInfo} />
          <TaskReportHistoryButton
            onClear={props.onClearReports}
            onSelect={props.onOpenReport}
            reports={props.reports}
          />
          <LanguageMenu />
        </div>
      </div>
    </div>
  );
}

interface ActionRowProps {
  blockerHint: string;
  busy: boolean;
  cancelling: boolean;
  canRun: boolean;
  cullingPolicy: CullingPolicy;
  noPadding: boolean;
  onRun: () => Promise<void>;
  onCancel: () => Promise<void>;
  progressLabel: string;
  setNoPadding: (value: boolean) => void;
  setCullingPolicy: (value: CullingPolicy) => void;
  setUnmatchedUnitPolicy: (value: UnmatchedUnitPolicy) => void;
  toolMode: ToolMode;
  unmatchedUnitPolicy: UnmatchedUnitPolicy;
}

function ActionRow(props: ActionRowProps) {
  const { t } = useI18n();
  return (
    <div className="flex flex-col items-stretch gap-3 border-t border-hd2-border bg-hd2-pit px-5 py-3 min-[51.25rem]:flex-row min-[51.25rem]:items-center min-[51.25rem]:gap-4">
      {props.toolMode === "migrate" && (
        <div className="flex flex-wrap items-center gap-x-4 gap-y-2 min-[51.25rem]:shrink-0">
          <OptionsPanel
            cullingPolicy={props.cullingPolicy}
            noPadding={props.noPadding}
            setCullingPolicy={props.setCullingPolicy}
            setNoPadding={props.setNoPadding}
            setUnmatchedUnitPolicy={props.setUnmatchedUnitPolicy}
            unmatchedUnitPolicy={props.unmatchedUnitPolicy}
          />
        </div>
      )}
      <div className="flex min-w-0 flex-1 items-center gap-4">
        <div className="flex min-w-0 flex-1 items-center justify-end gap-2" aria-live="polite">
          <span className="flex h-5 w-5 shrink-0 items-center justify-center">{props.busy && <CircularProgress size="1.25rem" />}</span>
          <span className="min-w-0 truncate text-xs text-hd2-muted" title={props.busy ? props.progressLabel : props.blockerHint}>
            {props.busy ? props.progressLabel : props.blockerHint}
          </span>
        </div>
        {props.busy ? (
          <Button disabled={props.cancelling} onClick={() => void props.onCancel()} startIcon={<CancelIcon />} variant="outlined">
            {props.cancelling ? t("task.cancelling") : t("task.cancel")}
          </Button>
        ) : (
          <Button disabled={!props.canRun} onClick={() => void props.onRun()} startIcon={<PlayArrowIcon />} variant="contained">
            {props.toolMode === "migrate" ? t("app.run") : t("repatch.run")}
          </Button>
        )}
      </div>
    </div>
  );
}

function applyGameDir(path: string, setGameDir: (path: string) => void) {
  localStorage.setItem(GAME_DATA_STORAGE_KEY, path);
  setGameDir(path);
}

interface DesktopDropSubscription {
  hoveredDropZoneRef: { current: DropZone | null };
  importGameDir: (path: string) => Promise<void>;
  importPatchPaths: (paths: string[]) => Promise<void>;
  setHoveredDropZone: (zone: DropZone | null) => void;
}

function subscribeToDesktopDrop(subscription: DesktopDropSubscription) {
  let disposed = false;
  let unsubscribe: (() => void) | undefined;
  void getCurrentWebview().onDragDropEvent(({ payload }) => {
    void handleDesktopDrop(payload, subscription);
  }).then((value) => {
    if (disposed) value();
    else unsubscribe = value;
  });
  return () => {
    disposed = true;
    unsubscribe?.();
  };
}

async function handleDesktopDrop(event: DragDropEvent, subscription: DesktopDropSubscription) {
  if (event.type === "leave") {
    updateHoveredDropZone(null, subscription);
    return;
  }
  const zone = dropZoneFromPhysicalPosition(event.position);
  updateHoveredDropZone(zone, subscription);
  if (event.type !== "drop") return;
  updateHoveredDropZone(null, subscription);
  if (!zone || event.paths.length === 0) return;
  if (zone === "gameData") await subscription.importGameDir(event.paths[0]);
  else await subscription.importPatchPaths(event.paths);
}

function updateHoveredDropZone(zone: DropZone | null, subscription: DesktopDropSubscription) {
  subscription.hoveredDropZoneRef.current = zone;
  subscription.setHoveredDropZone(zone);
}

async function restoreOrDetectGameDir(setGameDir: (path: string) => void) {
  const stored = localStorage.getItem(GAME_DATA_STORAGE_KEY);
  if (stored) {
    try {
      await validateGameDataDir(stored);
      setGameDir(stored);
      return;
    } catch {
      localStorage.removeItem(GAME_DATA_STORAGE_KEY);
    }
  }
  const discovery = await detectGameDataDir().catch(() => null);
  if (discovery?.dataDir) applyGameDir(discovery.dataDir, setGameDir);
}

function nextBlockerHint(
  gameDir: string | null,
  patch: PatchDescriptor | null,
  mode: ToolMode,
  mappingCount: number,
  t: Translate,
) {
  if (!gameDir) return t("app.blockerGameData");
  if (!patch) return t("app.blockerPatch");
  if (mode === "migrate" && !mappingCount) return t("app.blockerTarget");
  return "";
}

function outputFilename(patch: PatchDescriptor, variants: MigrationVariant[], options: EquipmentOption[]) {
  const mapping = variants.length === 1 && variants[0].mappings.length === 1
    ? variants[0].mappings[0]
    : null;
  if (!mapping) return "hd2-migrated-patch.zip";
  return uniqueOutputFilename(patch.originalName ?? patch.name, mapping.targetHash, options);
}

function stableDesktopMigrationProgress(
  mappings: MigrationVariant["mappings"],
  options: EquipmentOption[],
  setLabel: (label: string) => void,
) {
  const labels = mappings.map((mapping) => (
    options.find((candidate) => candidate.hash === mapping.targetHash)?.name ?? mapping.targetHash
  ));
  const counter = createMigrationProgressCounter(labels, setLabel);
  return (event: MigrationProgressEvent) => {
    if (event.kind === "targetFinish") counter.advance();
  };
}

async function runTask(
  setBusy: (value: boolean) => void,
  t: Translate,
  task: () => Promise<void>,
) {
  setBusy(true);
  try {
    await task();
  } catch (error) {
    showError(error, t);
  } finally {
    setBusy(false);
  }
}

function showError(error: unknown, t: Translate) {
  console.error("[hd2-migrator-native] task failed:", error);
  const presentation = presentTaskError(error, t);
  if (presentation.error.code === "task.cancelled") {
    toast.info(t("task.cancelled"));
    return;
  }
  toast.error(presentation.title, {
    description: presentation.description,
    action: {
      label: t("error.copyDiagnostics"),
      onClick: () => copyDiagnostic(presentation.diagnostic, t),
    },
  });
}

function copyDiagnostic(diagnostic: string, t: Translate): void {
  void writeText(diagnostic)
    .then(() => toast.success(t("error.diagnosticsCopied")))
    .catch(() => toast.error(t("error.diagnosticsCopyFailed")));
}

export default App;
