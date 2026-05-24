import ElectricBoltIcon from "@mui/icons-material/ElectricBolt";
import PlayArrowIcon from "@mui/icons-material/PlayArrow";
import { Button, CircularProgress } from "@mui/material";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { downloadZip, patchFilesFromList } from "./fileInputs";
import { GameDataDirPanel, type GameDirSelection } from "./GameDataDirPanel";
import { GameDataSource } from "./gameDataSource";
import {
  AuthorityPanel,
  PatchPanel,
  PerformanceDialog,
  ResultPanel,
  TargetPanel,
} from "./MigratorPanels";
import { loadAuthorityMappings } from "./metadata";
import type {
  AuthorityMappings,
  MigrateOptions,
  MigrationSummary,
  PatchFiles,
  PatchInfo,
  TargetOption,
} from "./types";
import { builtinTargetOptions, detectSource, migrate, migrateCrossArchive } from "./wasmClient";

const PATCH_SUFFIX = "9ba626afa44a3aa3.patch_0";

function App() {
  const [authority, setAuthority] = useState<AuthorityMappings | null>(null);
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
  const [errorText, setErrorText] = useState("");
  // zipBytes 同样可能上百 MB —— 与 patch 字节一样放 ref，state 只留 summary，避免 React DevTools 枚举触发 OOM。
  const resultBytesRef = useRef<Uint8Array | null>(null);
  const [resultSummary, setResultSummary] = useState<MigrationSummary | null>(null);
  const [warningOpen, setWarningOpen] = useState(false);
  const [multiConfirmed, setMultiConfirmed] = useState(false);
  const [showAllSources, setShowAllSources] = useState(false);
  const [gameDir, setGameDir] = useState<GameDirSelection | null>(null);

  useEffect(() => {
    loadAuthorityMappings()
      .then(setAuthority)
      .catch((error) => console.error("[hd2-migrator] load authority mappings failed:", error));
  }, []);

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
  const blockerHint = nextBlockerHint({
    crossArchiveReady,
    patchInfo,
    selectedTargetCount,
  });

  const sourceChoices = useMemo(
    () => (patchInfo ? sourceChoicesForSelection(targets, sourceHash, showAllSources) : []),
    [patchInfo, showAllSources, sourceHash, targets],
  );

  const targetOptions = useMemo(
    () => targets.filter((target) => target.hash !== sourceHash),
    [sourceHash, targets],
  );

  const clearResult = useCallback(() => {
    resultBytesRef.current = null;
    setResultSummary(null);
  }, []);

  const downloadResult = useCallback(() => {
    const bytes = resultBytesRef.current;
    if (!bytes) {
      return;
    }
    downloadZip(bytes, "hd2-migrated-patch.zip");
  }, []);

  const applyPatch = useCallback(async (nextPatch: PatchFiles) => {
    patchRef.current = nextPatch;
    setPatchInfo({ name: nextPatch.name });
    resultBytesRef.current = null;
    setResultSummary(null);
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
      if (!files) {
        return;
      }
      await runTask(setBusy, setErrorText, async () => {
        const nextPatch = await patchFilesFromList(files);
        await applyPatch(nextPatch);
      });
    },
    [applyPatch],
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
    setTargetHashes((current) => current.filter((targetHash) => targetHash !== hash));
    clearResult();
  }, [clearResult]);

  const chooseTarget = useCallback(
    (hash: string) => {
      clearResult();
      if (!multiTarget) {
        setTargetHashes([hash]);
        return;
      }
      setTargetHashes((current) => toggleHash(current, hash));
    },
    [clearResult, multiTarget],
  );

  const runMigration = useCallback(async () => {
    const patch = patchRef.current;
    if (!patch) {
      return;
    }
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
    await runTask(setBusy, setErrorText, async () => {
      const output = useCrossArchive && gameDir
        ? await migrateCrossArchive(patch, options, new GameDataSource(gameDir.handle), {
            onTargetStart: (name) => setProgressLabel(`正在迁移 ${name}`),
            onStage: (name, stage) => setProgressLabel(`${name} · ${stage}`),
            onTargetFinish: () => setProgressLabel(""),
          })
        : await migrate(patch, options);
      resultBytesRef.current = output.zipBytes;
      setResultSummary(output.summary);
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
  ]);

  return (
    <div className="min-h-screen">
      {/* Fixed game background image — z-0 so it paints above the html canvas bg */}
      <div
        className="fixed inset-0 z-0 bg-center bg-cover"
        style={{ backgroundImage: "url(/background.webp)", filter: "brightness(0.4)" }}
      />

      {/* Content wrapper sits above the background layer */}
      <div className="relative z-[1]">
      <header className="sticky top-0 z-[2] flex flex-col items-start gap-4 border-b border-hd2-border bg-hd2-surface/95 px-5 py-4 backdrop-blur-[0.875rem] min-[51.25rem]:flex-row min-[51.25rem]:items-center min-[51.25rem]:px-8">
        <div className="flex min-w-0 flex-row items-center gap-3">
          <div className="flex h-7 w-7 items-center justify-center bg-hd2-yellow text-hd2-bg [&_svg]:text-[1.125rem]">
            <ElectricBoltIcon />
          </div>
          <div>
            <h1 className="m-0 text-base font-bold leading-tight text-hd2-text">HD2 外观 Mod 迁移工具</h1>
            <p className="m-0 text-xs text-hd2-muted">导入补丁、识别来源、选择目标</p>
          </div>
        </div>
        <div className="hidden flex-1 min-[51.25rem]:block" />
        <div className="flex flex-row items-center gap-3 max-[51.1875rem]:w-full">
          {busy && <CircularProgress size="1.375rem" />}
          {busy && progressLabel && (
            <span className="text-xs text-hd2-muted max-[51.1875rem]:hidden">{progressLabel}</span>
          )}
          <Button
            className="max-[51.1875rem]:flex-1!"
            disabled={!canRun || busy}
            onClick={runMigration}
            startIcon={<PlayArrowIcon />}
            variant="contained"
          >
            运行
          </Button>
        </div>
      </header>

      <main className="mx-auto w-full max-w-[56rem] px-4 py-6 min-[51.25rem]:px-6 min-[51.25rem]:py-10">
        {/* Unified panel: semi-transparent with outer border — replaces individual cards */}
        <div className="overflow-hidden border-2 border-hd2-border bg-black/60">

          {/* Row 1: source + patch side by side */}
          <div className="flex flex-col min-[51.25rem]:flex-row">
            <div className="flex min-w-0 flex-1">
              <AuthorityPanel
                authority={authority}
                crossArchiveReady={crossArchiveReady}
                targetCount={targets.length}
              />
            </div>
            <div className="flex min-w-0 flex-1 border-t border-hd2-border min-[51.25rem]:border-t-0 min-[51.25rem]:border-l">
              <PatchPanel onPatchFiles={importPatchFiles} patch={patchInfo} />
            </div>
          </div>

          {/* Row 2: game data directory */}
          <div className="border-t border-hd2-border">
            <GameDataDirPanel onChange={setGameDir} selection={gameDir} />
          </div>

          {/* Row 3: migration mapping */}
          <div className="border-t border-hd2-border">
            <TargetPanel
              multiTarget={multiTarget}
              noPadding={noPadding}
              onMultiTargetChange={toggleMultiTarget}
              onSourceChange={chooseSource}
              onTargetChange={chooseTarget}
              partialRemap={partialRemap}
              selectedTargets={targetHashes}
              setNoPadding={setNoPadding}
              setPartialRemap={setPartialRemap}
              sourceHash={sourceHash}
              sourceChoices={sourceChoices}
              targetOptions={targetOptions}
            />
          </div>

          {/* Row 4: result or status hint */}
          {(errorText || resultSummary) && (
            <div className="border-t border-hd2-border">
              <ResultPanel
                errorText={errorText}
                onDownload={downloadResult}
                summary={resultSummary}
              />
            </div>
          )}
          {!errorText && !resultSummary && (
            <div className="flex items-center gap-2.5 border-t border-hd2-border px-4 py-3 text-hd2-text">
              <span className="h-2 w-2 bg-hd2-yellow shadow-(--hd2-glow-dot)" />
              <p className="m-0 text-xs font-bold">{busy && progressLabel ? progressLabel : blockerHint}</p>
            </div>
          )}
        </div>
      </main>
      </div>{/* end content wrapper */}

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

function sourceChoicesForSelection(
  targets: TargetOption[],
  sourceHash: string,
  showAllSources: boolean,
) {
  if (showAllSources) {
    return targets;
  }
  const selectedSource = targets.find((target) => target.hash === sourceHash);
  return selectedSource ? [selectedSource] : [];
}

interface BlockerHintInput {
  crossArchiveReady: boolean;
  patchInfo: PatchInfo | null;
  selectedTargetCount: number;
}

function nextBlockerHint(input: BlockerHintInput): string {
  if (!input.crossArchiveReady) {
    return "请先在下方选择游戏 data 目录";
  }
  if (!input.patchInfo) {
    return "请选择补丁文件";
  }
  if (!input.selectedTargetCount) {
    return "请选择目标版本";
  }
  return "就绪";
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
  if (values.includes(hash)) {
    return values.filter((value) => value !== hash);
  }
  return [...values, hash];
}

async function runTask(
  setBusy: (value: boolean) => void,
  setErrorText: (value: string) => void,
  task: () => Promise<void>,
) {
  setBusy(true);
  setErrorText("");
  try {
    await task();
  } catch (error) {
    console.error("[hd2-migrator] task failed:", error);
    setErrorText(errorMessage(error));
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

export default App;
