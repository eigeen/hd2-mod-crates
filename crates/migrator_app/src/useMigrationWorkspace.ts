import { useCallback, useMemo, useState } from "react";
import {
  buildMigrationVariants,
  configuredMappings as collectConfiguredMappings,
  emptyUnitBehavior,
  multiTargetEligible as canUseMultiTarget,
  selectTarget,
  singlePatchRequired as mustUseSinglePatch,
  targetsForSource,
  type DetectedSource,
  type EquipmentOption,
  type EquipmentPartGraph,
  type MissingUnitPolicy,
  type UnitBehaviorOptions,
  type UnmatchedUnitPolicy,
} from "@hd2-mod-tools/migrator-ui";
import type { PatchAnalysis } from "./platform";

export function useMigrationWorkspace(equipmentOptions: EquipmentOption[]) {
  const [equipmentGraph, setEquipmentGraph] = useState<EquipmentPartGraph | null>(null);
  const [sources, setSources] = useState<DetectedSource[]>([]);
  const [activeSourceId, setActiveSourceId] = useState("");
  const [targetsBySource, setTargetsBySource] = useState<Record<string, string[]>>({});
  const [multiTarget, setMultiTarget] = useState(false);
  const [singlePatch, setSinglePatch] = useState(false);
  const [noPadding, setNoPadding] = useState(false);
  const [unmatchedUnitPolicy, setUnmatchedUnitPolicy] = useState<UnmatchedUnitPolicy>("keep");
  const [unitBehavior, setUnitBehavior] = useState<UnitBehaviorOptions>(emptyUnitBehavior);
  const [missingUnitPolicy, setMissingUnitPolicy] = useState<MissingUnitPolicy>("drop");

  const activeSource = sources.find((source) => source.id === activeSourceId) ?? null;
  const activeTargets = activeSource ? targetsBySource[activeSource.id] ?? [] : [];
  const configuredMappings = useMemo(
    () => collectConfiguredMappings(sources, targetsBySource),
    [sources, targetsBySource],
  );
  const targetOptions = useMemo(
    () => targetsForSource(activeSource, equipmentOptions),
    [activeSource, equipmentOptions],
  );
  const multiTargetEligible = canUseMultiTarget(sources);
  const singlePatchRequired = mustUseSinglePatch(configuredMappings);
  const outputAsSinglePatch = singlePatch || singlePatchRequired;
  const variants = useMemo(
    () => buildMigrationVariants(configuredMappings, outputAsSinglePatch),
    [configuredMappings, outputAsSinglePatch],
  );

  const applyAnalysis = useCallback((analysis: PatchAnalysis) => {
    setEquipmentGraph(analysis.equipmentGraph);
    setSources(analysis.inspection.sources);
    setActiveSourceId(analysis.inspection.sources[0]?.id ?? "");
    setTargetsBySource({});
    setMultiTarget(canUseMultiTarget(analysis.inspection.sources));
    setSinglePatch(false);
    setUnitBehavior(emptyUnitBehavior());
  }, []);

  const clearPatchAnalysis = useCallback(() => {
    setEquipmentGraph(null);
    setSources([]);
    setActiveSourceId("");
    setTargetsBySource({});
  }, []);

  const resolveSource = useCallback((sourceId: string, hash: string) => {
    setSources((current) => {
      const resolved = current.map((source) => (
        source.id === sourceId ? { ...source, resolvedHash: hash } : source
      ));
      setMultiTarget(canUseMultiTarget(resolved));
      return resolved;
    });
    setTargetsBySource((current) => ({ ...current, [sourceId]: [] }));
  }, []);

  const replaceActiveTargets = useCallback((targets: string[]) => {
    if (!activeSource) return;
    setTargetsBySource((current) => ({ ...current, [activeSource.id]: targets }));
  }, [activeSource]);

  const chooseTarget = useCallback((hash: string) => {
    replaceActiveTargets(selectTarget(activeTargets, hash, multiTarget));
  }, [activeTargets, multiTarget, replaceActiveTargets]);

  const disableMultiTarget = useCallback(() => {
    setMultiTarget(false);
    setTargetsBySource((current) => Object.fromEntries(
      Object.entries(current).map(([sourceId, targets]) => [sourceId, targets.slice(0, 1)]),
    ));
  }, []);

  return {
    activeSource,
    activeSourceId,
    activeTargets,
    applyAnalysis,
    chooseTarget,
    clearPatchAnalysis,
    configuredMappings,
    disableMultiTarget,
    equipmentGraph,
    missingUnitPolicy,
    multiTarget,
    multiTargetEligible,
    noPadding,
    outputAsSinglePatch,
    replaceActiveTargets,
    resolveSource,
    setActiveSourceId,
    setMissingUnitPolicy,
    setMultiTarget,
    setNoPadding,
    setSinglePatch,
    setUnitBehavior,
    setUnmatchedUnitPolicy,
    singlePatch,
    singlePatchRequired,
    sources,
    targetOptions,
    targetsBySource,
    unitBehavior,
    unmatchedUnitPolicy,
    variants,
  };
}
