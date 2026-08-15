import AccountTreeIcon from "@mui/icons-material/AccountTree";
import GitHubIcon from "@mui/icons-material/GitHub";
import PlayArrowIcon from "@mui/icons-material/PlayArrow";
import { Button, CircularProgress, IconButton, Tab, Tabs, Tooltip } from "@mui/material";
import { getCurrentWebview, type DragDropEvent } from "@tauri-apps/api/webview";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { toast, Toaster } from "sonner";
import { LanguageMenu } from "../../web_ui/src/LanguageMenu";
import {
  buildMigrationVariants,
  configuredMappings as collectConfiguredMappings,
  multiTargetEligible as canUseMultiTarget,
  selectTarget,
  singlePatchRequired as mustUseSinglePatch,
  targetsForSource,
} from "../../web_ui/src/migrationMappings";
import { OptionsPanel, TargetPanel } from "../../web_ui/src/MigratorPanels";
import { ToolIntro } from "../../web_ui/src/ToolIntro";
import { UnitUpdaterPanel } from "../../web_ui/src/UnitUpdaterPanel";
import { useI18n, type Translate } from "../../web_ui/src/i18n";
import { uniqueOutputFilename } from "../../web_ui/src/migrationDownloads";
import { DesktopGameDataPanel, DesktopPatchPanel } from "./DesktopPanels";
import { dropZoneFromPhysicalPosition, type DropZone } from "./dropTarget";
import {
  chooseGameDataDir,
  chooseOutputZip,
  choosePatchPaths,
  detectGameDataDir,
  inspectPatch,
  loadEquipmentOptions,
  migrateEquipment,
  repatchMod,
  subscribeToMigrationProgress,
  validateGameDataDir,
} from "./desktopClient";
import type {
  DetectedSource,
  EquipmentOption,
  MigrationProgressEvent,
  MigrationSummary,
  MigrationVariant,
  MissingUnitPolicy,
  PatchDescriptor,
  UnmatchedUnitPolicy,
  UnitRepatchSummary,
} from "./types";

const PATCH_SUFFIX = "9ba626afa44a3aa3.patch_0";
const GAME_DATA_STORAGE_KEY = "hd2-migrator-native-game-data";
type ToolMode = "migrate" | "repatch";

function App() {
  const { t } = useI18n();
  const [toolMode, setToolMode] = useState<ToolMode>("migrate");
  const [equipmentOptions, setEquipmentOptions] = useState<EquipmentOption[]>([]);
  const [patchPaths, setPatchPaths] = useState<string[]>([]);
  const [patch, setPatch] = useState<PatchDescriptor | null>(null);
  const [gameDir, setGameDir] = useState<string | null>(null);
  const [sources, setSources] = useState<DetectedSource[]>([]);
  const [activeSourceId, setActiveSourceId] = useState("");
  const [targetsBySource, setTargetsBySource] = useState<Record<string, string[]>>({});
  const [multiTarget, setMultiTarget] = useState(false);
  const [singlePatch, setSinglePatch] = useState(false);
  const [noPadding, setNoPadding] = useState(false);
  const [unmatchedUnitPolicy, setUnmatchedUnitPolicy] = useState<UnmatchedUnitPolicy>("keep");
  const [missingUnitPolicy, setMissingUnitPolicy] = useState<MissingUnitPolicy>("drop");
  const [busy, setBusy] = useState(false);
  const [progressLabel, setProgressLabel] = useState("");
  const [hoveredDropZone, setHoveredDropZone] = useState<DropZone | null>(null);
  const hoveredDropZoneRef = useRef<DropZone | null>(null);

  useEffect(() => {
    void loadEquipmentOptions().then(setEquipmentOptions).catch(showError);
  }, []);

  useEffect(() => {
    void restoreOrDetectGameDir(setGameDir);
  }, []);

  useEffect(() => {
    let disposed = false;
    let unsubscribe: (() => void) | undefined;
    void subscribeToMigrationProgress((event) => {
      if (!disposed) setProgressLabel(progressText(event, t));
    }).then((value) => {
      if (disposed) value();
      else unsubscribe = value;
    });
    return () => {
      disposed = true;
      unsubscribe?.();
    };
  }, [t]);

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
    setSources(result.inspection.sources);
    setActiveSourceId(result.inspection.sources[0]?.id ?? "");
    setTargetsBySource({});
    setMultiTarget(canUseMultiTarget(result.inspection.sources));
    setSinglePatch(false);
  }, []);

  useEffect(() => {
    if (!patchPaths.length) return;
    let cancelled = false;
    setBusy(true);
    void inspectPatch(patchPaths, gameDir).then((result) => {
      if (!cancelled) applyInspection(result);
    }).catch((error) => {
      if (!cancelled) showError(error);
    }).finally(() => {
      if (!cancelled) setBusy(false);
    });
    return () => {
      cancelled = true;
    };
  }, [applyInspection, gameDir, patchPaths]);

  const importPatchPaths = useCallback(async (selected: string[]) => {
    setPatchPaths(selected);
  }, []);

  const selectPatch = useCallback(async () => {
    const selected = await choosePatchPaths();
    if (selected) await importPatchPaths(selected);
  }, [importPatchPaths]);

  const importGameDir = useCallback(async (selected: string) => {
    await runTask(setBusy, async () => {
      await validateGameDataDir(selected);
      applyGameDir(selected, setGameDir);
    });
  }, []);

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
    setProgressLabel("");
    await runTask(setBusy, async () => {
      const summary = await migrateEquipment({
        patchPaths,
        dataDir: gameDir,
        outputPath,
        options: { variants, patchSuffix: PATCH_SUFFIX, noPadding, unmatchedUnitPolicy },
      });
      showMigrationReport(summary, t);
    });
    setProgressLabel("");
  }, [configuredMappings, equipmentOptions, gameDir, noPadding, outputAsSinglePatch, patch, patchPaths, t, unmatchedUnitPolicy]);

  const runRepatch = useCallback(async () => {
    if (!gameDir || !patch) return;
    const outputPath = await chooseOutputZip("hd2-repatched-mod.zip");
    if (!outputPath) return;
    setProgressLabel(t("repatch.progress"));
    await runTask(setBusy, async () => {
      const summary = await repatchMod({
        patchPaths,
        dataDir: gameDir,
        outputPath,
        options: { missingUnitPolicy },
      });
      showUnitRepatchReport(summary, t);
    });
    setProgressLabel("");
  }, [gameDir, missingUnitPolicy, patch, patchPaths, t]);

  return (
    <div className="min-h-screen">
      <div className="fixed inset-0 z-0 bg-center bg-cover" style={{ backgroundImage: "url(/background.webp)", filter: "brightness(0.4)" }} />
      <Toaster position="top-center" theme="dark" />
      <div className="relative z-[1]">
        <main className="mx-auto w-full max-w-[56rem] px-4 py-6 min-[51.25rem]:px-6 min-[51.25rem]:py-10">
          <div className="overflow-hidden border-2 border-hd2-border bg-black/60">
            <Header />
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
              {toolMode === "migrate" ? (
                <TargetPanel
                  activeSourceId={activeSourceId}
                  equipmentOptions={equipmentOptions}
                  multiTarget={multiTarget}
                  multiTargetEligible={multiTargetEligible}
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
              ) : (
                <UnitUpdaterPanel missingUnitPolicy={missingUnitPolicy} onMissingUnitPolicyChange={setMissingUnitPolicy} />
              )}
            </div>
            <ActionRow
              blockerHint={blockerHint}
              busy={busy}
              canRun={canRun}
              noPadding={noPadding}
              onRun={toolMode === "migrate" ? runMigration : runRepatch}
              progressLabel={progressLabel}
              setNoPadding={setNoPadding}
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

function Header() {
  const { t } = useI18n();
  return (
    <div className="flex flex-col items-center border-b border-hd2-border bg-hd2-surface/70 px-4 py-5">
      <div className="flex w-full items-center gap-3">
        <Tooltip title={t("github.revision", { hash: __GIT_HASH__ })}>
          <div className="flex w-[4.75rem] shrink-0 items-center gap-1 font-mono text-[0.625rem] tracking-wide text-hd2-faint min-[35rem]:w-24 min-[35rem]:text-[0.6875rem]">
            <AccountTreeIcon sx={{ fontSize: "0.875rem" }} /><span>{__GIT_HASH__}</span>
          </div>
        </Tooltip>
        <div className="flex min-w-0 flex-1 items-center justify-center gap-3">
          <img alt="" className="hidden min-[40rem]:block" draggable={false} src="/title.svg" style={{ height: "2rem", transform: "scaleX(-1)" }} />
          <h1 className="m-0 text-center text-lg font-bold text-hd2-yellow min-[35rem]:text-xl min-[51.25rem]:text-2xl">{t("app.title")}</h1>
          <img alt="" className="hidden min-[40rem]:block" draggable={false} src="/title.svg" style={{ height: "2rem" }} />
        </div>
        <div className="flex w-[4.75rem] shrink-0 items-center justify-end min-[35rem]:w-24">
          <Tooltip title={t("github.openRepository")}>
            <IconButton className="headerIconBtn" component="a" href="https://github.com/eigeen/hd2-mod-crates" rel="noreferrer" size="small" target="_blank">
              <GitHubIcon fontSize="small" />
            </IconButton>
          </Tooltip>
          <LanguageMenu />
        </div>
      </div>
    </div>
  );
}

interface ActionRowProps {
  blockerHint: string;
  busy: boolean;
  canRun: boolean;
  noPadding: boolean;
  onRun: () => Promise<void>;
  progressLabel: string;
  setNoPadding: (value: boolean) => void;
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
            noPadding={props.noPadding}
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
        <Button disabled={!props.canRun || props.busy} onClick={() => void props.onRun()} startIcon={<PlayArrowIcon />} variant="contained">
          {props.toolMode === "migrate" ? t("app.run") : t("repatch.run")}
        </Button>
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

function progressText(event: MigrationProgressEvent, t: Translate) {
  if (event.kind === "stage") return t("app.progressStage", { name: event.targetName, stage: event.stage });
  if (event.kind === "targetFinish") return "";
  return t("app.progressMigrating", { name: event.targetName });
}

function showMigrationReport(summary: MigrationSummary, t: Translate) {
  const details = summary.reports.map((report) => reportLine(report, t)).join("\n");
  const options = { description: <span style={{ whiteSpace: "pre-line" }}>{details}</span>, duration: 8000 };
  const title = t("report.title", { count: summary.migratedCount });
  if (summary.warningCount) toast.warning(title, options);
  else toast.success(title, options);
}

function reportLine(report: MigrationSummary["reports"][number], t: Translate) {
  const parts = [report.targetName];
  if (report.fileIdRemapped) parts.push(t("report.remapped", { count: report.fileIdRemapped }));
  if (report.paddedUnits) parts.push(t("report.padded", { count: report.paddedUnits }));
  if (report.skippedEntries) parts.push(t("report.skipped", { count: report.skippedEntries }));
  if (report.warnings.length) parts.push(t("report.warnings", { count: report.warnings.length }));
  return parts.join(" · ");
}

function showUnitRepatchReport(summary: UnitRepatchSummary, t: Translate) {
  const description = t("repatch.reportDetails", {
    updated: summary.updatedUnits,
    current: summary.alreadyCurrentUnits,
    removed: summary.removedUnits,
    failed: summary.failedUnits,
    archives: summary.scannedArchives,
  });
  if (summary.warnings.length || summary.failedUnits) toast.warning(t("repatch.reportTitle"), { description });
  else toast.success(t("repatch.reportTitle"), { description });
}

async function runTask(setBusy: (value: boolean) => void, task: () => Promise<void>) {
  setBusy(true);
  try {
    await task();
  } catch (error) {
    showError(error);
  } finally {
    setBusy(false);
  }
}

function showError(error: unknown) {
  console.error("[hd2-migrator-native] task failed:", error);
  toast.error(errorMessage(error));
}

function errorMessage(error: unknown) {
  if (error && typeof error === "object" && "message" in error) return String(error.message);
  return String(error);
}

export default App;
