import { describe, expect, test } from "bun:test";
import {
  buildMigrationVariants,
  configuredMappings,
  exceedsWebMappingLimit,
  MAX_WEB_SEPARATE_PATCH_OUTPUTS,
  MAX_WEB_SINGLE_PATCH_MAPPINGS,
  maxWebSeparateOutputsForPatch,
  multiTargetEligible,
  selectTarget,
  singlePatchRequired,
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
    expect(buildMigrationVariants(mappings)).toEqual([{ mappings }]);
  });

  test("cancels a single target by selecting it again", () => {
    expect(selectTarget([armorTarget.hash], armorTarget.hash, false)).toEqual([]);
    expect(selectTarget([], armorTarget.hash, false)).toEqual([armorTarget.hash]);
  });

  test("expands one source with multiple targets into independent variants", () => {
    const mappings = configuredMappings([source("helmet", helmet)], {
      helmet: [helmetTarget.hash, "helmet-target-2"],
    });
    expect(buildMigrationVariants(mappings).map((variant) => variant.mappings.length))
      .toEqual([1, 1]);
  });

  test("bundles all targets from one source into one variant when requested", () => {
    const mappings = configuredMappings([source("helmet", helmet)], {
      helmet: [helmetTarget.hash, "helmet-target-2"],
    });

    expect(buildMigrationVariants(mappings, true)).toEqual([{ mappings }]);
  });

  test("expands multiple sources into the cartesian product of their targets", () => {
    const mappings = configuredMappings([source("armor", armor), source("helmet", helmet)], {
      armor: [armorTarget.hash, "armor-target-2"],
      helmet: [helmetTarget.hash, "helmet-target-2", "helmet-target-3"],
    });
    const variants = buildMigrationVariants(mappings);

    expect(variants).toHaveLength(6);
    expect(variants.every((variant) => variant.mappings.length === 2)).toBeTrue();
    expect(variants.map((variant) => variant.mappings.map((mapping) => mapping.targetHash)))
      .toEqual([
        [armorTarget.hash, helmetTarget.hash],
        [armorTarget.hash, "helmet-target-2"],
        [armorTarget.hash, "helmet-target-3"],
        ["armor-target-2", helmetTarget.hash],
        ["armor-target-2", "helmet-target-2"],
        ["armor-target-2", "helmet-target-3"],
      ]);
  });

  test("requires one Patch output when multiple sources have mappings", () => {
    const oneSource = configuredMappings([source("armor", armor), source("helmet", helmet)], {
      armor: [armorTarget.hash, "armor-target-2"],
    });
    const multipleSources = configuredMappings(
      [source("armor", armor), source("helmet", helmet)],
      { armor: [armorTarget.hash], helmet: [helmetTarget.hash, "helmet-target-2"] },
    );

    expect(singlePatchRequired(oneSource)).toBe(false);
    expect(singlePatchRequired(multipleSources)).toBe(true);
    expect(buildMigrationVariants(multipleSources, true)).toEqual([{
      mappings: multipleSources,
    }]);
  });

  test("limits every migration job in the web build", () => {
    const mappings = Array.from(
      { length: MAX_WEB_SINGLE_PATCH_MAPPINGS + 1 },
      (_, index) => ({
        category: "Armor" as const,
        sourceHash: armor.hash,
        targetHash: `armor-target-${index}`,
      }),
    );

    expect(exceedsWebMappingLimit(
      mappings.slice(0, MAX_WEB_SINGLE_PATCH_MAPPINGS),
      MAX_WEB_SINGLE_PATCH_MAPPINGS,
    )).toBe(false);
    expect(exceedsWebMappingLimit(mappings, MAX_WEB_SINGLE_PATCH_MAPPINGS)).toBe(true);
    expect(exceedsWebMappingLimit(mappings.slice(0, 6), 5)).toBe(true);
  });

  test("raises the measured 151 MiB fixture from six to thirteen independent outputs", () => {
    expect(maxWebSeparateOutputsForPatch(158_659_188)).toBe(13);
    expect(maxWebSeparateOutputsForPatch(50 * 1024 * 1024))
      .toBe(MAX_WEB_SEPARATE_PATCH_OUTPUTS);
  });

  test("enables multi-target when any source is resolved", () => {
    expect(multiTargetEligible([source("armor", armor)])).toBe(true);
    expect(multiTargetEligible([source("armor", armor), source("helmet", helmet)])).toBe(true);
    expect(multiTargetEligible([
      source("armor", armor),
      { ...source("helmet", helmet), resolvedHash: null },
    ])).toBe(true);
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
