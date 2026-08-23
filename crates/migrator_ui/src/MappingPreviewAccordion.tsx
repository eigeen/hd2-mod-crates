import AccountTreeIcon from "@mui/icons-material/AccountTree";
import ExpandMoreIcon from "@mui/icons-material/ExpandMore";
import {
  Accordion,
  AccordionDetails,
  AccordionSummary,
  CircularProgress,
} from "@mui/material";
import { useEffect, useMemo, useState } from "react";
import { useI18n } from "./i18n";
import {
  EmptyPreview,
  MappingBatchCanvas,
  PatchGraphCanvas,
} from "./MappingPreviewCanvas";
import { summarizeMappingGraph } from "./mappingPreviewGraph";
import type {
  EquipmentMappingPreview,
  EquipmentPartGraph,
  MigrationMapping,
  UnitBehaviorOptions,
  UnmatchedUnitPolicy,
} from "./types";

const PREVIEW_DEBOUNCE_MS = 220;

interface MappingPreviewAccordionProps {
  contextKey: string;
  loadPreviews: (mappings: MigrationMapping[]) => Promise<EquipmentMappingPreview[]>;
  mappings: MigrationMapping[];
  patchGraph: EquipmentPartGraph | null;
  unitBehavior: UnitBehaviorOptions;
  unmatchedUnitPolicy: UnmatchedUnitPolicy;
  onUnitBehaviorChange: (behavior: UnitBehaviorOptions) => void;
}

export function MappingPreviewAccordion(props: MappingPreviewAccordionProps) {
  const { t } = useI18n();
  const [expanded, setExpanded] = useState(false);
  const [loaded, setLoaded] = useState<LoadedPreviewBatch | null>(null);
  const [errorRequestKey, setErrorRequestKey] = useState<string | null>(null);
  const selectionKey = mappingSelectionKey(props.mappings);
  const requestKey = previewRequestKey(props.contextKey, selectionKey);
  const debounced = useDebouncedMappings(props.mappings, props.contextKey);
  const previews = loaded?.requestKey === requestKey ? loaded.previews : null;
  const error = errorRequestKey === requestKey;

  useEffect(() => {
    setLoaded(null);
    setErrorRequestKey(null);
  }, [props.contextKey]);

  useEffect(() => {
    if (props.mappings.length > 0) return;
    setLoaded(null);
    setErrorRequestKey(null);
  }, [props.mappings.length]);

  useEffect(() => {
    if (!expanded || debounced.mappings.length === 0) return;
    let current = true;
    setErrorRequestKey(null);
    void props.loadPreviews(debounced.mappings).then((result) => {
      if (current) setLoaded({ previews: result, requestKey: debounced.requestKey });
    }).catch(() => {
      if (current) setErrorRequestKey(debounced.requestKey);
    });
    return () => { current = false; };
  }, [debounced, expanded, props.loadPreviews]);

  const summary = useMemo(
    () => previewSummary({
      behavior: props.unitBehavior,
      mappings: props.mappings,
      patchGraph: props.patchGraph,
      previews,
      t,
    }),
    [previews, props.patchGraph, props.mappings, props.unitBehavior, t],
  );
  return (
    <Accordion
      disableGutters
      expanded={expanded}
      onChange={(_, value) => setExpanded(value)}
      square
      sx={{ backgroundImage: "none", borderTop: "1px solid", borderColor: "var(--color-hd2-border)" }}
    >
      <AccordionSummary expandIcon={<ExpandMoreIcon />}>
        <div className="flex min-w-0 flex-1 items-center gap-3">
          <AccountTreeIcon className="shrink-0 text-hd2-yellow" fontSize="small" />
          <div className="min-w-0 flex-1">
            <div className="text-sm font-bold text-hd2-text">{t("preview.title")}</div>
            <div className="mt-0.5 truncate text-xs text-hd2-muted">{summary}</div>
          </div>
        </div>
      </AccordionSummary>
      <AccordionDetails sx={{ padding: 0 }}>
        {expanded && (
          <PreviewBody {...props} error={error} previews={previews} />
        )}
      </AccordionDetails>
    </Accordion>
  );
}

function PreviewBody(props: MappingPreviewAccordionProps & {
  error: boolean;
  previews: EquipmentMappingPreview[] | null;
}) {
  const { t } = useI18n();
  if (props.mappings.length === 0 && props.patchGraph) {
    return <PatchGraphCanvas
      behavior={props.unitBehavior}
      graph={props.patchGraph}
      onBehaviorChange={props.onUnitBehaviorChange}
      unmatchedUnitPolicy={props.unmatchedUnitPolicy}
    />;
  }
  if (props.mappings.length === 0) return <EmptyPreview text={t("preview.importPatch")} />;
  if (props.error) return <GraphStatusPreview text={t("preview.failed")} />;
  if (!props.previews) return <GraphStatusPreview loading text={t("preview.loading")} />;
  if (!props.previews?.some((preview) => preview.mappings.length > 0)) {
    return <GraphStatusPreview text={t("preview.empty")} />;
  }
  return <MappingBatchCanvas
    behavior={props.unitBehavior}
    onBehaviorChange={props.onUnitBehaviorChange}
    previews={props.previews}
    unmatchedUnitPolicy={props.unmatchedUnitPolicy}
  />;
}

function GraphStatusPreview({ loading, text }: { loading?: boolean; text: string }) {
  return (
    <div className="flex h-[34rem] min-h-[28rem] items-center justify-center gap-2 border-t border-hd2-border bg-hd2-sunken px-6 text-sm text-hd2-muted">
      {loading && <CircularProgress size="1.25rem" />}
      {text}
    </div>
  );
}

interface LoadedPreviewBatch {
  previews: EquipmentMappingPreview[];
  requestKey: string;
}

interface DebouncedMappingSelection {
  mappings: MigrationMapping[];
  requestKey: string;
}

function useDebouncedMappings(
  mappings: MigrationMapping[],
  contextKey: string,
): DebouncedMappingSelection {
  const selectionKey = mappingSelectionKey(mappings);
  const requestKey = previewRequestKey(contextKey, selectionKey);
  const [debounced, setDebounced] = useState<DebouncedMappingSelection>({ mappings, requestKey });
  useEffect(() => {
    const updateDebouncedSelection = () => {
      setDebounced((current) =>
        current.requestKey === requestKey ? current : { mappings, requestKey },
      );
    };
    if (mappings.length === 0) {
      updateDebouncedSelection();
      return;
    }
    const timeout = window.setTimeout(updateDebouncedSelection, PREVIEW_DEBOUNCE_MS);
    return () => window.clearTimeout(timeout);
  }, [mappings, requestKey]);
  return debounced;
}

interface PreviewSummaryInput {
  behavior: UnitBehaviorOptions;
  mappings: MigrationMapping[];
  patchGraph: EquipmentPartGraph | null;
  previews: EquipmentMappingPreview[] | null;
  t: ReturnType<typeof useI18n>["t"];
}

function previewSummary(input: PreviewSummaryInput) {
  const { behavior, mappings, patchGraph, previews, t } = input;
  if (mappings.length === 0 && patchGraph) {
    return t("preview.patchSummary", {
      equipments: patchGraph.patch.equipmentCount,
      mapped: patchGraph.patch.mappedUnitCount,
      wild: patchGraph.patch.unmappedUnitCount,
    });
  }
  if (mappings.length === 0) return t("preview.importPatch");
  if (!previews) return t("preview.batchReady", { count: mappings.length });
  const summary = summarizeMappingGraph(previews, behavior);
  return t("preview.batchSummary", {
    conflicts: summary.conflictCount,
    mappings: summary.mappingCount,
    shared: summary.sharedUnitCount,
    units: summary.unitCount,
  });
}

function mappingSelectionKey(mappings: MigrationMapping[]): string {
  return mappings.map((mapping) => (
    `${mapping.category}:${mapping.sourceHash}>${mapping.targetHash}`
  )).sort().join("|");
}

function previewRequestKey(contextKey: string, selectionKey: string): string {
  return `${contextKey}::${selectionKey}`;
}
