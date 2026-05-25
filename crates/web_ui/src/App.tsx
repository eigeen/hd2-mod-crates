import PlayArrowIcon from "@mui/icons-material/PlayArrow";
import { Button, CircularProgress } from "@mui/material";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { toast, Toaster } from "sonner";
import { downloadZip, patchFilesFromList } from "./fileInputs";
import { GameDataDirPanel, type GameDirSelection } from "./GameDataDirPanel";
import { GameDataSource } from "./gameDataSource";
import { useI18n, type Translate } from "./i18n";
import { LanguageMenu } from "./LanguageMenu";
import {
  OptionsPanel,
  PatchPanel,
  PerformanceDialog,
  TargetPanel,
} from "./MigratorPanels";
import type {
  MigrateOptions,
  MigrationSummary,
  PatchFiles,
  PatchInfo,
  TargetOption,
} from "./types";
import { builtinTargetOptions, detectSource, migrate, migrateCrossArchive } from "./wasmClient";

const PATCH_SUFFIX = "9ba626afa44a3aa3.patch_0";

function App() {
  const { t } = useI18n();
  const [targets, setTargets] = useState<TargetOption[]>([]);
  // patch 的 Uint8Array 可能上百 MB，放在 React state 里会被 React DevTools 扩展枚举/序列化导致 CPU 拉满 + 堆 OOM。
  // 因此把字节存到 ref，state 只留小元数据用于驱动 UI。
  const patchRef = useRef<PatchFiles | null>(null);
  const [patchInfo, setPatchInfo] = useState<PatchInfo | null>(null);
  const [sourceHash, setSourceHash] = useState("");
  const [targetHashes, setTargetHashes] = useState<string[]>([]);
  const [multiTarget, setMultiTarget] = useState(false);
  const [noPadding, setNoPadding] = useState(false);
  const [partialRemap, setPartialRemap] = useState(false);
  const [busy, setBusy] = useState(false);
  const [progressLabel, setProgressLabel] = useState("");
  const [warningOpen, setWarningOpen] = useState(false);
  const [multiConfirmed, setMultiConfirmed] = useState(false);
  const [showAllSources, setShowAllSources] = useState(false);
  const [gameDir, setGameDir] = useState<GameDirSelection | null>(null);

  useEffect(() => {
    builtinTargetOptions()
      .then(setTargets)
      .catch((error) => console.error("[hd2-migrator] load builtin targets failed:", error));
  }, []);

  const selectedTargetCount = targetHashes.length;
  const crossArchiveReady = gameDir !== null && gameDir.status.kind !== "empty";
  const canRun = Boolean(
    targets.length && patchInfo && sourceHash && selectedTargetCount && crossArchiveReady,
  );
  const blockerHint = canRun ? "" : nextBlockerHint({ crossArchiveReady, patchInfo, selectedTargetCount }, t);
  const patchMessages = useMemo(() => patchFileMessages(t), [t]);

  const sourceChoices = useMemo(
    () => (patchInfo ? sourceChoicesForSelection(targets, sourceHash, showAllSources) : []),
    [patchInfo, showAllSources, sourceHash, targets],
  );

  const targetOptions = useMemo(
    () => targets.filter((target) => target.hash !== sourceHash),
    [sourceHash, targets],
  );

  const applyPatch = useCallback(async (nextPatch: PatchFiles) => {
    patchRef.current = nextPatch;
    setPatchInfo({ name: nextPatch.name });
    setSourceHash("");
    setTargetHashes([]);
    setShowAllSources(false);
    await detectPatchSource({
      patch: nextPatch,
      setSourceHash,
      setShowAllSources,
    });
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
      experimentalPartialRemap: partialRemap,
    };
    const useCrossArchive = gameDir !== null && targetHashes.some((hash) => hash !== sourceHash);
    setProgressLabel("");
    await runTask(setBusy, async () => {
      const output = useCrossArchive && gameDir
        ? await migrateCrossArchive(patch, options, new GameDataSource(gameDir.handle), {
            onTargetStart: (name) => setProgressLabel(t("app.progressMigrating", { name })),
            onStage: (name, stage) => setProgressLabel(t("app.progressStage", { name, stage })),
            onTargetFinish: () => setProgressLabel(""),
          })
        : await migrate(patch, options);
      downloadZip(output.zipBytes, buildZipFilename(targetHashes, targets));
      showMigrationReport(output.summary, t);
    });
    setProgressLabel("");
  }, [
    gameDir,
    multiConfirmed,
    multiTarget,
    noPadding,
    partialRemap,
    sourceHash,
    targetHashes,
    targets,
    t,
  ]);

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
              <div className="w-8 shrink-0" />
              <div className="flex min-w-0 flex-1 items-center justify-center gap-3">
              <img alt="" draggable={false} src="/title.svg" style={{ height: "2rem", transform: "scaleX(-1)" }} />
              <h1 className="m-0 text-center text-xl font-bold text-hd2-yellow min-[51.25rem]:text-2xl">{t("app.title")}</h1>
              <img alt="" draggable={false} src="/title.svg" style={{ height: "2rem" }} />
              </div>
              <LanguageMenu />
            </div>
          </div>

          {/* Row 1: game data dir + patch */}
          <div className="flex flex-col min-[51.25rem]:flex-row">
            <div className="flex min-w-0 flex-1">
              <GameDataDirPanel onChange={setGameDir} selection={gameDir} />
            </div>
            <div className="flex min-w-0 flex-1 border-t border-hd2-border min-[51.25rem]:border-t-0 min-[51.25rem]:border-l">
              <PatchPanel onPatchFiles={importPatchFiles} patch={patchInfo} />
            </div>
          </div>

          {/* Row 2: migration mapping */}
          <div className="border-t border-hd2-border">
            <TargetPanel
              multiTarget={multiTarget}
              onBatchSelect={setTargetHashes}
              onMultiTargetChange={toggleMultiTarget}
              onSourceChange={chooseSource}
              onTargetChange={chooseTarget}
              selectedTargets={targetHashes}
              sourceHash={sourceHash}
              sourceChoices={sourceChoices}
              targetOptions={targetOptions}
            />
          </div>

          {/* Action row: options + blocker hint + execute */}
          <div className="flex flex-wrap items-center gap-x-4 gap-y-2 border-t border-hd2-border bg-hd2-pit px-5 py-3">
            <OptionsPanel
              noPadding={noPadding}
              partialRemap={partialRemap}
              setNoPadding={setNoPadding}
              setPartialRemap={setPartialRemap}
            />
            <div className="flex-1" />
            {busy && <CircularProgress size="1.25rem" />}
            {busy && progressLabel
              ? <span className="text-xs text-hd2-muted">{progressLabel}</span>
              : !canRun && <span className="text-xs text-hd2-muted">{blockerHint}</span>
            }
            <Button
              disabled={!canRun || busy}
              onClick={runMigration}
              startIcon={<PlayArrowIcon />}
              variant="contained"
            >
              {t("app.run")}
            </Button>
          </div>
        </div>
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

/** Build a zip filename, including the target name when there is only one target. */
function buildZipFilename(targetHashes: string[], targets: TargetOption[]): string {
  if (targetHashes.length === 1) {
    const target = targets.find((t) => t.hash === targetHashes[0]);
    if (target) {
      const safe = target.name.replace(/[\\/:*?"<>|]/g, "_").trim();
      return `hd2-patch-${safe}.zip`;
    }
  }
  return "hd2-migrated-patch.zip";
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
  patch: PatchFiles;
  setSourceHash: (hash: string) => void;
  setShowAllSources: (show: boolean) => void;
}

async function detectPatchSource(request: DetectPatchSourceRequest) {
  const { patch, setSourceHash, setShowAllSources } = request;
  const source = await detectSource(patch);
  if (source) {
    setSourceHash(source.hash);
    return;
  }
  setShowAllSources(true);
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
