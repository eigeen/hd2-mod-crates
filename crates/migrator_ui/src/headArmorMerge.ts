import type { DetectedSource, EquipmentCategory, EquipmentOption } from "./types";

/** Flatten and de-duplicate detected choices for one head-merge input role. */
export function headMergeCandidates(
  sources: Pick<DetectedSource, "candidates">[],
  category: EquipmentCategory,
): EquipmentOption[] {
  const matching = sources
    .flatMap((source) => source.candidates)
    .filter((option) => option.category === category);
  return [...new Map(matching.map((option) => [option.hash, option])).values()];
}

/** Auto-select only when detection found exactly one candidate. */
export function uniqueHeadMergeHash(options: EquipmentOption[]): string {
  return options.length === 1 ? options[0].hash : "";
}
