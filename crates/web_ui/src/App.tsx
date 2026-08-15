import AccountTreeIcon from "@mui/icons-material/AccountTree";
import CancelIcon from "@mui/icons-material/Cancel";
import PlayArrowIcon from "@mui/icons-material/PlayArrow";
import GitHubIcon from "@mui/icons-material/GitHub";
import { Button, CircularProgress, IconButton, Tab, Tabs, Tooltip } from "@mui/material";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { toast, Toaster } from "sonner";
import { downloadRepatchedPatch, downloadZip, patchFilesFromList } from "./fileInputs";
import { FrequentlyAskedQuestions } from "./FrequentlyAskedQuestions";
import { GameDataDirPanel, type GameDirSelection } from "./GameDataDirPanel";
import { GameDataSource } from "./gameDataSource";
import { useI18n, type Translate } from "./i18n";
import type { TranslationKey } from "./locales/translationKeys";
import { LanguageMenu } from "./LanguageMenu";
import {
  uniqueOutputFilename,
} from "./migrationDownloads";
import {
  buildMigrationVariants,
  configuredMappings as collectConfiguredMappings,
  exceedsWebMappingLimit,
  MAX_WEB_SINGLE_PATCH_MAPPINGS,
  maxWebSeparateOutputsForPatch,
  multiTargetEligible as canUseMultiTarget,
  selectTarget,
  singlePatchRequired as mustUseSinglePatch,
  targetsForSource,
} from "./migrationMappings";
import {
  OptionsPanel,
  PatchPanel,
  PerformanceDialog,
  TargetPanel,
} from "./MigratorPanels";
import { UnitUpdaterPanel } from "./UnitUpdaterPanel";
import { ToolIntro } from "./ToolIntro";
import { ResultReportDialog, type CompletedTaskReport } from "./ResultReportDialog";
import { normalizeTaskError, throwIfTaskCancelled } from "./taskError";
import type {
  DetectedSource,
  EquipmentOption,
  MissingUnitPolicy,
  MigrationMapping,
  MigrationSummary,
  MigrationVariant,
  PatchFiles,
  PatchInfo,
  UnmatchedUnitPolicy,
  UnitRepatchSummary,
} from "./types";
import {
  builtinEquipmentOptions,
  inspectEquipmentContents,
  isWasmRuntimeTrapError,
  migrateEquipmentVariants,
  repatchUnits,
  type MigrationProgressSink,
} from "./wasmClient";

const PATCH_SUFFIX = "9ba626afa44a3aa3.patch_0";
type ToolMode = "migrate" | "repatch";

function App() {
  const { t } = useI18n();
  const [toolMode, setToolMode] = useState<ToolMode>("migrate");
  const [equipmentOptions, setEquipmentOptions] = useState<EquipmentOption[]>([]);
  // patch 的 Uint8Array 可能上百 MB，放在 React state 里会被 React DevTools 扩展枚举/序列化导致 CPU 拉满 + 堆 OOM。
  // 因此把字节存到 ref，state 只留小元数据用于驱动 UI。
  const patchRef = useRef<PatchFiles | null>(null);
  const [patchInfo, setPatchInfo] = useState<PatchInfo | null>(null);
  const [sources, setSources] = useState<DetectedSource[]>([]);
  const [activeSourceId, setActiveSourceId] = useState("");
  const [targetsBySource, setTargetsBySource] = useState<Record<string, string[]>>({});
  const [multiTarget, setMultiTarget] = useState(false);
  const [singlePatch, setSinglePatch] = useState(false);
  const [noPadding, setNoPadding] = useState(false);
  const [unmatchedUnitPolicy, setUnmatchedUnitPolicy] = useState<UnmatchedUnitPolicy>("keep");
  const [busy, setBusy] = useState(false);
  const [cancelling, setCancelling] = useState(false);
  const [progressLabel, setProgressLabel] = useState("");
  const [warningOpen, setWarningOpen] = useState(false);
  const [multiConfirmed, setMultiConfirmed] = useState(false);
  const [gameDir, setGameDir] = useState<GameDirSelection | null>(null);
  const [missingUnitPolicy, setMissingUnitPolicy] = useState<MissingUnitPolicy>("drop");
  const [faqOpen, setFaqOpen] = useState(false);
  const [faqAttention, setFaqAttention] = useState<TranslationKey | null>(null);
  const [completedReport, setCompletedReport] = useState<CompletedTaskReport | null>(null);
  const activeTaskRef = useRef<AbortController | null>(null);

  const runCancellableTask = useCallback(async (
    task: (signal: AbortSignal) => Promise<void>,
  ) => {
    const controller = new AbortController();
    activeTaskRef.current = controller;
    setBusy(true);
    try {
      await task(controller.signal);
    } catch (error) {
      showTaskError(error, t);
    } finally {
      if (activeTaskRef.current === controller) activeTaskRef.current = null;
      setCancelling(false);
      setBusy(false);
    }
  }, [t]);

  const cancelActiveTask = useCallback(() => {
    if (!activeTaskRef.current || cancelling) return;
    setCancelling(true);
    setProgressLabel(t("task.cancelling"));
    activeTaskRef.current.abort();
  }, [cancelling, t]);

  useEffect(() => {
    let cancelled = false;
    builtinEquipmentOptions()
      .then((options) => {
        if (!cancelled) setEquipmentOptions(options);
      })
      .catch((error) => console.error("[hd2-migrator] load builtin targets failed:", error));
    return () => {
      cancelled = true;
    };
  }, []);

  const activeSource = sources.find((source) => source.id === activeSourceId) ?? null;
  const activeTargets = activeSource ? targetsBySource[activeSource.id] ?? [] : [];
  const configuredMappings = useMemo(
    () => collectConfiguredMappings(sources, targetsBySource),
    [sources, targetsBySource],
  );
  const selectedTargetCount = configuredMappings.length;
  const singlePatchRequired = mustUseSinglePatch(configuredMappings);
  const outputAsSinglePatch = singlePatch || singlePatchRequired;
  const singlePatchMappingLimit = MAX_WEB_SINGLE_PATCH_MAPPINGS;
  const separateOutputMappingLimit = maxWebSeparateOutputsForPatch(
    patchRef.current ? patchByteLength(patchRef.current) : 0,
  );
  const webMappingLimit = outputAsSinglePatch
    ? singlePatchMappingLimit
    : separateOutputMappingLimit;
  const webMappingLimitExceeded = exceedsWebMappingLimit(
    configuredMappings,
    webMappingLimit,
  );
  const multiTargetEligible = canUseMultiTarget(sources);
  const crossArchiveReady = gameDir !== null && gameDir.status.kind !== "empty";
  const canMigrate = Boolean(
    equipmentOptions.length
      && patchInfo
      && selectedTargetCount
      && crossArchiveReady
      && !webMappingLimitExceeded,
  );
  const canRepatch = Boolean(patchInfo && crossArchiveReady);
  const canRun = toolMode === "migrate" ? canMigrate : canRepatch;
  const blockerHint = canRun ? "" : nextBlockerHint({
    crossArchiveReady,
    patchInfo,
    selectedTargetCount: toolMode === "migrate" ? selectedTargetCount : 1,
    webMappingLimit,
    webMappingLimitExceeded: toolMode === "migrate" && webMappingLimitExceeded,
  }, t);
  const patchMessages = useMemo(() => patchFileMessages(t), [t]);

  const targetOptions = useMemo(
    () => targetsForSource(activeSource, equipmentOptions),
    [activeSource, equipmentOptions],
  );

  const applyPatch = useCallback(async (nextPatch: PatchFiles) => {
    patchRef.current = nextPatch;
    setPatchInfo({ name: nextPatch.name });
    setTargetsBySource({});
    setMultiTarget(false);
    setSinglePatch(false);
    const dataSource = gameDir ? new GameDataSource(gameDir.handle) : undefined;
    const inspection = await inspectEquipmentContents(nextPatch, dataSource);
    setSources(inspection.sources);
    setActiveSourceId(inspection.sources[0]?.id ?? "");
    setMultiTarget(canUseMultiTarget(inspection.sources));
  }, [gameDir]);

  useEffect(() => {
    const patch = patchRef.current;
    if (!patch || !gameDir) return;
    void runTask(setBusy, async () => {
      const inspection = await inspectEquipmentContents(
        patch,
        new GameDataSource(gameDir.handle),
      );
      setSources(inspection.sources);
      setActiveSourceId(inspection.sources[0]?.id ?? "");
      setTargetsBySource({});
      setMultiTarget(canUseMultiTarget(inspection.sources));
      setSinglePatch(false);
    }, t);
  }, [gameDir, t]);

  const importPatchFiles = useCallback(
    async (files: FileList | File[] | null, originalName?: string) => {
      if (!files) return;
      await runTask(setBusy, async () => {
        const nextPatch = await patchFilesFromList(files, patchMessages, originalName);
        await applyPatch(nextPatch);
      }, t);
    },
    [applyPatch, patchMessages, t],
  );

  const toggleMultiTarget = useCallback((enabled: boolean) => {
    if (!multiTargetEligible) return;
    setMultiTarget(enabled);
    if (!enabled) {
      setTargetsBySource((current) => Object.fromEntries(
        Object.entries(current).map(([sourceId, targets]) => [sourceId, targets.slice(0, 1)]),
      ));
      return;
    }
    if (!multiConfirmed) {
      setWarningOpen(true);
    }
  }, [multiConfirmed, multiTargetEligible]);

  const toggleSinglePatch = useCallback((enabled: boolean) => {
    const nextLimit = enabled ? singlePatchMappingLimit : separateOutputMappingLimit;
    if (configuredMappings.length <= nextLimit) {
      setSinglePatch(enabled);
      return;
    }
    showWebMappingLimitWarning(configuredMappings.length, nextLimit, t);
  }, [configuredMappings.length, separateOutputMappingLimit, singlePatchMappingLimit, t]);

  const chooseSource = useCallback((sourceId: string) => {
    setActiveSourceId(sourceId);
  }, []);

  const resolveSource = useCallback((sourceId: string, hash: string) => {
    const resolvedSources = sources.map((source) => (
      source.id === sourceId ? { ...source, resolvedHash: hash } : source
    ));
    setSources(resolvedSources);
    setTargetsBySource((current) => ({ ...current, [sourceId]: [] }));
    setMultiTarget(canUseMultiTarget(resolvedSources));
  }, [sources]);

  const setActiveSourceTargets = useCallback((targets: string[]) => {
    if (!activeSource) return;
    const nextTargetsBySource = { ...targetsBySource, [activeSource.id]: targets };
    const nextMappings = collectConfiguredMappings(sources, nextTargetsBySource);
    const nextSinglePatch = singlePatch || mustUseSinglePatch(nextMappings);
    const nextMappingLimit = nextSinglePatch
      ? singlePatchMappingLimit
      : separateOutputMappingLimit;
    const reducesMappingCount = nextMappings.length < configuredMappings.length;
    if (!reducesMappingCount
      && exceedsWebMappingLimit(nextMappings, nextMappingLimit)) {
      showWebMappingLimitWarning(nextMappings.length, nextMappingLimit, t);
      return;
    }
    setTargetsBySource(nextTargetsBySource);
  }, [
    activeSource,
    configuredMappings.length,
    singlePatch,
    singlePatchMappingLimit,
    separateOutputMappingLimit,
    sources,
    t,
    targetsBySource,
  ]);

  const chooseTarget = useCallback((hash: string) => {
    setActiveSourceTargets(selectTarget(activeTargets, hash, multiTarget));
  }, [activeTargets, multiTarget, setActiveSourceTargets]);

  const runMigration = useCallback(async () => {
    const patch = patchRef.current;
    if (!patch) return;
    if (webMappingLimitExceeded) {
      showWebMappingLimitWarning(selectedTargetCount, webMappingLimit, t);
      return;
    }
    const variants = buildMigrationVariants(configuredMappings, outputAsSinglePatch);
    if (!gameDir) return;
    const dataSource = new GameDataSource(gameDir.handle);
    setProgressLabel("");
    await runCancellableTask(async (signal) => {
      const result = await migrateVariants({
        dataSource,
        noPadding,
        options: equipmentOptions,
        patch,
        setProgressLabel,
        signal,
        t,
        unmatchedUnitPolicy,
        variants,
      });
      setCompletedReport({ kind: "migration", output: result.output, summary: result.summary });
    });
    setProgressLabel("");
  }, [
    gameDir,
    configuredMappings,
    equipmentOptions,
    noPadding,
    outputAsSinglePatch,
    selectedTargetCount,
    webMappingLimit,
    webMappingLimitExceeded,
    singlePatchMappingLimit,
    t,
    unmatchedUnitPolicy,
    runCancellableTask,
  ]);

  const runUnitRepatch = useCallback(async () => {
    const patch = patchRef.current;
    if (!patch || !gameDir) return;
    setProgressLabel(t("repatch.progress"));
    await runCancellableTask(async (signal) => {
      const output = await repatchUnits(
        patch,
        { missingUnitPolicy },
        new GameDataSource(gameDir.handle),
        {
          onStage: (_name, stage) => {
            throwIfTaskCancelled(signal);
            setProgressLabel(`${t("repatch.progress")} · ${stage}`);
          },
        },
      );
      throwIfTaskCancelled(signal);
      downloadRepatchedPatch(patch, output.tocBytes, "hd2-repatched-mod.zip");
      setCompletedReport({
        kind: "repatch",
        output: "hd2-repatched-mod.zip",
        summary: output.summary,
      });
    });
    setProgressLabel("");
  }, [gameDir, missingUnitPolicy, runCancellableTask, t]);

  const runSelectedTool = toolMode === "migrate" ? runMigration : runUnitRepatch;

  return (
    <div className="min-h-screen">
      <div
        className="fixed inset-0 z-0 bg-center bg-cover"
        style={{ backgroundImage: "url(/background.webp)", filter: "brightness(0.4)" }}
      />

      <Toaster position="top-center" theme="dark" />
      <ResultReportDialog
        equipmentOptions={equipmentOptions}
        onClose={() => setCompletedReport(null)}
        report={completedReport}
      />

      <div className="relative z-[1]">
      <main className="mx-auto w-full max-w-[56rem] px-4 py-6 min-[51.25rem]:px-6 min-[51.25rem]:py-10">
        <div className="overflow-hidden border-2 border-hd2-border bg-black/60">

          {/* Panel title bar */}
          <div className="flex flex-col items-center border-b border-hd2-border bg-hd2-surface/70 px-4 py-5">
            <div className="flex w-full items-center gap-3">
              <Tooltip title={t("github.revision", { hash: __GIT_HASH__ })}>
                <div
                  aria-label={t("github.revision", { hash: __GIT_HASH__ })}
                  className="flex w-[4.75rem] shrink-0 items-center gap-1 font-mono text-[0.625rem] tracking-wide text-hd2-faint min-[35rem]:w-24 min-[35rem]:text-[0.6875rem]"
                >
                  <AccountTreeIcon sx={{ fontSize: "0.875rem" }} />
                  <span>{__GIT_HASH__}</span>
                </div>
              </Tooltip>
              <div className="flex min-w-0 flex-1 items-center justify-center gap-3">
              <img alt="" className="hidden min-[40rem]:block" draggable={false} src="/title.svg" style={{ height: "2rem", transform: "scaleX(-1)" }} />
              <h1 className="m-0 text-center text-lg font-bold text-hd2-yellow min-[35rem]:text-xl min-[51.25rem]:text-2xl">{t("app.title")}</h1>
              <img alt="" className="hidden min-[40rem]:block" draggable={false} src="/title.svg" style={{ height: "2rem" }} />
              </div>
              <div className="flex w-[4.75rem] shrink-0 items-center justify-end min-[35rem]:w-24">
                <Tooltip title={t("github.openRepository")}>
                  <IconButton
                    aria-label={t("github.openRepository")}
                    className="headerIconBtn"
                    component="a"
                    href="https://github.com/eigeen/hd2-mod-crates"
                    rel="noreferrer"
                    size="small"
                    target="_blank"
                  >
                    <GitHubIcon fontSize="small" />
                  </IconButton>
                </Tooltip>
                <LanguageMenu />
              </div>
            </div>
          </div>

          <Tabs
            centered
            onChange={(_, value: ToolMode) => setToolMode(value)}
            value={toolMode}
          >
            <Tab label={t("mode.migrate")} value="migrate" />
            <Tab label={t("mode.repatch")} value="repatch" />
          </Tabs>

          <ToolIntro mode={toolMode} />

          {/* Row 1: game data dir + patch */}
          <div className="flex flex-col min-[51.25rem]:flex-row">
            <div className="flex min-w-0 flex-1">
              <GameDataDirPanel
                onChange={setGameDir}
                onDirectoryAccessAborted={() => {
                  setFaqAttention("faq.workaroundQuestion");
                  setFaqOpen(true);
                }}
                selection={gameDir}
              />
            </div>
            <div className="flex min-w-0 flex-1 border-t border-hd2-border min-[51.25rem]:border-t-0 min-[51.25rem]:border-l">
              <PatchPanel onPatchFiles={importPatchFiles} patch={patchInfo} />
            </div>
          </div>

          <div className="border-t border-hd2-border">
            {toolMode === "migrate" ? <TargetPanel
              activeSourceId={activeSourceId}
              equipmentOptions={equipmentOptions}
              multiTarget={multiTarget}
              multiTargetEligible={multiTargetEligible}
              onResolveSource={resolveSource}
              onMultiTargetChange={toggleMultiTarget}
              onSinglePatchChange={toggleSinglePatch}
              onSourceChange={chooseSource}
              onTargetChange={chooseTarget}
              selectedTargets={activeTargets}
              separateOutputMappingLimit={separateOutputMappingLimit}
              singlePatch={outputAsSinglePatch}
              singlePatchMappingLimit={singlePatchMappingLimit}
              singlePatchRequired={singlePatchRequired}
              sources={sources}
              targetOptions={targetOptions}
              targetSelectionEnabled={Boolean(activeSource?.resolvedHash)}
              targetsBySource={targetsBySource}
            /> : <UnitUpdaterPanel
              missingUnitPolicy={missingUnitPolicy}
              onMissingUnitPolicyChange={setMissingUnitPolicy}
            />}
          </div>

          {/* Action row: options + blocker hint + execute */}
          <div className="flex flex-col items-stretch gap-3 border-t border-hd2-border bg-hd2-pit px-5 py-3 min-[51.25rem]:flex-row min-[51.25rem]:items-center min-[51.25rem]:gap-4">
            {toolMode === "migrate" && (
              <div className="flex flex-wrap items-center gap-x-4 gap-y-2 min-[51.25rem]:shrink-0">
                <OptionsPanel
                  noPadding={noPadding}
                  setNoPadding={setNoPadding}
                  setUnmatchedUnitPolicy={setUnmatchedUnitPolicy}
                  unmatchedUnitPolicy={unmatchedUnitPolicy}
                />
              </div>
            )}
            <div className="flex min-w-0 flex-1 items-center gap-4">
              <div
                aria-atomic="true"
                aria-live="polite"
                className="flex min-w-0 flex-1 items-center justify-end gap-2"
              >
                <span className="flex h-5 w-5 shrink-0 items-center justify-center">
                  {busy && <CircularProgress size="1.25rem" />}
                </span>
                <span
                  className="min-w-0 truncate text-xs text-hd2-muted"
                  title={busy ? progressLabel : blockerHint}
                >
                  {busy ? progressLabel : blockerHint}
                </span>
              </div>
              {busy ? (
                <Button
                  className="shrink-0"
                  disabled={cancelling}
                  onClick={cancelActiveTask}
                  startIcon={<CancelIcon />}
                  variant="outlined"
                >
                  {cancelling ? t("task.cancelling") : t("task.cancel")}
                </Button>
              ) : (
                <Button
                  className="shrink-0"
                  disabled={!canRun}
                  onClick={runSelectedTool}
                  startIcon={<PlayArrowIcon />}
                  variant="contained"
                >
                  {toolMode === "migrate" ? t("app.run") : t("repatch.run")}
                </Button>
              )}
            </div>
          </div>
        </div>
        <FrequentlyAskedQuestions
          attentionQuestion={faqAttention}
          onOpenChange={(open) => {
            setFaqOpen(open);
            if (!open) setFaqAttention(null);
          }}
          open={faqOpen}
        />
      </main>
      </div>

      <PerformanceDialog
        open={warningOpen}
        onCancel={() => {
          setWarningOpen(false);
          setMultiTarget(false);
        }}
        onConfirm={() => {
          setMultiConfirmed(true);
          setWarningOpen(false);
        }}
      />
    </div>
  );
}

interface BlockerHintInput {
  crossArchiveReady: boolean;
  patchInfo: PatchInfo | null;
  selectedTargetCount: number;
  webMappingLimit: number;
  webMappingLimitExceeded: boolean;
}

function nextBlockerHint(input: BlockerHintInput, t: Translate): string {
  if (!input.crossArchiveReady) return t("app.blockerGameData");
  if (!input.patchInfo) return t("app.blockerPatch");
  if (!input.selectedTargetCount) return t("app.blockerTarget");
  if (input.webMappingLimitExceeded) {
    return t("app.blockerWebMappingLimit", {
      count: input.selectedTargetCount,
      max: input.webMappingLimit,
    });
  }
  return "";
}

function showWebMappingLimitWarning(count: number, max: number, t: Translate): void {
  toast.warning(t("app.blockerWebMappingLimit", { count, max }));
}

interface VariantMigrationRequest {
  dataSource: GameDataSource;
  noPadding: boolean;
  options: EquipmentOption[];
  patch: PatchFiles;
  setProgressLabel: (label: string) => void;
  signal: AbortSignal;
  t: Translate;
  unmatchedUnitPolicy: UnmatchedUnitPolicy;
  variants: MigrationVariant[];
}

/** Migrate all selected variants into one incrementally assembled ZIP. */
async function migrateVariants(
  request: VariantMigrationRequest,
): Promise<{ output: string; summary: MigrationSummary }> {
  const mappings = request.variants.flatMap((variant) => variant.mappings);
  const result = await migrateEquipmentVariants(
    request.patch,
    unifiedOptions(request.variants, request),
    request.dataSource,
    migrationProgress(
      mappings,
      request.options,
      request.setProgressLabel,
      request.signal,
      request.t,
    ),
  );
  throwIfTaskCancelled(request.signal);
  const output = migrationOutputFilename(request.patch, request.variants, request.options);
  downloadZip(result.zipBlob, output);
  return { output, summary: result.summary };
}

function migrationOutputFilename(
  patch: PatchFiles,
  variants: MigrationVariant[],
  options: EquipmentOption[],
): string {
  const variant = variants.length === 1 ? variants[0] : undefined;
  if (!variant) return "hd2-migrated-patch.zip";
  const mapping = variant.mappings.length === 1 ? variant.mappings[0] : undefined;
  if (!mapping) return "hd2-migrated-patch.zip";
  return uniqueOutputFilename(
    patch.originalName ?? patch.name,
    mapping.targetHash,
    options,
  );
}

function unifiedOptions(
  variants: MigrationVariant[],
  settings: Pick<VariantMigrationRequest, "noPadding" | "unmatchedUnitPolicy">,
) {
  return {
    variants,
    patchSuffix: PATCH_SUFFIX,
    noPadding: settings.noPadding,
    unmatchedUnitPolicy: settings.unmatchedUnitPolicy,
  };
}

function migrationProgress(
  mappings: MigrationMapping[],
  options: EquipmentOption[],
  setProgressLabel: (label: string) => void,
  signal: AbortSignal,
  t: Translate,
): MigrationProgressSink {
  let mappingIndex = 0;
  const label = () => {
    const mapping = mappings[Math.min(mappingIndex, mappings.length - 1)];
    const source = options.find((candidate) => candidate.hash === mapping?.sourceHash)?.name
      ?? mapping?.sourceHash;
    const target = options.find((candidate) => candidate.hash === mapping?.targetHash)?.name
      ?? mapping?.targetHash;
    return numberedTargetLabel(`${source} → ${target}`, mappingIndex, mappings.length);
  };
  return {
    onTargetStart: () => {
      throwIfTaskCancelled(signal);
      setProgressLabel(t("app.progressMigrating", { name: label() }));
    },
    onStage: (_name, stage) => {
      throwIfTaskCancelled(signal);
      setProgressLabel(t("app.progressStage", { name: label(), stage }));
    },
    onTargetFinish: () => {
      throwIfTaskCancelled(signal);
      mappingIndex += 1;
      setProgressLabel("");
    },
  };
}

function patchByteLength(patch: PatchFiles): number {
  return patch.toc.byteLength + patch.gpu.byteLength + patch.stream.byteLength;
}

function numberedTargetLabel(name: string, targetIndex: number, targetCount: number): string {
  if (targetCount === 1) return name;
  return `${targetIndex + 1}/${targetCount} ${name}`;
}

async function runTask(
  setBusy: (value: boolean) => void,
  task: () => Promise<void>,
  t: Translate,
) {
  setBusy(true);
  try {
    await task();
  } catch (error) {
    showTaskError(error, t);
  } finally {
    setBusy(false);
  }
}

function showTaskError(error: unknown, t: Translate): void {
  console.error("[hd2-migrator] task failed:", error);
  const taskError = normalizeTaskError(error);
  if (taskError.code === "task.cancelled") {
    toast.info(t("task.cancelled"));
    return;
  }
  toast.error(isWasmRuntimeTrapError(error) || taskError.code === "wasm.runtime"
    ? t("app.wasmRuntimeError")
    : taskError.message);
}

function patchFileMessages(t: Translate) {
  return {
    noToc: t("patch.noToc"),
    missingIntro: t("patch.missingIntro"),
    missingAction: (toc: string, gpu: string, stream: string) =>
      t("patch.missingAction", { toc, gpu, stream }),
    missingSidecar: (filename: string, expected: number) =>
      t("patch.missingSidecar", { filename, expected }),
    shortSidecar: (filename: string, expected: number, actual: number) =>
      t("patch.shortSidecar", { filename, expected, actual }),
  };
}

export default App;
