import dagre from "@dagrejs/dagre";
import { MarkerType, Position, type Edge, type Node } from "@xyflow/react";
import {
  claimBehaviorKey,
  mappingClaims,
  mappingConflictTargetIds,
  mappingSharedUnitIds,
} from "./mappingPreviewGraph";
import type {
  EquipmentMappingPreview,
  EquipmentPartGraph,
  EquipmentPartRole,
  GraphEquipment,
  UnitMappingBehaviorKey,
} from "./types";

const EQUIPMENT_SIZE = { width: 210, height: 76 };
const MAPPING_UNIT_WIDTH = 230;
const UNIT_BASE_HEIGHT = 52;
const UNIT_MIN_HEIGHT = 72;
const UNIT_ROW_HEIGHT = 18;
const PATCH_UNIT_SIZE = { width: 220, height: UNIT_MIN_HEIGHT };
const PATCH_COLUMNS = 4;
const PATCH_COLUMN_GAP = 28;
const PATCH_ROW_GAP = 24;
const PATCH_RANK_GAP = 90;
const PATCH_CLUSTER_GAP = 56;
const PATCH_MARGIN = 30;

export interface PreviewEquipmentNodeData extends Record<string, unknown> {
  kind: "equipment";
  side: "source" | "target" | "both";
  name: string;
  category: "Armor" | "Helmet";
}

export interface PreviewUnitNodeData extends Record<string, unknown> {
  kind: "unit";
  fileId: string;
  sourceRoles: EquipmentPartRole[];
  targetRoles: EquipmentPartRole[];
  layout: "compact" | "detailed";
  shared?: boolean;
  directReuse?: boolean;
  replacementSource?: boolean;
  conflictCapable?: boolean;
  conflictSourceCount?: number;
  conflictState?: "unresolved" | "resolved";
  behaviorState?: "custom" | "excluded";
}

export interface PreviewGroupNodeData extends Record<string, unknown> {
  kind: "group";
  count: number;
}

export interface PreviewEdgeData extends Record<string, unknown> {
  kind: "mapping" | "ownership";
  mappingKeys: UnitMappingBehaviorKey[];
  targetUnitId?: string;
}

export type MappingPreviewNodeData = PreviewEquipmentNodeData | PreviewUnitNodeData | PreviewGroupNodeData;
export type MappingPreviewNode = Node<MappingPreviewNodeData>;
export type MappingPreviewEdge = Edge<PreviewEdgeData>;

export interface MappingPreviewLayout {
  nodes: MappingPreviewNode[];
  edges: MappingPreviewEdge[];
}

/** Backward-compatible single-pair layout used by focused tests and callers. */
export function layoutMappingPreview(preview: EquipmentMappingPreview): MappingPreviewLayout {
  return layoutMappingPreviews([preview]);
}

/** Lays out every selected replacement in one canonical Unit graph. */
export function layoutMappingPreviews(previews: EquipmentMappingPreview[]): MappingPreviewLayout {
  return layoutDagNodes(createMappingNodes(previews), createMappingEdges(previews));
}

/** Layouts recognized equipment relationships and groups otherwise unmatched Patch Units. */
export function layoutPatchEquipmentGraph(graph: EquipmentPartGraph): MappingPreviewLayout {
  const rolesByComponent = componentRoles(graph);
  const equipmentNodes = graph.equipments.map(patchEquipmentNode);
  const unitNodes = graph.components.map((component) => patchUnitNode(
    component.id,
    component.fileId,
    rolesByComponent,
  ));
  const unmatchedIds = new Set(graph.diagnostics.map((diagnostic) => diagnostic.componentId));
  const mappedUnits = unitNodes.filter((node) => !unmatchedIds.has(node.id));
  const unmatchedUnits = unitNodes.filter((node) => unmatchedIds.has(node.id));
  const edges = graph.relations.map((relation) => ownershipEdge(
    relation.id,
    relation.equipmentId,
    relation.componentId,
  ));
  const recognizedLayout = layoutDagNodes([...equipmentNodes, ...mappedUnits], edges);
  const unmatchedGroup = createUnmatchedUnitGroup(graph, unmatchedUnits.length);
  return {
    nodes: appendUnmatchedUnitCluster(recognizedLayout.nodes, unmatchedGroup, unmatchedUnits),
    edges,
  };
}

function createMappingNodes(previews: EquipmentMappingPreview[]): MappingPreviewNode[] {
  return [...collectEquipmentNodes(previews), ...collectUnitNodes(previews)];
}

function collectEquipmentNodes(previews: EquipmentMappingPreview[]): MappingPreviewNode[] {
  const equipment = new Map<string, PreviewEquipmentNodeData>();
  previews.forEach((preview) => {
    mergeEquipment(equipment, preview.sourceEquipment, "source");
    mergeEquipment(equipment, preview.targetEquipment, "target");
  });
  return [...equipment].map(([id, data]) => ({
    id,
    type: "previewEquipment",
    position: { x: 0, y: 0 },
    sourcePosition: Position.Right,
    targetPosition: Position.Left,
    data,
  }));
}

function mergeEquipment(
  equipment: Map<string, PreviewEquipmentNodeData>,
  value: GraphEquipment,
  side: "source" | "target",
) {
  const current = equipment.get(value.id);
  equipment.set(value.id, {
    kind: "equipment",
    side: current && current.side !== side ? "both" : side,
    name: value.name,
    category: value.category,
  });
}

function collectUnitNodes(previews: EquipmentMappingPreview[]): MappingPreviewNode[] {
  const roles = new Map<string, UnitRoleBuilder>();
  previews.forEach((preview) => preview.units.forEach((unit) => mergeUnitRoles(roles, unit)));
  const claims = mappingClaims(previews);
  const shared = mappingSharedUnitIds(claims);
  const conflictTargets = mappingConflictTargetIds(claims);
  const directReuse = new Set(
    claims.filter((claim) => claim.sourceUnitId === claim.targetUnitId)
      .map((claim) => claim.sourceUnitId),
  );
  const replacementSources = new Set(
    claims.filter((claim) => claim.sourceUnitId !== claim.targetUnitId)
      .map((claim) => claim.sourceUnitId),
  );
  return [...roles].map(([id, unit]) => ({
    id,
    type: "previewUnit",
    position: { x: 0, y: 0 },
    sourcePosition: Position.Right,
    targetPosition: Position.Left,
    data: {
      kind: "unit",
      fileId: unit.fileId,
      sourceRoles: [...unit.sourceRoles],
      targetRoles: [...unit.targetRoles],
      layout: "detailed",
      shared: shared.has(id),
      directReuse: directReuse.has(id),
      replacementSource: replacementSources.has(id),
      conflictCapable: conflictTargets.has(id),
    },
  }));
}

interface UnitRoleBuilder {
  fileId: string;
  sourceRoles: Set<EquipmentPartRole>;
  targetRoles: Set<EquipmentPartRole>;
}

function mergeUnitRoles(
  roles: Map<string, UnitRoleBuilder>,
  unit: EquipmentMappingPreview["units"][number],
) {
  const builder = roles.get(unit.id) ?? {
    fileId: unit.fileId,
    sourceRoles: new Set<EquipmentPartRole>(),
    targetRoles: new Set<EquipmentPartRole>(),
  };
  unit.sourceRoles.forEach((role) => builder.sourceRoles.add(role));
  unit.targetRoles.forEach((role) => builder.targetRoles.add(role));
  roles.set(unit.id, builder);
}

function createMappingEdges(previews: EquipmentMappingPreview[]): MappingPreviewEdge[] {
  const edges = new Map<string, MappingPreviewEdge>();
  mappingClaims(previews).forEach((claim) => {
    const key = claimBehaviorKey(claim);
    mergeBehaviorEdge(edges, {
      className: "hd2-preview-ownership-edge",
      id: `ownership:${claim.sourceEquipment.id}:${claim.sourceUnitId}`,
      key,
      kind: "ownership",
      source: claim.sourceEquipment.id,
      target: claim.sourceUnitId,
    });
    mergeBehaviorEdge(edges, {
      className: "hd2-preview-ownership-edge",
      id: `ownership:${claim.targetUnitId}:${claim.targetEquipment.id}`,
      key,
      kind: "ownership",
      source: claim.targetUnitId,
      target: claim.targetEquipment.id,
    });
    if (claim.sourceUnitId === claim.targetUnitId) return;
    mergeBehaviorEdge(edges, {
      className: "hd2-preview-remap-edge",
      id: `mapping:${claim.sourceUnitId}:${claim.targetUnitId}`,
      key,
      kind: "mapping",
      source: claim.sourceUnitId,
      target: claim.targetUnitId,
      targetUnitId: claim.targetUnitId,
    });
  });
  return [...edges.values()];
}

interface BehaviorEdgeInput {
  className: string;
  id: string;
  key: UnitMappingBehaviorKey;
  kind: PreviewEdgeData["kind"];
  source: string;
  target: string;
  targetUnitId?: string;
}

function mergeBehaviorEdge(edges: Map<string, MappingPreviewEdge>, input: BehaviorEdgeInput) {
  const current = edges.get(input.id);
  if (current?.data) {
    current.data.mappingKeys = appendUniqueMappingKey(current.data.mappingKeys, input.key);
    return;
  }
  edges.set(input.id, {
    id: input.id,
    source: input.source,
    target: input.target,
    type: "default",
    className: input.className,
    data: {
      kind: input.kind,
      mappingKeys: [input.key],
      targetUnitId: input.targetUnitId,
    },
    ...(input.kind === "mapping" ? {
      markerEnd: { type: MarkerType.ArrowClosed, width: 16, height: 16, color: "#fee70f" },
    } : {}),
  });
}

function appendUniqueMappingKey(
  keys: UnitMappingBehaviorKey[],
  next: UnitMappingBehaviorKey,
): UnitMappingBehaviorKey[] {
  return keys.some((key) => (
    key.sourceFileId === next.sourceFileId && key.targetFileId === next.targetFileId
  )) ? keys : [...keys, next];
}

function layoutDagNodes(
  nodes: MappingPreviewNode[],
  edges: MappingPreviewEdge[],
): MappingPreviewLayout {
  const graph = new dagre.graphlib.Graph().setDefaultEdgeLabel(() => ({}));
  graph.setGraph({ rankdir: "LR", ranksep: 115, nodesep: 32, marginx: 30, marginy: 30 });
  nodes.forEach((node) => graph.setNode(node.id, { ...dimensions(node) }));
  edges.forEach((edge) => graph.setEdge(edge.source, edge.target));
  dagre.layout(graph);
  return {
    nodes: nodes.map((node) => positionNode(node, graph.node(node.id))),
    edges,
  };
}

function appendUnmatchedUnitCluster(
  recognizedNodes: MappingPreviewNode[],
  unmatchedGroup: MappingPreviewNode | null,
  unmatchedUnits: MappingPreviewNode[],
): MappingPreviewNode[] {
  if (!unmatchedGroup) return recognizedNodes;
  const recognizedBottom = Math.max(PATCH_MARGIN, ...recognizedNodes.map(nodeBottom));
  const unmatchedTop = recognizedBottom + (recognizedNodes.length ? PATCH_CLUSTER_GAP : 0);
  const cluster = layoutPatchCluster([unmatchedGroup], unmatchedUnits, unmatchedTop);
  return [...recognizedNodes, ...cluster.nodes];
}

function nodeBottom(node: MappingPreviewNode): number {
  return node.position.y + dimensions(node).height;
}

function layoutPatchCluster(
  roots: MappingPreviewNode[],
  units: MappingPreviewNode[],
  top: number,
): { nodes: MappingPreviewNode[]; height: number } {
  const rootHeight = stackHeight(roots.length, EQUIPMENT_SIZE.height, PATCH_ROW_GAP);
  const columns = Math.min(PATCH_COLUMNS, Math.max(units.length, 1));
  const rows = Math.ceil(units.length / columns);
  const unitHeight = stackHeight(rows, PATCH_UNIT_SIZE.height, PATCH_ROW_GAP);
  const height = Math.max(rootHeight, unitHeight);
  const rootTop = top + (height - rootHeight) / 2;
  const unitTop = top + (height - unitHeight) / 2;
  const rootNodes = roots.map((node, index) => positionSizedNode(
    node,
    PATCH_MARGIN,
    rootTop + index * (EQUIPMENT_SIZE.height + PATCH_ROW_GAP),
  ));
  const unitX = PATCH_MARGIN + EQUIPMENT_SIZE.width + PATCH_RANK_GAP;
  const unitNodes = units.map((node, index) => positionPatchUnit(node, index, columns, unitX, unitTop));
  return { nodes: [...rootNodes, ...unitNodes], height };
}

function positionPatchUnit(
  node: MappingPreviewNode,
  index: number,
  columns: number,
  left: number,
  top: number,
): MappingPreviewNode {
  const column = index % columns;
  const row = Math.floor(index / columns);
  const x = left + column * (PATCH_UNIT_SIZE.width + PATCH_COLUMN_GAP);
  const y = top + row * (PATCH_UNIT_SIZE.height + PATCH_ROW_GAP);
  return positionSizedNode(node, x, y);
}

function stackHeight(count: number, size: number, gap: number): number {
  return count === 0 ? 0 : count * size + (count - 1) * gap;
}

function positionSizedNode(node: MappingPreviewNode, x: number, y: number): MappingPreviewNode {
  const size = dimensions(node);
  return { ...node, ...size, position: { x, y } };
}

function componentRoles(graph: EquipmentPartGraph): Map<string, EquipmentPartRole[]> {
  const roles = new Map<string, Set<EquipmentPartRole>>();
  graph.relations.forEach((relation) => {
    const values = roles.get(relation.componentId) ?? new Set<EquipmentPartRole>();
    values.add(relation.role);
    roles.set(relation.componentId, values);
  });
  return new Map([...roles].map(([id, values]) => [id, [...values]]));
}

function patchEquipmentNode(equipment: EquipmentPartGraph["equipments"][number]): MappingPreviewNode {
  return {
    id: equipment.id,
    type: "previewEquipment",
    position: { x: 0, y: 0 },
    sourcePosition: Position.Right,
    data: {
      kind: "equipment",
      side: "source",
      name: equipment.name,
      category: equipment.category,
    },
  };
}

function patchUnitNode(
  id: string,
  fileId: string,
  rolesByComponent: ReadonlyMap<string, EquipmentPartRole[]>,
): MappingPreviewNode {
  return {
    id,
    type: "previewUnit",
    position: { x: 0, y: 0 },
    targetPosition: Position.Left,
    data: {
      kind: "unit",
      fileId,
      sourceRoles: rolesByComponent.get(id) ?? [],
      targetRoles: [],
      layout: "compact",
    },
  };
}

function createUnmatchedUnitGroup(
  graph: EquipmentPartGraph,
  unmatchedUnitCount: number,
): MappingPreviewNode | null {
  if (unmatchedUnitCount === 0) return null;
  return {
    id: `unmatched:${graph.patch.name}`,
    type: "previewGroup",
    position: { x: 0, y: 0 },
    sourcePosition: Position.Right,
    data: { kind: "group", count: unmatchedUnitCount },
  };
}

function ownershipEdge(id: string, source: string, target: string): MappingPreviewEdge {
  return {
    id,
    source,
    target,
    type: "default",
    className: "hd2-preview-ownership-edge",
    data: { kind: "ownership", mappingKeys: [] },
  };
}

function dimensions(node: MappingPreviewNode) {
  if (node.data.kind !== "unit") return EQUIPMENT_SIZE;
  if (node.data.layout === "compact") return PATCH_UNIT_SIZE;
  return { width: MAPPING_UNIT_WIDTH, height: detailedUnitHeight(node.data) };
}

/** Calculates layout height from structural rows so interaction state never triggers relayout. */
function detailedUnitHeight(unit: PreviewUnitNodeData): number {
  const roleRows = Number(unit.sourceRoles.length > 0) + Number(unit.targetRoles.length > 0);
  const relationshipRows = Number(Boolean(unit.directReuse || unit.shared));
  const conflictRows = Number(Boolean(unit.conflictCapable));
  const height = UNIT_BASE_HEIGHT
    + UNIT_ROW_HEIGHT * (roleRows + relationshipRows + conflictRows);
  return Math.max(UNIT_MIN_HEIGHT, height);
}

function positionNode(
  node: MappingPreviewNode,
  center: { x: number; y: number },
): MappingPreviewNode {
  const size = dimensions(node);
  return {
    ...node,
    width: size.width,
    height: size.height,
    position: { x: center.x - size.width / 2, y: center.y - size.height / 2 },
  };
}
