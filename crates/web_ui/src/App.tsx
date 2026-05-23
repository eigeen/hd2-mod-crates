import ElectricBoltIcon from "@mui/icons-material/ElectricBolt";
import PlayArrowIcon from "@mui/icons-material/PlayArrow";
import { Button, CircularProgress } from "@mui/material";
import { useCallback, useEffect, useMemo, useState } from "react";
import { patchFilesFromList } from "./fileInputs";
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
  MigrationResult,
  PatchFiles,
  TargetOption,
} from "./types";
import { builtinTargetOptions, detectSource, migrate, migrateCrossArchive } from "./wasmClient";

const PATCH_SUFFIX = "9ba626afa44a3aa3.patch_0";

function App() {
  const [authority, setAuthority] = useState<AuthorityMappings | null>(null);
  const [targets, setTargets] = useState<TargetOption[]>([]);
  const [patch, setPatch] = useState<PatchFiles | null>(null);
  const [sourceHash, setSourceHash] = useState("");
  const [targetHashes, setTargetHashes] = useState<string[]>([]);
  const [multiTarget, setMultiTarget] = useState(false);
  const [noPadding, setNoPadding] = useState(false);
  const [partialRemap, setPartialRemap] = useState(false);
  const [busy, setBusy] = useState(false);
  const [progressLabel, setProgressLabel] = useState("");
  const [errorText, setErrorText] = useState("");
  const [result, setResult] = useState<MigrationResult | null>(null);
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
  const canRun = Boolean(targets.length && patch && sourceHash && selectedTargetCount);
  const crossArchiveReady = gameDir !== null && gameDir.status.kind !== "empty";

  const sourceChoices = useMemo(
    () => (patch ? sourceChoicesForSelection(targets, sourceHash, showAllSources, crossArchiveReady) : []),
    [crossArchiveReady, patch, showAllSources, sourceHash, targets],
  );

  const targetOptions = useMemo(
    () => targets.filter((target) => target.hash !== sourceHash),
    [sourceHash, targets],
  );

  const applyPatch = useCallback(async (nextPatch: PatchFiles) => {
    setPatch(nextPatch);
    setResult(null);
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
    setResult(null);
  }, []);

  const chooseTarget = useCallback(
    (hash: string) => {
      setResult(null);
      if (!multiTarget) {
        setTargetHashes([hash]);
        return;
      }
      setTargetHashes((current) => toggleHash(current, hash));
    },
    [multiTarget],
  );

  const runMigration = useCallback(async () => {
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
      if (useCrossArchive && gameDir) {
        const dataSource = new GameDataSource(gameDir.handle);
        const output = await migrateCrossArchive(patch, options, dataSource, {
          onTargetStart: (name) => setProgressLabel(`正在迁移 ${name}`),
          onStage: (name, stage) => setProgressLabel(`${name} · ${stage}`),
          onTargetFinish: () => setProgressLabel(""),
        });
        setResult(output);
      } else {
        const output = await migrate(patch, options);
        setResult(output);
      }
    });
    setProgressLabel("");
  }, [
    gameDir,
    multiConfirmed,
    multiTarget,
    noPadding,
    partialRemap,
    patch,
    sourceHash,
    targetHashes,
  ]);

  return (
    <div className="min-h-screen">
      <header className="sticky top-0 z-[2] flex flex-col items-start gap-4 border-b border-slate-200/85 bg-white/85 px-5 py-4 backdrop-blur-[14px] min-[820px]:flex-row min-[820px]:items-center min-[820px]:px-8">
        <div className="flex min-w-0 flex-row items-center gap-3">
          <div className="flex h-7 w-7 items-center justify-center rounded-lg bg-blue-600 text-white shadow-[0_8px_18px_rgb(37_99_235_/_0.18)] [&_svg]:text-[18px]">
            <ElectricBoltIcon />
          </div>
          <div>
            <h1 className="m-0 text-base font-bold leading-tight text-slate-900">HD2 外观 Mod 迁移工具</h1>
            <p className="m-0 text-xs text-slate-500">导入补丁、识别来源、选择目标</p>
          </div>
        </div>
        <div className="hidden flex-1 min-[820px]:block" />
        <div className="flex flex-row items-center gap-3 max-[819px]:w-full">
          {busy && <CircularProgress size={22} />}
          {busy && progressLabel && (
            <span className="text-xs text-slate-600 max-[819px]:hidden">{progressLabel}</span>
          )}
          <Button
            className="max-[819px]:flex-1!"
            disabled={!canRun || busy}
            onClick={runMigration}
            startIcon={<PlayArrowIcon />}
            variant="contained"
          >
            运行
          </Button>
        </div>
      </header>

      <main className="mx-auto flex w-full max-w-[896px] flex-col gap-5 px-4 py-6 min-[820px]:px-6 min-[820px]:py-10">
        <div className="flex flex-col items-stretch gap-5 min-[820px]:flex-row">
          <AuthorityPanel
            authority={authority}
            crossArchiveReady={crossArchiveReady}
            targetCount={targets.length}
          />
          <PatchPanel onPatchFiles={importPatchFiles} patch={patch} />
        </div>
        <GameDataDirPanel onChange={setGameDir} selection={gameDir} />
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
        <ResultPanel errorText={errorText} result={result} />
        {!errorText && !result && (
          <div className="flex items-center gap-2.5 rounded-xl border border-blue-100 bg-blue-50/70 px-4 py-3 text-blue-800">
            <span className="h-2 w-2 rounded-full bg-blue-600 shadow-[0_0_0_5px_rgb(59_130_246_/_0.16)]" />
            <p className="m-0 text-xs font-bold">{busy && progressLabel ? progressLabel : "就绪"}</p>
          </div>
        )}
      </main>

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
  crossArchiveReady: boolean,
) {
  if (showAllSources || crossArchiveReady) {
    return targets;
  }
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
