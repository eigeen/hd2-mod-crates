import { describe, expect, test } from "bun:test";
import { compileMigrationRequest, MIGRATION_PATCH_SUFFIX } from "../src/migrationRequest";

const emptyUnitBehavior = { disabledMappings: [], exportOverrides: [], conflictResolutions: [] };

describe("migration request compiler", () => {
  test("produces the same engine options, output name, and source-to-target progress label", () => {
    const compiled = compileMigrationRequest({
      equipmentOptions: [
        { category: "Armor", excluded: false, hash: "source", name: "Original Armor" },
        { category: "Armor", excluded: false, hash: "target", name: "Target Armor" },
      ],
      noPadding: true,
      patch: {
        byteLength: 10,
        info: { name: "fallback" },
        key: "patch",
        originalName: "my:mod.zip",
        source: "opaque-source",
      },
      unitBehavior: emptyUnitBehavior,
      unmatchedUnitPolicy: "keep",
      variants: [{ mappings: [{ category: "Armor", sourceHash: "source", targetHash: "target" }] }],
    });

    expect(compiled.options.patchSuffix).toBe(MIGRATION_PATCH_SUFFIX);
    expect(compiled.options.noPadding).toBe(true);
    expect(compiled.options.unmatchedUnitPolicy).toBe("keep");
    expect(compiled.progressLabels).toEqual(["Original Armor → Target Armor"]);
    expect(compiled.suggestedOutputFilename).toBe("my_mod_Target Armor.zip");
  });

  test("uses the stable combined-output name for more than one mapping", () => {
    const mappings = ["one", "two"].map((targetHash) => ({
      category: "Armor" as const,
      sourceHash: "source",
      targetHash,
    }));
    const compiled = compileMigrationRequest({
      equipmentOptions: [],
      noPadding: false,
      patch: { byteLength: 1, info: { name: "patch" }, key: "patch", source: null },
      unitBehavior: emptyUnitBehavior,
      unmatchedUnitPolicy: "drop",
      variants: [{ mappings }],
    });
    expect(compiled.suggestedOutputFilename).toBe("hd2-migrated-patch.zip");
  });
});
