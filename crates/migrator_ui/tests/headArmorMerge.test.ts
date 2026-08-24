import { describe, expect, test } from "bun:test";
import { headMergeCandidates, uniqueHeadMergeHash } from "../src/headArmorMerge";
import type { EquipmentOption } from "../src/types";

const armor = option("Armor", "a", "Armor A");
const helmet = option("Helmet", "h", "Helmet H");

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

function option(category: "Armor" | "Helmet", hash: string, name: string): EquipmentOption {
  return { category, hash, name, excluded: false };
}
