import { describe, expect, test } from "bun:test";
import {
  buildMigrationVariants,
  configuredMappings,
  multiTargetEligible,
  selectTarget,
  targetsForSource,
} from "../src/migrationMappings";
import type { DetectedSource, EquipmentOption } from "../src/types";

const armor = option("Armor", "armor-source", "FS-55 Devastator");
const armorTarget = option("Armor", "armor-target", "XX-66 Armor");
const helmet = option("Helmet", "helmet-source", "FS-55 Devastator");
const helmetTarget = option("Helmet", "helmet-target", "XX-66 Helmet");

describe("unified migration mapping state", () => {
  test("filters targets to the active source category and excludes the source", () => {
    expect(targetsForSource(source("armor", armor), [armor, armorTarget, helmetTarget]))
      .toEqual([armorTarget]);
  });

  test("allows partial mappings and combines mixed equipment into one variant", () => {
    const sources = [source("armor", armor), source("helmet", helmet)];
    const mappings = configuredMappings(sources, { armor: [armorTarget.hash] });

    expect(mappings).toEqual([{
      category: "Armor",
      sourceHash: armor.hash,
      targetHash: armorTarget.hash,
    }]);
    expect(buildMigrationVariants(mappings, false)).toEqual([{ mappings }]);
  });

  test("cancels a single target by selecting it again", () => {
    expect(selectTarget([armorTarget.hash], armorTarget.hash, false)).toEqual([]);
    expect(selectTarget([], armorTarget.hash, false)).toEqual([armorTarget.hash]);
  });

  test("expands one source with multiple targets into independent variants", () => {
    const mappings = configuredMappings([source("helmet", helmet)], {
      helmet: [helmetTarget.hash, "helmet-target-2"],
    });
    expect(buildMigrationVariants(mappings, true).map((variant) => variant.mappings.length))
      .toEqual([1, 1]);
  });

  test("enables multi-target only for one resolved source", () => {
    expect(multiTargetEligible([source("armor", armor)])).toBe(true);
    expect(multiTargetEligible([source("armor", armor), source("helmet", helmet)])).toBe(false);
    expect(multiTargetEligible([{ ...source("armor", armor), resolvedHash: null }])).toBe(false);
  });
});

function option(category: EquipmentOption["category"], hash: string, name: string): EquipmentOption {
  return { category, excluded: false, hash, name };
}

function source(id: string, candidate: EquipmentOption): DetectedSource {
  return {
    id,
    category: candidate.category,
    unitHits: 1,
    candidates: [candidate],
    resolvedHash: candidate.hash,
  };
}
