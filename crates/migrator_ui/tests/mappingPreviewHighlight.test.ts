import { describe, expect, test } from "bun:test";
import type { Edge } from "@xyflow/react";
import { collectPreviewHighlight } from "../src/mappingPreviewHighlight";
import type { MappingPreviewNode } from "../src/mappingPreviewLayout";

describe("mapping preview relationship highlight", () => {
  test("highlights shared equipment consumers without expanding through their hubs", () => {
    const nodes = [equipment("equipment:a"), equipment("equipment:b"), equipment("equipment:c"), unit("unit:shared"), unit("unit:other")];
    const edges = [
      edge("a-shared", "equipment:a", "unit:shared"),
      edge("b-shared", "equipment:b", "unit:shared"),
      edge("b-other", "equipment:b", "unit:other"),
      edge("c-other", "equipment:c", "unit:other"),
    ];

    const highlight = collectPreviewHighlight(nodes, edges, "unit:shared");

    expect([...highlight.nodeIds]).toEqual(["unit:shared"]);
    expect([...highlight.edgeIds]).toEqual([]);
    expect([...highlight.reverseNodeIds].sort()).toEqual(["equipment:a", "equipment:b"]);
    expect([...highlight.reverseEdgeIds].sort()).toEqual(["a-shared", "b-shared"]);
  });

  test("keeps an equipment's forward Units primary and other consumers reverse", () => {
    const nodes = [equipment("equipment:a"), equipment("equipment:b"), equipment("equipment:c"), unit("unit:shared"), unit("unit:other")];
    const edges = [
      edge("a-shared", "equipment:a", "unit:shared"),
      edge("b-shared", "equipment:b", "unit:shared"),
      edge("b-other", "equipment:b", "unit:other"),
      edge("c-other", "equipment:c", "unit:other"),
    ];

    const highlight = collectPreviewHighlight(nodes, edges, "equipment:a");

    expect([...highlight.nodeIds].sort()).toEqual(["equipment:a", "unit:shared"]);
    expect([...highlight.edgeIds]).toEqual(["a-shared"]);
    expect([...highlight.reverseNodeIds]).toEqual(["equipment:b"]);
    expect([...highlight.reverseEdgeIds]).toEqual(["b-shared"]);
  });

  test("follows replacement Unit chains to both equipment endpoints", () => {
    const nodes = [equipment("equipment:source"), equipment("equipment:target"), unit("unit:a"), unit("unit:b")];
    const edges = [
      edge("source", "equipment:source", "unit:a"),
      edge("replace", "unit:a", "unit:b"),
      edge("target", "unit:b", "equipment:target"),
    ];

    const highlight = collectPreviewHighlight(nodes, edges, "unit:a");

    expect([...highlight.nodeIds].sort()).toEqual(["equipment:target", "unit:a", "unit:b"]);
    expect([...highlight.edgeIds].sort()).toEqual(["replace", "target"]);
    expect([...highlight.reverseNodeIds]).toEqual(["equipment:source"]);
    expect([...highlight.reverseEdgeIds]).toEqual(["source"]);
  });

  test("isolated wild Units do not imply relationships", () => {
    const nodes = [unit("unit:wild"), group("wild:Patch")];
    const highlight = collectPreviewHighlight(nodes, [], "unit:wild");

    expect([...highlight.nodeIds]).toEqual(["unit:wild"]);
    expect(highlight.edgeIds.size).toBe(0);
    expect(highlight.reverseNodeIds.size).toBe(0);
    expect(highlight.reverseEdgeIds.size).toBe(0);
  });
});

function equipment(id: string): MappingPreviewNode {
  return {
    id,
    type: "previewEquipment",
    position: { x: 0, y: 0 },
    data: { kind: "equipment", side: "source", name: id, category: "Armor" },
  };
}

function unit(id: string): MappingPreviewNode {
  return {
    id,
    type: "previewUnit",
    position: { x: 0, y: 0 },
    data: { kind: "unit", fileId: id, sourceRoles: [], targetRoles: [] },
  };
}

function group(id: string): MappingPreviewNode {
  return { id, type: "previewGroup", position: { x: 0, y: 0 }, data: { kind: "group", count: 1 } };
}

function edge(id: string, source: string, target: string): Edge {
  return { id, source, target };
}
