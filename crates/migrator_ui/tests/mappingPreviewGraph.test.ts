import { describe, expect, test } from "bun:test";
import { analyzeMappingGraph } from "../src/mappingPreviewGraph";
import { emptyUnitBehavior, setMappingsEnabled, setPreferredConflictSource } from "../src/unitBehavior";
import type { EquipmentMappingPreview } from "../src/types";

describe("combined mapping graph analysis", () => {
  test("detects different source equipments claiming one target Unit", () => {
    const previews = [
      preview("source-a", "target", "unit:a", "unit:target"),
      preview("source-b", "target", "unit:b", "unit:target"),
    ];

    const conflict = analyzeMappingGraph(previews, emptyUnitBehavior())
      .conflictsByTarget.get("unit:target");

    expect(conflict?.state).toBe("unresolved");
    expect(conflict?.sourceFileIds).toEqual(["unit:a", "unit:b"]);
  });

  test("does not report compatible fan-out from the same source equipment", () => {
    const previews = [
      preview("source", "target-a", "unit:a", "unit:shared"),
      preview("source", "target-b", "unit:b", "unit:shared"),
    ];

    expect(analyzeMappingGraph(previews, emptyUnitBehavior()).conflictsByTarget.size).toBe(0);
  });

  test("recalculates conflicts from behavior instead of assuming a preset", () => {
    const previews = [
      preview("source-a", "target", "unit:a", "unit:target"),
      preview("source-b", "target", "unit:b", "unit:target"),
    ];
    const preferred = setPreferredConflictSource(emptyUnitBehavior(), "unit:target", "unit:b");
    const disabled = setMappingsEnabled(preferred, [{
      sourceFileId: "unit:a",
      targetFileId: "unit:target",
    }], false);

    expect(analyzeMappingGraph(previews, preferred).conflictsByTarget.get("unit:target")?.state)
      .toBe("resolved");
    expect(analyzeMappingGraph(previews, disabled).conflictsByTarget.size).toBe(0);
  });
});

function preview(
  sourceHash: string,
  targetHash: string,
  sourceUnitId: string,
  targetUnitId: string,
): EquipmentMappingPreview {
  return {
    schemaVersion: 1,
    sourceEquipment: equipment(sourceHash),
    targetEquipment: equipment(targetHash),
    units: [sourceUnitId, targetUnitId].map((id) => ({
      id,
      fileId: id,
      presentInPatch: id === sourceUnitId,
      sourceRoles: id === sourceUnitId ? ["slimBody"] : [],
      targetRoles: id === targetUnitId ? ["slimBody"] : [],
    })),
    mappings: [{
      id: `mapping:${sourceHash}:${targetHash}`,
      sourceUnitId,
      targetUnitId,
      role: "slimBody",
      action: sourceUnitId === targetUnitId ? "reuse" : "replace",
    }],
    summary: {
      mappedUnitCount: 1,
      replacedUnitCount: sourceUnitId === targetUnitId ? 0 : 1,
      unchangedUnitCount: sourceUnitId === targetUnitId ? 1 : 0,
      reusedSourceUnitCount: 0,
    },
  };
}

function equipment(hash: string) {
  return { id: `equipment:${hash}`, category: "Armor" as const, hash, name: hash };
}
