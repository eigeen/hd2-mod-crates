import ArrowDownwardIcon from "@mui/icons-material/ArrowDownward";
import ArrowUpwardIcon from "@mui/icons-material/ArrowUpward";
import DeleteIcon from "@mui/icons-material/Delete";
import DragIndicatorIcon from "@mui/icons-material/DragIndicator";
import MergeTypeIcon from "@mui/icons-material/MergeType";
import UploadFileIcon from "@mui/icons-material/UploadFile";
import { Button, CircularProgress, IconButton, TextField } from "@mui/material";
import { useCallback, useMemo, useState } from "react";
import { toast } from "sonner";
import {
  closestCenter,
  DndContext,
  KeyboardSensor,
  PointerSensor,
  useSensor,
  useSensors,
  type DragEndEvent,
} from "@dnd-kit/core";
import {
  SortableContext,
  sortableKeyboardCoordinates,
  useSortable,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { useDropZone, useI18n, type PatchFiles, type Translate } from "@hd2-mod-tools/migrator-ui";
import { downloadRepatchedPatch } from "./fileInputs";
import {
  appendManualMergeBatch,
  ManualMergeInputError,
  moveManualMergeRecord,
  type ManualMergeRecord,
} from "./manualMergeInputs";
import { mergePatches, type PatchMergeSummary } from "./wasmClient";

export function ManualMergePanel() {
  const { t } = useI18n();
  const [records, setRecords] = useState<ManualMergeRecord[]>([]);
  const [outputName, setOutputName] = useState("");
  const [busy, setBusy] = useState(false);
  const [summary, setSummary] = useState<PatchMergeSummary | null>(null);
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 6 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );
  const addFiles = useCallback((files: FileList | File[]) => {
    try {
      const next = appendManualMergeBatch(records, files);
      const firstMain = next.find((record) => record.toc);
      if (!outputName && firstMain) setOutputName(firstMain.name);
      setRecords(next);
      setSummary(null);
    } catch (error) {
      toast.error(mergeInputErrorMessage(error, t));
    }
  }, [outputName, records, t]);
  const receiveDrop = useCallback((transfer: DataTransfer) => addFiles(transfer.files), [addFiles]);
  const dropZone = useDropZone(receiveDrop);
  const canMerge = records.length > 0 && records.every((record) => record.toc) && outputName.trim() !== "";
  const summaries = useMemo(() => summary?.sources ?? [], [summary]);

  const runMerge = useCallback(async () => {
    setBusy(true);
    try {
      const inputs = await readPatchInputs(records);
      const result = await mergePatches(inputs, outputName.trim());
      downloadRepatchedPatch(result.patch, result.patch.toc, "hd2-merged-mod.zip");
      setSummary(result.summary);
      toast.success(t("merge.completed", { count: result.summary.resourceCount }));
    } catch (error) {
      toast.error(t("merge.failed"), { description: errorMessage(error) });
    } finally {
      setBusy(false);
    }
  }, [outputName, records, t]);

  const moveRecord = useCallback((sourceId: string, targetId: string) => {
    setRecords((current) => moveManualMergeRecord(current, sourceId, targetId));
    setSummary(null);
  }, []);

  const finishDrag = useCallback((event: DragEndEvent) => {
    const targetId = event.over?.id;
    if (targetId) moveRecord(String(event.active.id), String(targetId));
  }, [moveRecord]);

  return (
    <section className="border-t border-hd2-border bg-black/20 p-5 min-[51.25rem]:p-6">
      <div
        className={`border border-dashed px-4 py-4 ${dropZone.dragging ? "border-hd2-yellow bg-hd2-yellow-bg" : "border-hd2-line"}`}
        {...dropZone.handlers}
      >
        <div className="flex flex-wrap items-center gap-3">
          <Button component="label" startIcon={<UploadFileIcon />} variant="contained">
            {t("merge.pick")}
            <input
              hidden
              multiple
              type="file"
              onChange={(event) => {
                if (event.target.files) addFiles(event.target.files);
                event.target.value = "";
              }}
            />
          </Button>
          <span className="text-xs text-hd2-muted">
            {dropZone.dragging ? t("merge.dropActive") : t("merge.dropHint")}
          </span>
        </div>
      </div>

      <div className="mt-4">
        {records.length === 0 && (
          <p className="m-0 border border-hd2-line px-4 py-6 text-center text-sm text-hd2-muted">
            {t("merge.empty")}
          </p>
        )}
        <DndContext collisionDetection={closestCenter} onDragEnd={finishDrag} sensors={sensors}>
          <SortableContext items={records.map((record) => record.id)} strategy={verticalListSortingStrategy}>
            <div className="space-y-2">
              {records.map((record, index) => (
                <MergeRecordRow
                  key={record.id}
                  index={index}
                  record={record}
                  sourceSummary={summaries[index]}
                  onMove={(offset) => {
                    const target = records[index + offset];
                    if (target) moveRecord(record.id, target.id);
                  }}
                  onRemove={() => {
                    setRecords((current) => current.filter((item) => item.id !== record.id));
                    setSummary(null);
                  }}
                />
              ))}
            </div>
          </SortableContext>
        </DndContext>
      </div>

      <div className="mt-5 flex flex-col gap-3 border-t border-hd2-border pt-4 min-[40rem]:flex-row min-[40rem]:items-center">
        <TextField
          disabled={busy}
          fullWidth
          label={t("merge.outputName")}
          onChange={(event) => setOutputName(event.target.value)}
          size="small"
          value={outputName}
        />
        <Button
          className="shrink-0"
          disabled={!canMerge || busy}
          onClick={() => void runMerge()}
          startIcon={busy ? <CircularProgress size="1rem" /> : <MergeTypeIcon />}
          variant="contained"
        >
          {busy ? t("merge.running") : t("merge.run")}
        </Button>
      </div>
      {records.some((record) => !record.toc) && (
        <p className="mb-0 mt-3 text-xs text-red-300">{t("merge.mainRequired")}</p>
      )}
      {summary && (
        <p className="mb-0 mt-3 text-xs text-hd2-muted">
          {t("merge.summary", {
            conflicts: summary.conflictCount,
            duplicates: summary.duplicateCount,
            repairs: summary.repairedMetadataCount,
            resources: summary.resourceCount,
          })}
        </p>
      )}
    </section>
  );
}

interface MergeRecordRowProps {
  index: number;
  record: ManualMergeRecord;
  sourceSummary?: PatchMergeSummary["sources"][number];
  onMove: (offset: number) => void;
  onRemove: () => void;
}

function MergeRecordRow(props: MergeRecordRowProps) {
  const { t } = useI18n();
  const sortable = useSortable({ id: props.record.id });
  const style = {
    transform: CSS.Transform.toString(sortable.transform),
    transition: sortable.transition,
  };
  return (
    <div
      ref={sortable.setNodeRef}
      className={`flex items-center gap-2 border bg-hd2-surface/90 px-2 py-2 ${sortable.isDragging ? "relative z-10 border-hd2-yellow shadow-lg" : "border-hd2-line"}`}
      style={style}
    >
      <span
        aria-label={t("merge.drag")}
        className="flex cursor-grab touch-none items-center text-hd2-faint outline-none active:cursor-grabbing"
        {...sortable.attributes}
        {...sortable.listeners}
      >
        <DragIndicatorIcon fontSize="small" />
      </span>
      <span className="w-6 shrink-0 text-center font-mono text-xs text-hd2-faint">{props.index + 1}</span>
      <div className="min-w-0 flex-1">
        <p className="m-0 truncate font-mono text-xs text-hd2-text" title={props.record.name}>
          {props.record.name}
        </p>
        <p className="m-0 mt-1 flex flex-wrap gap-x-3 gap-y-1 text-[0.6875rem] text-hd2-muted">
          <FileState label="TOC" present={Boolean(props.record.toc)} />
          <FileState label="GPU" present={Boolean(props.record.gpu)} />
          <FileState label="Stream" present={Boolean(props.record.stream)} />
          {props.sourceSummary && (
            <span>{t("merge.rowSummary", {
              repairs: props.sourceSummary.repairedMetadataCount,
              replaced: props.sourceSummary.replacedResourceCount,
              resources: props.sourceSummary.resourceCount,
            })}</span>
          )}
        </p>
      </div>
      <div className="flex shrink-0 items-center">
        <IconButton aria-label={t("merge.moveUp")} onClick={() => props.onMove(-1)} size="small">
          <ArrowUpwardIcon fontSize="small" />
        </IconButton>
        <IconButton aria-label={t("merge.moveDown")} onClick={() => props.onMove(1)} size="small">
          <ArrowDownwardIcon fontSize="small" />
        </IconButton>
        <IconButton aria-label={t("merge.remove")} onClick={props.onRemove} size="small">
          <DeleteIcon fontSize="small" />
        </IconButton>
      </div>
    </div>
  );
}

function FileState({ label, present }: { label: string; present: boolean }) {
  const { t } = useI18n();
  return (
    <span className={present ? "text-green-300" : "text-hd2-faint"}>
      {label}: {t(present ? "merge.fileSet" : "merge.fileUnset")}
    </span>
  );
}

async function readPatchInputs(records: ManualMergeRecord[]): Promise<PatchFiles[]> {
  const inputs: PatchFiles[] = [];
  for (const record of records) {
    if (!record.toc) throw new Error(`Missing patch main file: ${record.name}`);
    inputs.push({
      name: record.name,
      toc: await fileBytes(record.toc),
      gpu: record.gpu ? await fileBytes(record.gpu) : new Uint8Array(),
      stream: record.stream ? await fileBytes(record.stream) : new Uint8Array(),
    });
  }
  return inputs;
}

async function fileBytes(file: File) {
  return new Uint8Array(await file.arrayBuffer());
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function mergeInputErrorMessage(error: unknown, t: Translate) {
  if (!(error instanceof ManualMergeInputError)) return errorMessage(error);
  if (error.code === "empty") return t("merge.errorEmpty");
  if (error.code === "unsupported") return t("merge.errorUnsupported", { filename: error.filename });
  if (error.code === "duplicate") return t("merge.errorDuplicate", { filename: error.filename });
  return t("merge.errorMissingMain");
}
