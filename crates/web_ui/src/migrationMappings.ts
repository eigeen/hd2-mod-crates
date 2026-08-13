import type {
  DetectedSource,
  EquipmentOption,
  MigrationMapping,
  MigrationVariant,
} from "./types";

export function configuredMappings(
  sources: DetectedSource[],
  targetsBySource: Record<string, string[]>,
): MigrationMapping[] {
  return sources.flatMap((source) => {
    if (!source.resolvedHash) return [];
    return (targetsBySource[source.id] ?? []).map((targetHash) => ({
      category: source.category,
      sourceHash: source.resolvedHash!,
      targetHash,
    }));
  });
}

export function buildMigrationVariants(
  mappings: MigrationMapping[],
  singleSource: boolean,
): MigrationVariant[] {
  if (singleSource) {
    return mappings.map((mapping) => ({ mappings: [mapping] }));
  }
  return mappings.length ? [{ mappings }] : [];
}

export function targetsForSource(
  source: DetectedSource | null,
  options: EquipmentOption[],
): EquipmentOption[] {
  if (!source?.resolvedHash) return [];
  return options.filter((target) => (
    target.category === source.category && target.hash !== source.resolvedHash
  ));
}

export function multiTargetEligible(sources: DetectedSource[]): boolean {
  return sources.length === 1 && Boolean(sources[0]?.resolvedHash);
}
