import ElectricBoltIcon from "@mui/icons-material/ElectricBolt";
import PlayArrowIcon from "@mui/icons-material/PlayArrow";
import { Button, CircularProgress } from "@mui/material";
import { useCallback, useMemo, useState } from "react";
import { patchFilesFromList } from "./fileInputs";
import {
  MetadataPanel,
  PatchPanel,
  PerformanceDialog,
  ResultPanel,
  TargetPanel,
} from "./MigratorPanels";
import {
  buildMetadataFromDirectory,
  fallbackTargetOptions,
  loadMetadataFile,
  loadPublicMetadata,
} from "./metadata";
import type { MetadataState, MigrationResult, PatchFiles, TargetOption } from "./types";
import { detectSource, listTargets, migrate } from "./wasmClient";

const patchSuffix = "9ba626afa44a3aa3.patch_0";

function App() {
  const [metadata, setMetadata] = useState<MetadataState | null>(null);
  const [targets, setTargets] = useState<TargetOption[]>([]);
  const [patch, setPatch] = useState<PatchFiles | null>(null);
  const [sourceHash, setSourceHash] = useState("");
  const [targetHashes, setTargetHashes] = useState<string[]>([]);
  const [multiTarget, setMultiTarget] = useState(false);
  const [noPadding, setNoPadding] = useState(false);
  const [partialRemap, setPartialRemap] = useState(false);
  const [busy, setBusy] = useState(false);
  const [errorText, setErrorText] = useState("");
  const [result, setResult] = useState<MigrationResult | null>(null);
  const [warningOpen, setWarningOpen] = useState(false);
  const [multiConfirmed, setMultiConfirmed] = useState(false);
  const [showAllSources, setShowAllSources] = useState(false);

  const loadPublicMetadataIntoState = useCallback(async () => {
    const loaded = await loadPublicMetadata();
    setMetadata(loaded);
    setTargets(await listTargets(loaded.json));
    return loaded;
  }, []);

  const selectedTargetCount = targetHashes.length;
  const canRun = Boolean(metadata?.targetCount && patch && sourceHash && selectedTargetCount);

  const sourceChoices = useMemo(
    () => (patch ? sourceChoicesForSelection(targets, sourceHash, showAllSources) : []),
    [patch, showAllSources, sourceHash, targets],
  );

  const targetOptions = useMemo(
    () => targets.filter((target) => target.hash !== sourceHash),
    [sourceHash, targets],
  );

  const importMetadata = useCallback(async (files: FileList | null) => {
    const file = files?.[0];
    if (!file) {
      return;
    }
    await runTask(setBusy, setErrorText, async () => {
      const loaded = await loadMetadataFile(file);
      setMetadata(loaded);
      setTargets(await listTargets(loaded.json));
      resetMigrationSelection(setSourceHash, setTargetHashes, setResult);
    });
  }, []);

  const loadDirectoryMetadata = useCallback(async () => {
    await runTask(setBusy, setErrorText, async () => {
      const currentMetadata = metadata ?? (await loadPublicMetadataIntoState());
      const options = await fallbackTargetOptions(currentMetadata);
      const loaded = await buildMetadataFromDirectory(options);
      setMetadata(loaded);
      setTargets(await listTargets(loaded.json));
      resetMigrationSelection(setSourceHash, setTargetHashes, setResult);
    });
  }, [loadPublicMetadataIntoState, metadata]);

  const importPatch = useCallback(
    async (files: FileList | null) => {
      if (!files) {
        return;
      }
      await runTask(setBusy, setErrorText, async () => {
        const nextPatch = await patchFilesFromList(files);
        setPatch(nextPatch);
        setResult(null);
        setSourceHash("");
        setTargetHashes([]);
        setShowAllSources(false);
        const currentMetadata = metadata ?? (await loadPublicMetadataIntoState());
        await detectPatchSource({
          metadata: currentMetadata,
          patch: nextPatch,
          setSourceHash,
          setShowAllSources,
        });
      });
    },
    [loadPublicMetadataIntoState, metadata],
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
    await runTask(setBusy, setErrorText, async () => {
      const currentMetadata = metadata ?? (await loadPublicMetadataIntoState());
      const output = await migrate(currentMetadata.json, patch, {
        sourceHash,
        targetHashes,
        patchSuffix,
        noPadding,
        experimentalPartialRemap: partialRemap,
      });
      setResult(output);
    });
  }, [loadPublicMetadataIntoState, metadata, multiConfirmed, multiTarget, noPadding, partialRemap, patch, sourceHash, targetHashes]);

  return (
    <div className="min-h-screen">
      <header className="sticky top-0 z-[2] flex flex-col items-start gap-4 border-b border-slate-200/85 bg-white/85 px-5 py-4 backdrop-blur-[14px] min-[820px]:flex-row min-[820px]:items-center min-[820px]:px-8">
        <div className="flex min-w-0 flex-row items-center gap-3">
          <div className="flex h-7 w-7 items-center justify-center rounded-lg bg-blue-600 text-white shadow-[0_8px_18px_rgb(37_99_235_/_0.18)] [&_svg]:text-[18px]">
            <ElectricBoltIcon />
          </div>
          <div>
            <h1 className="m-0 text-base font-bold leading-tight text-slate-900">HD2 外观 Mod 迁移工具</h1>
            <p className="m-0 text-xs text-slate-500">导入元数据 JSON 并执行迁移</p>
          </div>
        </div>
        <div className="hidden flex-1 min-[820px]:block" />
        <div className="flex flex-row items-center gap-3 max-[819px]:w-full">
          {busy && <CircularProgress size={22} />}
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
          <MetadataPanel
            busy={busy}
            metadata={metadata}
            onDirectoryMetadata={loadDirectoryMetadata}
            onMetadataFile={importMetadata}
          />
          <PatchPanel patch={patch} onPatchFiles={importPatch} />
        </div>
        <TargetPanel
          multiTarget={multiTarget}
          noPadding={noPadding}
          onMultiTargetChange={toggleMultiTarget}
          onSourceChange={chooseSource}
          onTargetChange={chooseTarget}
          partialRemap={partialRemap}
          patchSuffix={patchSuffix}
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
            <p className="m-0 text-xs font-bold">就绪</p>
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

function sourceChoicesForSelection(targets: TargetOption[], sourceHash: string, showAllSources: boolean) {
  if (showAllSources) {
    return targets;
  }
  const selectedSource = targets.find((target) => target.hash === sourceHash);
  return selectedSource ? [selectedSource] : [];
}

interface DetectPatchSourceRequest {
  metadata: MetadataState | null;
  patch: PatchFiles;
  setSourceHash: (hash: string) => void;
  setShowAllSources: (show: boolean) => void;
}

async function detectPatchSource(request: DetectPatchSourceRequest) {
  const { metadata, patch, setSourceHash, setShowAllSources } = request;
  if (!metadata || metadata.targetCount === 0) {
    setShowAllSources(true);
    return;
  }
  const source = await detectSource(metadata.json, patch);
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

function resetMigrationSelection(
  setSourceHash: (value: string) => void,
  setTargetHashes: (value: string[]) => void,
  setResult: (value: MigrationResult | null) => void,
) {
  setSourceHash("");
  setTargetHashes([]);
  setResult(null);
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
