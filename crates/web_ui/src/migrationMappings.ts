import type {
  DetectedSource,
  EquipmentOption,
  MigrationMapping,
  MigrationVariant,
} from "./types";

export const MAX_WEB_SEPARATE_PATCH_OUTPUTS = 20;
export const MAX_WEB_SINGLE_PATCH_MAPPINGS = 20;
const GIB = 1024 * 1024 * 1024;
export const MAX_WEB_PROJECTED_OUTPUT_BYTES = 2 * GIB;

/** Bound independent outputs by count and projected uncompressed ZIP size. */
export function maxWebSeparateOutputsForPatch(patchByteLength: number): number {
  if (patchByteLength <= 0) return MAX_WEB_SEPARATE_PATCH_OUTPUTS;
  const sizeLimit = Math.floor(MAX_WEB_PROJECTED_OUTPUT_BYTES / patchByteLength);
  return Math.max(1, Math.min(MAX_WEB_SEPARATE_PATCH_OUTPUTS, sizeLimit));
}

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

/** Keep every web run within its output mode's explicit mapping limit. */
export function exceedsWebMappingLimit(
  mappings: MigrationMapping[],
  limit: number,
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
