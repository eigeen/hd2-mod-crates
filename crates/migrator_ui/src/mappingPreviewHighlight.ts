import type { Edge } from "@xyflow/react";
import type { MappingPreviewNode } from "./mappingPreviewLayout";

interface DirectedEdge {
  edge: Edge;
  neighborId: string;
}

export interface PreviewHighlight {
  edgeIds: ReadonlySet<string>;
  nodeIds: ReadonlySet<string>;
  reverseEdgeIds: ReadonlySet<string>;
  reverseNodeIds: ReadonlySet<string>;
}

interface PreviewPath {
  edgeIds: Set<string>;
  nodeIds: Set<string>;
}

interface DirectedAdjacency {
  incoming: ReadonlyMap<string, DirectedEdge[]>;
  outgoing: ReadonlyMap<string, DirectedEdge[]>;
}

/** Separates the selected node's forward path from reverse/shared consumers. */
export function collectPreviewHighlight(
  nodes: MappingPreviewNode[],
  edges: Edge[],
  selectedNodeId: string,
): PreviewHighlight {
  const nodesById = new Map(nodes.map((node) => [node.id, node]));
  if (!nodesById.has(selectedNodeId)) return emptyHighlight();
  const adjacency = buildDirectedAdjacency(edges);
  const forward = collectReachablePath(selectedNodeId, adjacency.outgoing);
  const reverse = collectReversePath(forward, adjacency.incoming);
  return {
    edgeIds: forward.edgeIds,
    nodeIds: forward.nodeIds,
    reverseEdgeIds: reverse.edgeIds,
    reverseNodeIds: reverse.nodeIds,
  };
}

function collectReachablePath(
  selectedNodeId: string,
  adjacency: ReadonlyMap<string, DirectedEdge[]>,
): PreviewPath {
  const edgeIds = new Set<string>();
  const nodeIds = new Set<string>([selectedNodeId]);
  const queue = [selectedNodeId];
  while (queue.length > 0) {
    const nodeId = queue.shift();
    if (!nodeId) continue;
    for (const adjacent of adjacency.get(nodeId) ?? []) {
      edgeIds.add(adjacent.edge.id);
      if (nodeIds.has(adjacent.neighborId)) continue;
      nodeIds.add(adjacent.neighborId);
      queue.push(adjacent.neighborId);
    }
  }
  return { edgeIds, nodeIds };
}

function collectReversePath(forward: PreviewPath, incoming: ReadonlyMap<string, DirectedEdge[]>): PreviewPath {
  const edgeIds = new Set<string>();
  const nodeIds = new Set<string>();
  const queue = [...forward.nodeIds];
  while (queue.length > 0) {
    const nodeId = queue.shift();
    if (!nodeId) continue;
    for (const adjacent of incoming.get(nodeId) ?? []) {
      if (forward.edgeIds.has(adjacent.edge.id)) continue;
      edgeIds.add(adjacent.edge.id);
      if (forward.nodeIds.has(adjacent.neighborId) || nodeIds.has(adjacent.neighborId)) continue;
      nodeIds.add(adjacent.neighborId);
      queue.push(adjacent.neighborId);
    }
  }
  return { edgeIds, nodeIds };
}

function buildDirectedAdjacency(edges: Edge[]): DirectedAdjacency {
  const incoming = new Map<string, DirectedEdge[]>();
  const outgoing = new Map<string, DirectedEdge[]>();
  edges.forEach((edge) => {
    addDirectedEdge(outgoing, edge.source, { edge, neighborId: edge.target });
    addDirectedEdge(incoming, edge.target, { edge, neighborId: edge.source });
  });
  return { incoming, outgoing };
}

function addDirectedEdge(
  adjacency: Map<string, DirectedEdge[]>,
  nodeId: string,
  adjacent: DirectedEdge,
) {
  const entries = adjacency.get(nodeId) ?? [];
  entries.push(adjacent);
  adjacency.set(nodeId, entries);
}

function emptyHighlight(): PreviewHighlight {
  return { edgeIds: new Set(), nodeIds: new Set(), reverseEdgeIds: new Set(), reverseNodeIds: new Set() };
}
