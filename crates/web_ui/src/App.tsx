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
  migrateTargetsToBatchDownloads,
  type MigrationBatch,
} from "./migrationDownloads";
import {
  OptionsPanel,
  PatchPanel,
  PerformanceDialog,
  TargetPanel,
} from "./MigratorPanels";
import { UnitUpdaterPanel } from "./UnitUpdaterPanel";
import { ToolIntro } from "./ToolIntro";
import { UnclaimedModelAlert } from "./UnclaimedModelAlert";
import type {
  DetectedModel,
  MissingUnitPolicy,
  MigrateOptions,
  MigrationCategory,
  MigrationSummary,
  PatchFiles,
  PatchInfo,
  TargetOption,
  UnmatchedUnitPolicy,
  UnitRepatchSummary,
} from "./types";
import {
  builtinTargetOptions,
  detectSource,
  inspectPatchContents,
  migrate,
  migrateCrossArchive,
  repatchUnits,
  type MigrationProgressSink,
} from "./wasmClient";

const PATCH_SUFFIX = "9ba626afa44a3aa3.patch_0";
type ToolMode = "migrate" | "repatch";

function App() {
  const { t } = useI18n();
  const [toolMode, setToolMode] = useState<ToolMode>("migrate");
  const [migrationCategory, setMigrationCategory] = useState<MigrationCategory>("Armor");
  const [targets, setTargets] = useState<TargetOption[]>([]);
  // patch 的 Uint8Array 可能上百 MB，放在 React state 里会被 React DevTools 扩展枚举/序列化导致 CPU 拉满 + 堆 OOM。
  // 因此把字节存到 ref，state 只留小元数据用于驱动 UI。
  const patchRef = useRef<PatchFiles | null>(null);
  const [patchInfo, setPatchInfo] = useState<PatchInfo | null>(null);
  const [sourceHash, setSourceHash] = useState("");
  const [targetHashes, setTargetHashes] = useState<string[]>([]);
  const [multiTarget, setMultiTarget] = useState(false);
  const [noPadding, setNoPadding] = useState(false);
  const [unmatchedUnitPolicy, setUnmatchedUnitPolicy] = useState<UnmatchedUnitPolicy>("drop");
  const [busy, setBusy] = useState(false);
  const [progressLabel, setProgressLabel] = useState("");
  const [warningOpen, setWarningOpen] = useState(false);
  const [multiConfirmed, setMultiConfirmed] = useState(false);
  const [showAllSources, setShowAllSources] = useState(false);
  const [detectedModels, setDetectedModels] = useState<DetectedModel[]>([]);
  const [gameDir, setGameDir] = useState<GameDirSelection | null>(null);
  const [missingUnitPolicy, setMissingUnitPolicy] = useState<MissingUnitPolicy>("drop");
  const [faqOpen, setFaqOpen] = useState(false);
  const [faqAttention, setFaqAttention] = useState<TranslationKey | null>(null);

  useEffect(() => {
    let cancelled = false;
    setTargets([]);
    builtinTargetOptions(migrationCategory)
      .then((options) => {
        if (!cancelled) setTargets(options);
      })
      .catch((error) => console.error("[hd2-migrator] load builtin targets failed:", error));
    return () => {
      cancelled = true;
    };
  }, [migrationCategory]);

  const selectedTargetCount = targetHashes.length;
  const crossArchiveReady = gameDir !== null && gameDir.status.kind !== "empty";
  const canMigrate = Boolean(
    targets.length && patchInfo && sourceHash && selectedTargetCount && crossArchiveReady,
  );
  const canRepatch = Boolean(patchInfo && crossArchiveReady);
  const canRun = toolMode === "migrate" ? canMigrate : canRepatch;
  const blockerHint = canRun ? "" : nextBlockerHint({
    crossArchiveReady,
    patchInfo,
    selectedTargetCount: toolMode === "migrate" ? selectedTargetCount : 1,
  }, t);
  const patchMessages = useMemo(() => patchFileMessages(t), [t]);

  const sourceChoices = useMemo(
    () => (patchInfo ? sourceChoicesForSelection(targets, sourceHash, showAllSources) : []),
    [patchInfo, showAllSources, sourceHash, targets],
  );

  const targetOptions = useMemo(
    () => targets.filter((target) => target.hash !== sourceHash),
    [sourceHash, targets],
  );
  const currentSourceName = useMemo(
    () => targets.find((target) => target.hash === sourceHash)?.name ?? null,
    [sourceHash, targets],
  );

  const applyPatch = useCallback(async (nextPatch: PatchFiles) => {
    patchRef.current = nextPatch;
    setPatchInfo({ name: nextPatch.name });
    setSourceHash("");
    setTargetHashes([]);
    setShowAllSources(false);
    setDetectedModels([]);
    await inspectPatch({
      category: migrationCategory,
      patch: nextPatch,
      setDetectedModels,
      setSourceHash,
      setShowAllSources,
    });
  }, [migrationCategory]);

  const chooseMigrationCategory = useCallback((category: MigrationCategory) => {
    setMigrationCategory(category);
    setSourceHash("");
    setTargetHashes([]);
    setShowAllSources(false);
    const patch = patchRef.current;
    if (!patch) return;
    void runTask(setBusy, () => detectPatchSource({
      category,
      patch,
      setSourceHash,
      setShowAllSources,
    }));
  }, []);

  const importPatchFiles = useCallback(
    async (files: FileList | null) => {
      if (!files) return;
      await runTask(setBusy, async () => {
        const nextPatch = await patchFilesFromList(files, patchMessages);
        await applyPatch(nextPatch);
      });
    },
    [applyPatch, patchMessages],
  );

  const toggleMultiTarget = useCallback((enabled: boolean) => {
    setMultiTarget(enabled);
    if (!enabled) {
      setTargetHashes((current) => current.slice(0, 1));
      return;
    }
    if (!multiConfirmed) {
      setWarningOpen(true);
    }
  }, [multiConfirmed]);

  const chooseSource = useCallback((hash: string) => {
    setSourceHash(hash);
    setTargetHashes((current) => current.filter((h) => h !== hash));
  }, []);

  const chooseTarget = useCallback(
    (hash: string) => {
      if (!multiTarget) {
        setTargetHashes([hash]);
        return;
      }
      setTargetHashes((current) => toggleHash(current, hash));
    },
    [multiTarget],
  );

  const runMigration = useCallback(async () => {
    const patch = patchRef.current;
    if (!patch) return;
    if (multiTarget && targetHashes.length > 1 && !multiConfirmed) {
      setWarningOpen(true);
      return;
    }
    const options: MigrateOptions = {
      sourceHash,
      targetHashes,
      patchSuffix: PATCH_SUFFIX,
      noPadding,
      unmatchedUnitPolicy,
    };
    const dataSource = gameDir ? new GameDataSource(gameDir.handle) : null;
    setProgressLabel("");
    await runTask(setBusy, async () => {
      const summary = await migrateTargetsToBatchDownloads({
        patchByteLength: patch.toc.byteLength + patch.gpu.byteLength + patch.stream.byteLength,
        targetHashes,
        targets,
        migrateBatch: (batch) => migrateTargetBatch({
          batch,
          category: migrationCategory,
          dataSource,
          options,
          patch,
          progress: migrationProgress(batch, setProgressLabel, t),
        }),
        download: downloadZip,
      });
      showMigrationReport(summary, t);
    });
    setProgressLabel("");
  }, [
    gameDir,
    migrationCategory,
    multiConfirmed,
    multiTarget,
    noPadding,
    sourceHash,
    targetHashes,
    targets,
    t,
    unmatchedUnitPolicy,
  ]);

  const runUnitRepatch = useCallback(async () => {
    const patch = patchRef.current;
    if (!patch || !gameDir) return;
    setProgressLabel(t("repatch.progress"));
    await runTask(setBusy, async () => {
      const output = await repatchUnits(
        patch,
        { missingUnitPolicy },
        new GameDataSource(gameDir.handle),
      );
      downloadRepatchedPatch(patch, output.tocBytes, "hd2-repatched-mod.zip");
      showUnitRepatchReport(output.summary, t);
    });
    setProgressLabel("");
  }, [gameDir, missingUnitPolicy, t]);

  const runSelectedTool = toolMode === "migrate" ? runMigration : runUnitRepatch;

  return (
    <div className="min-h-screen">
      <div
        className="fixed inset-0 z-0 bg-center bg-cover"
        style={{ backgroundImage: "url(/background.webp)", filter: "brightness(0.4)" }}
      />

      <Toaster position="top-center" theme="dark" />

      <div className="relative z-[1]">
      <main className="mx-auto w-full max-w-[56rem] px-4 py-6 min-[51.25rem]:px-6 min-[51.25rem]:py-10">
        <div className="overflow-hidden border-2 border-hd2-border bg-black/60">

          {/* Panel title bar */}
          <div className="flex flex-col items-center border-b border-hd2-border bg-hd2-surface/70 px-4 py-5">
            <div className="flex w-full items-center gap-3">
              <div className="w-[4.75rem] shrink-0 min-[35rem]:w-24" />
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
              category={migrationCategory}
              multiTarget={multiTarget}
              onBatchSelect={setTargetHashes}
              onCategoryChange={chooseMigrationCategory}
              onMultiTargetChange={toggleMultiTarget}
              onSourceChange={chooseSource}
              onTargetChange={chooseTarget}
              selectedTargets={targetHashes}
              sourceHash={sourceHash}
              sourceChoices={sourceChoices}
              targetOptions={targetOptions}
            /> : <UnitUpdaterPanel
              missingUnitPolicy={missingUnitPolicy}
              onMissingUnitPolicyChange={setMissingUnitPolicy}
            />}
          </div>

          {toolMode === "migrate" && (
            <UnclaimedModelAlert
              currentCategory={migrationCategory}
              currentSourceName={currentSourceName}
              detectedModels={detectedModels}
            />
          )}

          {/* Action row: options + blocker hint + execute */}
          <div className="flex flex-wrap items-center gap-x-4 gap-y-2 border-t border-hd2-border bg-hd2-pit px-5 py-3">
            {toolMode === "migrate" && <OptionsPanel
              noPadding={noPadding}
              setNoPadding={setNoPadding}
              setUnmatchedUnitPolicy={setUnmatchedUnitPolicy}
              unmatchedUnitPolicy={unmatchedUnitPolicy}
            />}
            <div className="flex-1" />
            {busy && <CircularProgress size="1.25rem" />}
            {busy && progressLabel
              ? <span className="text-xs text-hd2-muted">{progressLabel}</span>
              : !canRun && <span className="text-xs text-hd2-muted">{blockerHint}</span>
            }
            <Button
              disabled={!canRun || busy}
              onClick={runSelectedTool}
              startIcon={<PlayArrowIcon />}
              variant="contained"
            >
              {toolMode === "migrate" ? t("app.run") : t("repatch.run")}
            </Button>
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
}

function nextBlockerHint(input: BlockerHintInput, t: Translate): string {
  if (!input.crossArchiveReady) return t("app.blockerGameData");
  if (!input.patchInfo) return t("app.blockerPatch");
  if (!input.selectedTargetCount) return t("app.blockerTarget");
  return "";
}

interface TargetBatchMigrationRequest {
  batch: MigrationBatch;
  category: MigrationCategory;
  dataSource: GameDataSource | null;
  options: MigrateOptions;
  patch: PatchFiles;
  progress: MigrationProgressSink;
}

/** Route one memory-bounded target batch through the applicable WASM migration entry point. */
async function migrateTargetBatch(request: TargetBatchMigrationRequest) {
  const options = { ...request.options, targetHashes: request.batch.targetHashes };
  const needsGameData = options.targetHashes.some((hash) => hash !== options.sourceHash);
  if (!request.dataSource || !needsGameData) {
    return migrate(request.patch, options, request.category);
  }
  return migrateCrossArchive(
    request.patch,
    options,
    request.dataSource,
    request.progress,
    request.category,
  );
}

function migrationProgress(
  batch: MigrationBatch,
  setProgressLabel: (label: string) => void,
  t: Translate,
): MigrationProgressSink {
  let targetIndex = batch.targetOffset;
  const label = (name: string) => numberedTargetLabel(name, targetIndex, batch.targetCount);
  return {
    onTargetStart: (name) => setProgressLabel(t("app.progressMigrating", { name: label(name) })),
    onStage: (name, stage) => setProgressLabel(t("app.progressStage", { name: label(name), stage })),
    onTargetFinish: () => {
      targetIndex += 1;
      setProgressLabel("");
    },
  };
}

function numberedTargetLabel(name: string, targetIndex: number, targetCount: number): string {
  if (targetCount === 1) return name;
  return `${targetIndex + 1}/${targetCount} ${name}`;
}

function showMigrationReport(summary: MigrationSummary, t: Translate): void {
  const details = summary.reports
    .map((r) => {
      const parts: string[] = [r.targetName];
      if (r.fileIdRemapped) parts.push(t("report.remapped", { count: r.fileIdRemapped }));
      if (r.paddedUnits) parts.push(t("report.padded", { count: r.paddedUnits }));
      if (r.skippedEntries) parts.push(t("report.skipped", { count: r.skippedEntries }));
      if (r.warnings.length) parts.push(t("report.warnings", { count: r.warnings.length }));
      return parts.join(" · ");
    })
    .join("\n");

  const description = <span style={{ whiteSpace: "pre-line" }}>{details}</span>;
  const title = t("report.title", { count: summary.migratedCount });
  if (summary.warningCount > 0) {
    toast.warning(title, { description, duration: 8000 });
  } else {
    toast.success(title, { description, duration: 6000 });
  }
}

function showUnitRepatchReport(summary: UnitRepatchSummary, t: Translate): void {
  const description = t("repatch.reportDetails", {
    updated: summary.updatedUnits,
    current: summary.alreadyCurrentUnits,
    removed: summary.removedUnits,
    failed: summary.failedUnits,
    archives: summary.scannedArchives,
  });
  const options = { description, duration: summary.warnings.length ? 8000 : 6000 };
  if (summary.warnings.length || summary.failedUnits) {
    toast.warning(t("repatch.reportTitle"), options);
    return;
  }
  toast.success(t("repatch.reportTitle"), options);
}

function sourceChoicesForSelection(
  targets: TargetOption[],
  sourceHash: string,
  showAllSources: boolean,
) {
  if (showAllSources) return targets;
  const selectedSource = targets.find((target) => target.hash === sourceHash);
  return selectedSource ? [selectedSource] : [];
}

interface DetectPatchSourceRequest {
  category: MigrationCategory;
  patch: PatchFiles;
  setSourceHash: (hash: string) => void;
  setShowAllSources: (show: boolean) => void;
}

async function detectPatchSource(request: DetectPatchSourceRequest) {
  const { category, patch, setSourceHash, setShowAllSources } = request;
  const source = await detectSource(patch, category);
  if (source) {
    setSourceHash(source.hash);
    return;
  }
  setShowAllSources(true);
}

interface InspectPatchRequest extends DetectPatchSourceRequest {
  setDetectedModels: (models: DetectedModel[]) => void;
}

async function inspectPatch(request: InspectPatchRequest) {
  const inspection = await inspectPatchContents(request.patch, request.category);
  request.setDetectedModels(inspection.models);
  if (inspection.source) {
    request.setSourceHash(inspection.source.hash);
    return;
  }
  request.setShowAllSources(true);
}

function toggleHash(values: string[], hash: string) {
  if (values.includes(hash)) return values.filter((v) => v !== hash);
  return [...values, hash];
}

async function runTask(
  setBusy: (value: boolean) => void,
  task: () => Promise<void>,
) {
  setBusy(true);
  try {
    await task();
  } catch (error) {
    console.error("[hd2-migrator] task failed:", error);
    toast.error(errorMessage(error));
  } finally {
    setBusy(false);
  }
}

function errorMessage(error: unknown) {
  if (error && typeof error === "object" && "message" in error) {
    return String((error as { message: unknown }).message);
  }
  return String(error);
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
