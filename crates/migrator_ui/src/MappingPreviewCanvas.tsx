import WarningAmberIcon from "@mui/icons-material/WarningAmber";
import ShieldOutlinedIcon from "@mui/icons-material/ShieldOutlined";
import {
  Background,
  BackgroundVariant,
  Controls,
  Handle,
  Position,
  ReactFlow,
  type NodeProps,
  type NodeTypes,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { memo, useLayoutEffect, useMemo, useRef, useState } from "react";
import { useI18n } from "./i18n";
import {
  analyzeMappingGraph,
  claimBehaviorKey,
  mappingClaims,
  type MappingConflict,
  type MappingGraphAnalysis,
} from "./mappingPreviewGraph";
import { collectPreviewHighlight } from "./mappingPreviewHighlight";
import {
  layoutMappingPreviews,
  layoutPatchEquipmentGraph,
  type MappingPreviewEdge,
  type MappingPreviewLayout,
  type MappingPreviewNode,
  type PreviewEquipmentNodeData,
  type PreviewGroupNodeData,
  type PreviewUnitNodeData,
} from "./mappingPreviewLayout";
import type {
  CullingPolicy,
  EquipmentMappingPreview,
  EquipmentPartGraph,
  EquipmentPartRole,
  UnitBehaviorOptions,
  UnitMappingBehaviorKey,
  UnmatchedUnitPolicy,
} from "./types";
import {
  hasUnitBehavior,
  mappingIsEnabled,
  preferredConflictSource,
  resolvedUnitExport,
} from "./unitBehavior";
import { UnitContextMenu, type UnitMenuContext, type UnitMenuOutput } from "./UnitContextMenu";
import { VectorPolygonIcon } from "./VectorPolygonIcon";

const nodeTypes: NodeTypes = {
  previewEquipment: memo(PreviewEquipmentNode),
  previewGroup: memo(PreviewGroupNode),
  previewUnit: memo(PreviewUnitNode),
};

interface CanvasBehaviorProps {
  behavior: UnitBehaviorOptions;
  cullingPolicy: CullingPolicy;
  onBehaviorChange: (behavior: UnitBehaviorOptions) => void;
  unmatchedUnitPolicy: UnmatchedUnitPolicy;
}

export function PatchGraphCanvas(props: CanvasBehaviorProps & { graph: EquipmentPartGraph }) {
  const { t } = useI18n();
  const layout = useMemo(() => layoutPatchEquipmentGraph(props.graph), [props.graph]);
  if (layout.nodes.length === 0) return <EmptyPreview text={t("preview.empty")} />;
  return <PreviewCanvas
    {...props}
    key={`patch:${props.graph.patch.name}:${props.graph.components.length}`}
    layout={layout}
    previews={null}
  />;
}

export function MappingBatchCanvas(
  props: CanvasBehaviorProps & { previews: EquipmentMappingPreview[] },
) {
  const layout = useMemo(() => layoutMappingPreviews(props.previews), [props.previews]);
  const structureKey = useMemo(() => mappingStructureKey(props.previews), [props.previews]);
  return <PreviewCanvas {...props} key={structureKey} layout={layout} />;
}

export function EmptyPreview({ icon, text }: { icon?: React.ReactNode; text: string }) {
  return (
    <div className="flex min-h-48 items-center justify-center gap-2 border-t border-hd2-border px-6 py-12 text-sm text-hd2-muted">
      {icon}{text}
    </div>
  );
}

interface PreviewCanvasProps extends CanvasBehaviorProps {
  layout: MappingPreviewLayout;
  previews: EquipmentMappingPreview[] | null;
}

function PreviewCanvas(props: PreviewCanvasProps) {
  const { containerRef, initialSize } = useInitialCanvasSize();
  const [activeNodeId, setActiveNodeId] = useState<string | null>(null);
  const [menuContext, setMenuContext] = useState<UnitMenuContext | null>(null);
  const analysis = useMemo(
    () => props.previews ? analyzeMappingGraph(props.previews, props.behavior) : null,
    [props.behavior, props.previews],
  );
  const behaviorNodes = useMemo(
    () => decorateNodes(props, analysis),
    [analysis, props.behavior, props.layout.nodes, props.previews, props.unmatchedUnitPolicy],
  );
  const behaviorEdges = useMemo(
    () => decorateEdges(props.layout.edges, props.behavior, analysis),
    [analysis, props.behavior, props.layout.edges],
  );
  const highlight = useMemo(
    () => activeNodeId ? collectPreviewHighlight(behaviorNodes, behaviorEdges, activeNodeId) : null,
    [activeNodeId, behaviorEdges, behaviorNodes],
  );
  const visibleNodes = useMemo(
    () => applyNodeHighlight(behaviorNodes, highlight),
    [behaviorNodes, highlight],
  );
  const visibleEdges = useMemo(
    () => applyEdgeHighlight(behaviorEdges, highlight),
    [behaviorEdges, highlight],
  );
  return (
    <div
      className="hd2-equipment-graph h-[34rem] min-h-[28rem] border-t border-hd2-border bg-hd2-sunken"
      onContextMenu={(event) => event.preventDefault()}
      ref={containerRef}
    >
      {initialSize && (
        <ReactFlow
          edges={visibleEdges}
          fitView
          fitViewOptions={{ padding: 0.16 }}
          height={initialSize.height}
          maxZoom={1.5}
          minZoom={0.06}
          nodes={visibleNodes}
          nodesConnectable={false}
          nodesDraggable={false}
          nodeTypes={nodeTypes}
          onNodeClick={(_, node) => setActiveNodeId((current) => current === node.id ? null : node.id)}
          onNodeContextMenu={(event, node) => openUnitMenu(event, node, {
            analysis,
            props,
            setActiveNodeId,
            setMenuContext,
          })}
          onPaneClick={() => {
            setActiveNodeId(null);
            setMenuContext(null);
          }}
          proOptions={{ hideAttribution: true }}
          width={initialSize.width}
        >
          <Background color="#333333" gap={22} size={1} variant={BackgroundVariant.Dots} />
          <Controls showInteractive={false} />
        </ReactFlow>
      )}
      <UnitContextMenu
        behavior={props.behavior}
        context={menuContext}
        onChange={props.onBehaviorChange}
        onClose={() => setMenuContext(null)}
      />
    </div>
  );
}

interface CanvasSize {
  height: number;
  width: number;
}

function useInitialCanvasSize() {
  const containerRef = useRef<HTMLDivElement>(null);
  const [initialSize, setInitialSize] = useState<CanvasSize | null>(null);
  useLayoutEffect(() => {
    const bounds = containerRef.current?.getBoundingClientRect();
    if (!bounds || bounds.width <= 0 || bounds.height <= 0) return;
    setInitialSize({ height: bounds.height, width: bounds.width });
  }, []);
  return { containerRef, initialSize };
}

interface OpenUnitMenuContext {
  analysis: MappingGraphAnalysis | null;
  props: PreviewCanvasProps;
  setActiveNodeId: (id: string) => void;
  setMenuContext: (context: UnitMenuContext) => void;
}

function openUnitMenu(
  event: React.MouseEvent,
  node: MappingPreviewNode,
  context: OpenUnitMenuContext,
) {
  if (node.data.kind !== "unit") return;
  event.preventDefault();
  context.setActiveNodeId(node.id);
  context.setMenuContext(unitMenuContext(
    node,
    {
      analysis: context.analysis,
      anchor: { left: event.clientX, top: event.clientY },
      behavior: context.props.behavior,
      cullingPolicy: context.props.cullingPolicy,
      previews: context.props.previews,
      unmatchedUnitPolicy: context.props.unmatchedUnitPolicy,
    },
  ));
}

function PreviewGroupNode({ data }: NodeProps<MappingPreviewNode>) {
  const { t } = useI18n();
  const group = data as PreviewGroupNodeData;
  return (
    <div className="h-full w-full border border-dashed border-hd2-line bg-hd2-pit px-3 py-3 shadow-lg">
      <div className="flex items-center gap-2">
        <VectorPolygonIcon className="shrink-0 text-hd2-faint" fontSize="small" />
        <div>
          <div className="text-xs font-bold text-hd2-muted">{t("preview.wildGroup")}</div>
          <div className="mt-1 text-[0.625rem] text-hd2-faint">{t("preview.wildCount", { count: group.count })}</div>
        </div>
      </div>
    </div>
  );
}

function PreviewEquipmentNode({ data }: NodeProps<MappingPreviewNode>) {
  const { t } = useI18n();
  const equipment = data as PreviewEquipmentNodeData;
  const sideLabel = equipment.side === "source"
    ? t("preview.sourceRole")
    : equipment.side === "target"
      ? t("preview.targetRole")
      : `${t("preview.sourceRole")} / ${t("preview.targetRole")}`;
  return (
    <div className="h-full w-full overflow-hidden border border-hd2-line bg-hd2-surface px-3 py-2 shadow-lg">
      {equipment.side !== "target" && <Handle position={Position.Right} type="source" />}
      {equipment.side !== "source" && <Handle position={Position.Left} type="target" />}
      <div className="flex items-start gap-2">
        <ShieldOutlinedIcon className="mt-0.5 shrink-0 text-hd2-yellow" fontSize="small" />
        <div className="min-w-0">
          <div className="text-[0.625rem] uppercase tracking-wider text-hd2-muted">{sideLabel}</div>
          <div className="mt-1 line-clamp-2 text-xs font-bold text-hd2-text">{equipment.name}</div>
        </div>
      </div>
    </div>
  );
}

function PreviewUnitNode({ data }: NodeProps<MappingPreviewNode>) {
  const { t } = useI18n();
  const unit = data as PreviewUnitNodeData;
  const related = unit.sourceRoles.length > 0 || unit.targetRoles.length > 0;
  const showCulling = unit.cullingMeshCount !== null && (
    (unit.patchCullingMeshCount ?? 0) > 0 || (unit.targetCullingMeshCount ?? 0) > 0
  );
  const nodeClassName = [
    "hd2-unit-node h-full w-full overflow-visible border border-hd2-line bg-hd2-surface px-3 py-2.5 shadow-lg",
    unit.directReuse ? "hd2-unit-direct-reuse" : "",
    unit.replacementSource ? "hd2-unit-replacement-source" : "",
    unit.shared ? "hd2-unit-shared" : "",
    related ? "" : "hd2-unit-unrecognized",
  ].filter(Boolean).join(" ");
  return (
    <div className={nodeClassName}>
      {unit.sourceRoles.length > 0 && <span aria-hidden className="hd2-unit-corner hd2-unit-corner-source" />}
      {unit.targetRoles.length > 0 && <span aria-hidden className="hd2-unit-corner hd2-unit-corner-target" />}
      {unit.behaviorState && <span aria-hidden className="hd2-unit-corner hd2-unit-corner-behavior" />}
      {related && <Handle position={Position.Left} type="target" />}
      {related && <Handle position={Position.Right} type="source" />}
      <div className="flex items-start gap-2">
        <VectorPolygonIcon className="mt-0.5 shrink-0 text-hd2-muted" fontSize="small" />
        <div className="min-w-0 flex-1">
          <div className="truncate font-mono text-[0.6875rem] text-hd2-text" title={unit.fileId}>{unit.fileId}</div>
          {unit.sourceRoles.length > 0 && <RoleLine label={t("preview.sourceRole")} roles={unit.sourceRoles} />}
          {unit.targetRoles.length > 0 && <RoleLine label={t("preview.targetRole")} roles={unit.targetRoles} />}
          {showCulling && (
            <StatusLine
              className="text-hd2-yellow"
              text={t("preview.culling", {
                count: unit.cullingMeshCount ?? 0,
                source: t(unit.cullingPolicy === "target" ? "preview.cullingTarget" : "preview.cullingPatch"),
              })}
            />
          )}
          {!related && <StatusLine className="text-hd2-faint" text={t("preview.wildUnit")} />}
          {unit.directReuse && <StatusLine className="text-hd2-yellow" text={t("preview.reused")} />}
          {unit.shared && !unit.directReuse && <StatusLine className="text-hd2-related" text={t("preview.shared")} />}
          {unit.conflictState && <ConflictLine unit={unit} />}
        </div>
      </div>
    </div>
  );
}

function ConflictLine({ unit }: { unit: PreviewUnitNodeData }) {
  const { t } = useI18n();
  const resolved = unit.conflictState === "resolved";
  return (
    <div className={`mt-1 flex items-center gap-1 truncate text-[0.625rem] font-bold ${resolved ? "text-hd2-related" : "text-hd2-danger"}`}>
      <WarningAmberIcon sx={{ fontSize: "0.75rem" }} />
      <span>{t(resolved ? "preview.conflictResolved" : "preview.conflictDetected", {
        count: unit.conflictSourceCount ?? 0,
      })}</span>
    </div>
  );
}

function StatusLine({ className, text }: { className: string; text: string }) {
  return <div className={`mt-1 truncate text-[0.625rem] font-bold ${className}`}>{text}</div>;
}

function RoleLine({ label, roles }: { label: string; roles: EquipmentPartRole[] }) {
  const { t } = useI18n();
  return (
    <div className="mt-1 truncate text-[0.625rem] text-hd2-muted">
      {label}：{roles.map((role) => t(roleKey(role))).join(" · ")}
    </div>
  );
}

function decorateNodes(
  props: PreviewCanvasProps,
  analysis: MappingGraphAnalysis | null,
): MappingPreviewNode[] {
  const context: UnitDecorationContext = {
    analysis,
    behavior: props.behavior,
    cullingPolicy: props.cullingPolicy,
    previews: props.previews,
    unmatchedUnitPolicy: props.unmatchedUnitPolicy,
  };
  return props.layout.nodes.map((node) => decorateNode(node, context));
}

interface UnitDecorationContext {
  analysis: MappingGraphAnalysis | null;
  behavior: UnitBehaviorOptions;
  cullingPolicy: CullingPolicy;
  previews: EquipmentMappingPreview[] | null;
  unmatchedUnitPolicy: UnmatchedUnitPolicy;
}

function decorateNode(node: MappingPreviewNode, input: UnitDecorationContext): MappingPreviewNode {
  if (node.data.kind !== "unit") return node;
  const context = unitMenuContext(node, { ...input, anchor: { left: 0, top: 0 } });
  const excluded = context.outputs.some((output) => (
    !resolvedUnitExport(input.behavior, output.fileId, output.defaultExport)
  ));
  const customized = hasUnitBehavior(
    input.behavior,
    context.mappings,
    context.outputs.map((output) => output.fileId),
    context.conflictTargetFileId,
  );
  const conflict = input.analysis?.conflictsByTarget.get(node.id);
  const decorated = decorateUnitNode(node, conflict, excluded, customized);
  const unit = decorated.data as PreviewUnitNodeData;
  return {
    ...decorated,
    data: {
      ...unit,
      cullingPolicy: input.cullingPolicy,
      cullingMeshCount: input.cullingPolicy === "target"
        ? unit.targetCullingMeshCount
        : unit.patchCullingMeshCount,
    },
  };
}

function decorateUnitNode(
  node: MappingPreviewNode,
  conflict: MappingConflict | undefined,
  excluded: boolean,
  customized: boolean,
): MappingPreviewNode {
  if (!conflict && !customized) return node;
  let className = node.className;
  if (conflict) className = appendClass(className, `hd2-unit-conflict-${conflict.state}`);
  if (customized) className = appendClass(className, excluded ? "hd2-unit-excluded" : "hd2-unit-custom");
  return {
    ...node,
    className,
    data: {
      ...node.data,
      behaviorState: customized ? (excluded ? "excluded" : "custom") : undefined,
      conflictSourceCount: conflict?.sourceFileIds.length,
      conflictState: conflict?.state,
    },
  };
}

function decorateEdges(
  edges: MappingPreviewEdge[],
  behavior: UnitBehaviorOptions,
  analysis: MappingGraphAnalysis | null,
): MappingPreviewEdge[] {
  return edges.map((edge) => {
    const keys = edge.data?.mappingKeys ?? [];
    const conflict = edge.data?.targetUnitId
      ? analysis?.conflictsByTarget.get(edge.data.targetUnitId)
      : undefined;
    const disabled = keys.length > 0 && keys.every((key) => mappingKeyIsDisabled(key, behavior, conflict));
    if (disabled) return { ...edge, className: appendClass(edge.className, "hd2-unit-mapping-disabled") };
    if (!conflict || edge.data?.kind !== "mapping") return edge;
    return {
      ...edge,
      className: appendClass(
        edge.className,
        conflict.state === "resolved" ? "hd2-preview-resolved-edge" : "hd2-preview-conflict-edge",
      ),
    };
  });
}

function mappingKeyIsDisabled(
  key: UnitMappingBehaviorKey,
  behavior: UnitBehaviorOptions,
  conflict: MappingConflict | undefined,
): boolean {
  if (!mappingIsEnabled(behavior, key)) return true;
  if (!resolvedUnitExport(behavior, key.targetFileId, true)) return true;
  return Boolean(
    conflict?.preferredSourceFileId && conflict.preferredSourceFileId !== key.sourceFileId,
  );
}

interface UnitMenuContextInput extends UnitDecorationContext {
  anchor: UnitMenuContext["anchor"];
}

function unitMenuContext(
  node: MappingPreviewNode,
  input: UnitMenuContextInput,
): UnitMenuContext {
  const { analysis, anchor, behavior, previews, unmatchedUnitPolicy } = input;
  const unit = node.data as PreviewUnitNodeData;
  if (!previews) return patchUnitMenuContext(unit.fileId, unmatchedUnitPolicy, anchor);
  const claims = analysis?.claims ?? mappingClaims(previews);
  const outgoing = claims.filter((claim) => claim.sourceUnitId === node.id);
  const incoming = claims.filter((claim) => claim.targetUnitId === node.id);
  const replacements = outgoing.filter((claim) => claim.targetUnitId !== node.id);
  const mappings = uniqueMappingKeys(replacements.map(claimBehaviorKey));
  const hasReplacementTarget = replacements.length > 0;
  const outputFileIds = incoming.length > 0 && !hasReplacementTarget ? [unit.fileId] : [];
  const conflict = analysis?.conflictsByTarget.get(node.id);
  const storedResolution = preferredConflictSource(behavior, unit.fileId);
  return {
    anchor,
    fileId: unit.fileId,
    mappings,
    outputs: uniqueOutputs(outputFileIds, true),
    conflictTargetFileId: conflict || storedResolution ? unit.fileId : null,
    conflictSourceFileIds: conflict?.sourceFileIds ?? [],
  };
}

function patchUnitMenuContext(
  fileId: string,
  unmatchedUnitPolicy: UnmatchedUnitPolicy,
  anchor: UnitMenuContext["anchor"],
): UnitMenuContext {
  return {
    anchor,
    fileId,
    mappings: [],
    outputs: [{ fileId, defaultExport: unmatchedUnitPolicy === "keep" }],
    conflictTargetFileId: null,
    conflictSourceFileIds: [],
  };
}

function uniqueMappingKeys(keys: UnitMappingBehaviorKey[]): UnitMappingBehaviorKey[] {
  const unique = new Map(keys.map((key) => [`${key.sourceFileId}>${key.targetFileId}`, key]));
  return [...unique.values()];
}

function uniqueOutputs(fileIds: string[], defaultExport: boolean): UnitMenuOutput[] {
  return [...new Set(fileIds)].map((fileId) => ({ fileId, defaultExport }));
}

function applyNodeHighlight(
  nodes: MappingPreviewNode[],
  highlight: ReturnType<typeof collectPreviewHighlight> | null,
): MappingPreviewNode[] {
  if (!highlight) return nodes;
  return nodes.map((node) => ({
    ...node,
    className: appendClass(node.className, highlightClass(
      highlight.nodeIds.has(node.id),
      highlight.reverseNodeIds.has(node.id),
    )),
  }));
}

function applyEdgeHighlight(
  edges: MappingPreviewEdge[],
  highlight: ReturnType<typeof collectPreviewHighlight> | null,
): MappingPreviewEdge[] {
  if (!highlight) return edges;
  return edges.map((edge) => ({
    ...edge,
    className: appendClass(edge.className, highlightClass(
      highlight.edgeIds.has(edge.id),
      highlight.reverseEdgeIds.has(edge.id),
    )),
  }));
}

function highlightClass(active: boolean, reverse: boolean): string {
  if (active) return "hd2-graph-active";
  return reverse ? "hd2-graph-reverse" : "hd2-graph-dimmed";
}

function appendClass(current: string | undefined, next: string): string {
  return current ? `${current} ${next}` : next;
}

function roleKey(role: EquipmentPartRole) {
  return `graph.role.${role}` as const;
}

function mappingStructureKey(previews: EquipmentMappingPreview[]): string {
  return previews.map((preview) => {
    const mappings = preview.mappings.map((mapping) => (
      `${mapping.sourceUnitId}>${mapping.targetUnitId}`
    )).join(",");
    return `${preview.sourceEquipment.id}>${preview.targetEquipment.id}:${mappings}`;
  }).join("|");
}
