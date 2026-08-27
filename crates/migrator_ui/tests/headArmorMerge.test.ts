import { describe, expect, test } from "bun:test";
import {
  combineEquipmentChoices,
  headMergeCandidates,
  headMergeVariantLabel,
  uniqueHeadMergeHash,
} from "../src/headArmorMerge";
import type { EquipmentOption } from "../src/types";

const armor = option("Armor", "a", "Armor A");
const armor2 = option("Armor", "a2", "Armor B");
const helmet = option("Helmet", "h", "Helmet H");
const helmet2 = option("Helmet", "h2", "Helmet I");

describe("head merge candidate selection", () => {
  test("filters by input role and de-duplicates candidates", () => {
    const result = headMergeCandidates([
      { candidates: [armor, helmet] },
      { candidates: [armor] },
    ], "Armor");
    expect(result).toEqual([armor]);
  });

  test("auto-selects only a unique result", () => {
    expect(uniqueHeadMergeHash([helmet])).toBe("h");
    expect(uniqueHeadMergeHash([])).toBe("");
    expect(uniqueHeadMergeHash([helmet, option("Helmet", "h2", "Other")])).toBe("");
  });
});

describe("combine equipment choices", () => {
  test("de-duplicates across Mods and tags the first source", () => {
    const result = combineEquipmentChoices([
      { sourceName: "mod-a", armors: [armor, armor2], helmets: [helmet] },
      { sourceName: "mod-b", armors: [armor], helmets: [helmet, helmet2] },
    ]);
    expect(result.armors).toEqual([
      { ...armor, sourceName: "mod-a" },
      { ...armor2, sourceName: "mod-a" },
    ]);
    expect(result.helmets).toEqual([
      { ...helmet, sourceName: "mod-a" },
      { ...helmet2, sourceName: "mod-b" },
    ]);
  });

  test("keeps categories separate even with equal hashes", () => {
    const result = combineEquipmentChoices([
      { sourceName: "mod-a", armors: [armor], helmets: [armor] },
    ]);
    expect(result.armors).toEqual([{ ...armor, sourceName: "mod-a" }]);
    expect(result.helmets).toEqual([{ ...armor, sourceName: "mod-a" }]);
  });

  test("returns empty lists for empty input", () => {
    expect(combineEquipmentChoices([])).toEqual({ armors: [], helmets: [] });
  });
});

describe("head merge variant label", () => {
  const t = (key: string) => ({ "graph.role.slimBody": "SLIM", "graph.role.stockyBody": "STOCKY" })[key] ?? key;

  test("translates the built-in body variants", () => {
    expect(headMergeVariantLabel("slim body", t)).toBe("SLIM");
    expect(headMergeVariantLabel("stocky body", t)).toBe("STOCKY");
  });

  test("keeps unknown variants as-is", () => {
    expect(headMergeVariantLabel("weird part", t)).toBe("weird part");
  });
});

function option(category: "Armor" | "Helmet", hash: string, name: string): EquipmentOption {
  return { category, hash, name, excluded: false };
}
