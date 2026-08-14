import type {
  DetectedSource,
  EquipmentOption,
  MigrationMapping,
  MigrationVariant,
} from "./types";

export const MAX_WEB_MAPPINGS_PER_RUN = 10;

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
  singlePatch = false,
): MigrationVariant[] {
  if (singlePatch) {
    return mappings.length ? [{ mappings }] : [];
  }
  const mappingGroups = mappingsBySource(mappings);
  const combinations = mappingGroups.reduce<MigrationMapping[][]>(
    (variants, group) => variants.flatMap((variant) => (
      group.map((mapping) => [...variant, mapping])
    )),
    [[]],
  );
  return combinations
    .filter((variant) => variant.length > 0)
    .map((variant) => ({ mappings: variant }));
}

/** Multiple configured sources must share one output to avoid Cartesian expansion. */
export function singlePatchRequired(mappings: MigrationMapping[]): boolean {
  return mappingsBySource(mappings).length > 1;
}

/** Keep every web run bounded; callers may lower the limit for single-Patch jobs. */
export function exceedsWebMappingLimit(
  mappings: MigrationMapping[],
  limit = MAX_WEB_MAPPINGS_PER_RUN,
): boolean {
  return mappings.length > limit;
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
  return sources.some((source) => Boolean(source.resolvedHash));
}

export function selectTarget(values: string[], hash: string, multiTarget: boolean): string[] {
  if (!multiTarget) {
    return values.includes(hash) ? [] : [hash];
  }
  return values.includes(hash) ? values.filter((value) => value !== hash) : [...values, hash];
}

function mappingsBySource(mappings: MigrationMapping[]): MigrationMapping[][] {
  const groups = new Map<string, MigrationMapping[]>();
  for (const mapping of mappings) {
    const key = `${mapping.category}:${mapping.sourceHash}`;
    const group = groups.get(key) ?? [];
    group.push(mapping);
    groups.set(key, group);
  }
  return Array.from(groups.values());
}
