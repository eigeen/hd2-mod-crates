import type { DetectedSource, EquipmentCategory, EquipmentOption } from "./types";
import type { Translate } from "./i18n";

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

/** Equipment detected inside one imported Mod. */
export interface EquipmentChoices {
  sourceName: string;
  armors: EquipmentOption[];
  helmets: EquipmentOption[];
}

/** An equipment option with the Mod it was first detected in. */
export interface LabelledEquipmentOption extends EquipmentOption {
  sourceName: string;
}

export interface CombinedEquipmentChoices {
  armors: LabelledEquipmentOption[];
  helmets: LabelledEquipmentOption[];
}

/**
 * Merge choices from several Mods into two de-duplicated lists.
 * The same equipment detected in multiple Mods appears once, tagged with the
 * first Mod that contains it.
 */
export function combineEquipmentChoices(parsed: EquipmentChoices[]): CombinedEquipmentChoices {
  return {
    armors: combineCategory(parsed, "Armor"),
    helmets: combineCategory(parsed, "Helmet"),
  };
}

function combineCategory(
  parsed: EquipmentChoices[],
  category: EquipmentCategory,
): LabelledEquipmentOption[] {
  const seen = new Set<string>();
  const result: LabelledEquipmentOption[] = [];
  for (const source of parsed) {
    const options = category === "Armor" ? source.armors : source.helmets;
    for (const option of options) {
      if (seen.has(option.hash)) continue;
      seen.add(option.hash);
      result.push({ ...option, sourceName: source.sourceName });
    }
  }
  return result;
}

/** User-facing name for a body variant reported by the merge result. */
export function headMergeVariantLabel(variant: string, t: Translate): string {
  if (variant === "slim body") return t("graph.role.slimBody");
  if (variant === "stocky body") return t("graph.role.stockyBody");
  return variant;
}
