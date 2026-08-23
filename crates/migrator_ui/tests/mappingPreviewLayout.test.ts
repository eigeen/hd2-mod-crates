import { describe, expect, test } from "bun:test";
import {
  layoutMappingPreview,
  layoutMappingPreviews,
  layoutPatchEquipmentGraph,
} from "../src/mappingPreviewLayout";
import type { EquipmentMappingPreview, EquipmentPartGraph } from "../src/types";

describe("mapping preview layout", () => {
  test("reuses a canonical target Unit that already exists in the source", () => {
    const layout = layoutMappingPreview(preview([
      mapping("a-to-b", "unit:a", "unit:b", "slimBody", "replace"),
      mapping("b-to-c", "unit:b", "unit:c", "stockyBody", "replace"),
    ]));

    expect(layout.nodes.filter((node) => node.id === "unit:b")).toHaveLength(1);
    expect(uniquePositionCount(layout.nodes)).toBe(layout.nodes.length);
    expect(layout.edges.some((edge) => edge.source === "unit:a" && edge.target === "unit:b")).toBeTrue();
    expect(layout.edges.some((edge) => edge.source === "unit:b" && edge.target === "equipment:target")).toBeTrue();
    expect(layout.edges.every((edge) => edge.type === "default")).toBeTrue();
  });

  test("unchanged FileID connects directly without a self-remap edge", () => {
    const layout = layoutMappingPreview(preview([
      mapping("a-to-a", "unit:a", "unit:a", "helmet", "reuse"),
    ]));

    expect(layout.nodes.filter((node) => node.id === "unit:a")).toHaveLength(1);
    expect(layout.edges.some((edge) => edge.source === "unit:a" && edge.target === "unit:a")).toBeFalse();
    expect(layout.edges.some((edge) => edge.source === "equipment:source" && edge.target === "unit:a")).toBeTrue();
    expect(layout.edges.some((edge) => edge.source === "unit:a" && edge.target === "equipment:target")).toBeTrue();
    expect(uniquePositionCount(layout.nodes)).toBe(layout.nodes.length);
  });

  test("renders every replacement object in one canonical graph", () => {
    const first = preview([
      mapping("a-to-shared", "unit:a", "unit:shared", "slimBody", "replace"),
    ]);
    const second = {
      ...preview([
        mapping("b-to-shared", "unit:b", "unit:shared", "stockyBody", "replace"),
      ]),
      sourceEquipment: { id: "equipment:source-b", category: "Armor" as const, hash: "source-b", name: "Source B" },
      targetEquipment: { id: "equipment:target-b", category: "Armor" as const, hash: "target-b", name: "Target B" },
    };
    const layout = layoutMappingPreviews([first, second]);

    expect(layout.nodes.filter((node) => node.id === "unit:shared")).toHaveLength(1);
    expect(layout.nodes.filter((node) => node.data.kind === "equipment")).toHaveLength(4);
    expect(layout.edges.some((edge) => edge.source === "unit:a" && edge.target === "unit:shared")).toBeTrue();
    expect(layout.edges.some((edge) => edge.source === "unit:b" && edge.target === "unit:shared")).toBeTrue();
    expect(layout.nodes.find((node) => node.id === "unit:shared")?.height).toBe(106);
    expect(layout.nodes.find((node) => node.id === "unit:a")?.height).toBe(72);
  });

  test("sizes detailed Unit nodes from their stable content rows", () => {
    const layout = layoutMappingPreview(preview([
      mapping("a-to-b", "unit:a", "unit:b", "slimBody", "replace"),
      mapping("b-to-c", "unit:b", "unit:c", "stockyBody", "replace"),
    ]));

    expect(layout.nodes.find((node) => node.id === "unit:a")?.height).toBe(72);
    expect(layout.nodes.find((node) => node.id === "unit:b")?.height).toBe(106);
    expect(layout.nodes.find((node) => node.id === "unit:c")?.height).toBe(72);
  });

  test("shows recognized relationships and groups wild Units without a target mapping", () => {
    const layout = layoutPatchEquipmentGraph(patchGraph());

    expect(layout.edges.some((edge) => edge.source === "equipment:source" && edge.target === "unit:known")).toBeTrue();
    expect(layout.nodes.some((node) => node.id === "unit:wild")).toBeTrue();
    expect(layout.edges.some((edge) => edge.source === "wild:Patch" || edge.target === "wild:Patch")).toBeFalse();
    expect(layout.edges.every((edge) => edge.type === "default")).toBeTrue();
    expect(uniquePositionCount(layout.nodes)).toBe(layout.nodes.length);
  });

  test("stacks recognized Patch Units in one vertical rank", () => {
    const layout = layoutPatchEquipmentGraph(densePatchGraph());
    const recognizedUnits = layout.nodes.filter((node) => (
      node.data.kind === "unit" && node.data.sourceRoles.length > 0
    ));

    expect(new Set(recognizedUnits.map((node) => node.position.x)).size).toBe(1);
    expect(new Set(recognizedUnits.map((node) => node.position.y)).size).toBe(recognizedUnits.length);
  });

  test("keeps a real-sized Patch graph compact enough to fit the preview", () => {
    const layout = layoutPatchEquipmentGraph(densePatchGraph());
    const bounds = layoutBounds(layout.nodes);

    expect(layout.nodes).toHaveLength(23);
    expect(uniquePositionCount(layout.nodes)).toBe(layout.nodes.length);
    expect(bounds.width).toBeLessThanOrEqual(1_300);
    expect(bounds.height).toBeLessThanOrEqual(1_400);
  });
});

function uniquePositionCount(nodes: ReturnType<typeof layoutMappingPreview>["nodes"]): number {
  return new Set(nodes.map((node) => `${node.position.x}:${node.position.y}`)).size;
}

function layoutBounds(nodes: ReturnType<typeof layoutMappingPreview>["nodes"]): { width: number; height: number } {
  const right = Math.max(...nodes.map((node) => node.position.x + (node.width ?? 0)));
  const bottom = Math.max(...nodes.map((node) => node.position.y + (node.height ?? 0)));
  return { width: right, height: bottom };
}

function densePatchGraph(): EquipmentPartGraph {
  const equipments = Array.from({ length: 6 }, (_, index) => ({
    id: `equipment:${index}`,
    category: "Armor" as const,
    hash: `hash:${index}`,
    name: `Equipment ${index}`,
  }));
  const components = Array.from({ length: 16 }, (_, index) => ({
    id: `unit:${index}`,
    fileId: `0x${index.toString(16).padStart(16, "0")}`,
    kind: "unit" as const,
    presentInPatch: true,
  }));
  const relations = components.slice(0, 7).map((component, index) => ({
    id: `relation:${index}`,
    equipmentId: equipments[index % equipments.length].id,
    componentId: component.id,
    role: "slimBody" as const,
  }));
  const diagnostics = components.slice(7).map((component) => ({
    code: "unmappedUnit" as const,
    componentId: component.id,
    fileId: component.fileId,
  }));
  return {
    schemaVersion: 1,
    patch: { name: "Dense Patch", unitCount: 16, mappedUnitCount: 7, unmappedUnitCount: 9, equipmentCount: 6, relationCount: 7 },
    equipments,
    components,
    relations,
    diagnostics,
  };
}

function patchGraph(): EquipmentPartGraph {
  return {
    schemaVersion: 1,
    patch: {
      name: "Patch",
      unitCount: 2,
      mappedUnitCount: 1,
      unmappedUnitCount: 1,
      equipmentCount: 1,
      relationCount: 1,
    },
    equipments: [{ id: "equipment:source", category: "Armor", hash: "source", name: "Source" }],
    components: [
      { id: "unit:known", fileId: "known", kind: "unit", presentInPatch: true },
      { id: "unit:wild", fileId: "wild", kind: "unit", presentInPatch: true },
    ],
    relations: [{ id: "owns:known", equipmentId: "equipment:source", componentId: "unit:known", role: "slimBody" }],
    diagnostics: [{ code: "unmappedUnit", componentId: "unit:wild", fileId: "wild" }],
  };
}

function preview(mappings: EquipmentMappingPreview["mappings"]): EquipmentMappingPreview {
  const ids = [...new Set(mappings.flatMap((mapping) => [mapping.sourceUnitId, mapping.targetUnitId]))];
  return {
    schemaVersion: 1,
    sourceEquipment: { id: "equipment:source", category: "Armor", hash: "source", name: "Source" },
    targetEquipment: { id: "equipment:target", category: "Armor", hash: "target", name: "Target" },
    units: ids.map((id) => ({
      id,
      fileId: id,
      presentInPatch: true,
      sourceRoles: mappings.filter((mapping) => mapping.sourceUnitId === id).map((mapping) => mapping.role),
      targetRoles: mappings.filter((mapping) => mapping.targetUnitId === id).map((mapping) => mapping.role),
    })),
    mappings,
    summary: {
      mappedUnitCount: mappings.length,
      replacedUnitCount: mappings.filter((mapping) => mapping.action === "replace").length,
      unchangedUnitCount: mappings.filter((mapping) => mapping.action === "reuse").length,
      reusedSourceUnitCount: 0,
    },
  };
}

function mapping(
  id: string,
  sourceUnitId: string,
  targetUnitId: string,
  role: EquipmentMappingPreview["mappings"][number]["role"],
  action: EquipmentMappingPreview["mappings"][number]["action"],
) {
  return { id, sourceUnitId, targetUnitId, role, action };
}
