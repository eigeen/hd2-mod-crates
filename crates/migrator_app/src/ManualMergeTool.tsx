import ArrowDownwardIcon from "@mui/icons-material/ArrowDownward";
import ArrowUpwardIcon from "@mui/icons-material/ArrowUpward";
import DeleteIcon from "@mui/icons-material/Delete";
import DragIndicatorIcon from "@mui/icons-material/DragIndicator";
import MergeTypeIcon from "@mui/icons-material/MergeType";
import UploadFileIcon from "@mui/icons-material/UploadFile";
import { Button, CircularProgress, IconButton, TextField } from "@mui/material";
import { useCallback, useEffect, useMemo, useState } from "react";
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
import {
  useDropZone,
  useI18n,
  type PatchMergeSummary,
} from "@hd2-mod-tools/migrator-ui";
import { useTaskController } from "./taskController";
import type { MergeInputFile } from "./mergeInputs";

export interface ManualMergeRecord<Source> {
  byteLength?: number;
  fileRoles?: { gpu: boolean; stream: boolean; toc: boolean };
  id: string;
  name: string;
  source: Source;
}

export interface ManualMergeAdapter<Source, FileSource = never> {
  browserFileReference?: (file: File) => MergeInputFile<FileSource>;
  cancellable: boolean;
  pickRecords: () => Promise<ManualMergeRecord<Source>[] | null>;
  recordsFromFiles?: (files: MergeInputFile<FileSource>[]) => Promise<ManualMergeRecord<Source>[]>;
  subscribeToFileDrop?: (
    receive: (files: MergeInputFile<FileSource>[]) => void,
    reject: (error: unknown) => void,
  ) => () => void;
  runMerge: (request: {
    outputName: string;
    records: ManualMergeRecord<Source>[];
    signal: AbortSignal;
  }) => Promise<{ output: string; summary: PatchMergeSummary } | null>;
}

export function ManualMergeTool<Source, FileSource>({ adapter }: { adapter: ManualMergeAdapter<Source, FileSource> }) {
  const { t } = useI18n();
  const [records, setRecords] = useState<ManualMergeRecord<Source>[]>([]);
  const [outputName, setOutputName] = useState("");
  const [summary, setSummary] = useState<PatchMergeSummary | null>(null);
  const task = useTaskController();
  const runTask = task.run;
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 6 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );

  const addRecords = useCallback((added: ManualMergeRecord<Source>[]) => {
    if (!added.length) return;
    setRecords((current) => [...current, ...added]);
    setOutputName((current) => current || added[0].name);
    setSummary(null);
  }, []);

  const pickRecords = useCallback(async () => {
    await runTask({
      execute: () => adapter.pickRecords(),
      fallbackErrorCode: "merge.failed",
      onSuccess: addRecords,
    });
  }, [adapter, addRecords, runTask]);

  const receiveFiles = useCallback(async (files: MergeInputFile<FileSource>[]) => {
    if (!adapter.recordsFromFiles) return;
    await runTask({
      execute: () => adapter.recordsFromFiles!(files),
      fallbackErrorCode: "merge.failed",
      onSuccess: addRecords,
    });
  }, [adapter, addRecords, runTask]);
  const receiveDrop = useCallback(async (transfer: DataTransfer) => {
    if (!adapter.browserFileReference) return;
    await receiveFiles(Array.from(transfer.files).map(adapter.browserFileReference));
  }, [adapter.browserFileReference, receiveFiles]);
  const dropZone = useDropZone(receiveDrop);
  const browserDropEnabled = Boolean(adapter.browserFileReference && adapter.recordsFromFiles);
  const dropEnabled = browserDropEnabled || Boolean(adapter.subscribeToFileDrop);
  useEffect(() => adapter.subscribeToFileDrop?.(
    (files) => { void receiveFiles(files); },
    (error) => { void runTask({ execute: () => Promise.reject(error), fallbackErrorCode: "merge.failed" }); },
  ), [adapter, receiveFiles, runTask]);
  const summaries = useMemo(() => summary?.sources ?? [], [summary]);
  const canMerge = records.length > 0
    && records.every((record) => record.fileRoles?.toc !== false)
    && outputName.trim() !== "";

  const runMerge = useCallback(async () => {
    await runTask({
      acceptResultAfterCancel: true,
      execute: (signal) => adapter.runMerge({
        outputName: outputName.trim(),
        records,
        signal,
      }),
      fallbackErrorCode: "merge.failed",
      onSuccess: (result) => {
        setSummary(result.summary);
        toast.success(t("merge.completed", { count: result.summary.resourceCount }));
      },
      report: (result) => ({ kind: "merge", ...result }),
    });
  }, [adapter, outputName, records, runTask, t]);

  const moveRecord = useCallback((sourceId: string, targetId: string) => {
    setRecords((current) => moveRecordToTarget(current, sourceId, targetId));
    setSummary(null);
  }, []);

  const finishDrag = useCallback((event: DragEndEvent) => {
    const targetId = event.over?.id;
    if (targetId) moveRecord(String(event.active.id), String(targetId));
  }, [moveRecord]);

  return (
    <section className="border-t border-hd2-border bg-black/20 p-5 min-[51.25rem]:p-6">
      <div className={dropEnabled ? `border border-dashed px-4 py-4 ${dropZone.dragging ? "border-hd2-yellow bg-hd2-yellow-bg" : "border-hd2-line"}` : ""} {...(browserDropEnabled ? dropZone.handlers : {})}>
        <div className="flex flex-wrap items-center gap-3">
          <Button disabled={task.busy} onClick={() => void pickRecords()} startIcon={<UploadFileIcon />} variant="contained">{t("merge.pick")}</Button>
          {dropEnabled && <span className="text-xs text-hd2-muted">{dropZone.dragging ? t("merge.dropActive") : t("merge.dropHint")}</span>}
        </div>
      </div>
      <RecordList
        busy={task.busy}
        finishDrag={finishDrag}
        moveRecord={moveRecord}
        records={records}
        removeRecord={(id) => {
          setRecords((current) => current.filter((record) => record.id !== id));
          setSummary(null);
        }}
        sensors={sensors}
        summaries={summaries}
      />
      <div className="mt-5 flex flex-col gap-3 border-t border-hd2-border pt-4 min-[40rem]:flex-row min-[40rem]:items-center">
        <TextField disabled={task.busy} fullWidth label={t("merge.outputName")} onChange={(event) => setOutputName(event.target.value)} size="small" value={outputName} />
        {task.busy && adapter.cancellable ? (
          <Button color="error" className="shrink-0" onClick={task.cancel} variant="outlined">{t("task.cancel")}</Button>
        ) : (
          <Button className="shrink-0" disabled={!canMerge || task.busy} onClick={() => void runMerge()} startIcon={task.busy ? <CircularProgress size="1rem" /> : <MergeTypeIcon />} variant="contained">{task.busy ? t("merge.running") : t("merge.run")}</Button>
        )}
      </div>
      {records.some((record) => record.fileRoles?.toc === false) && <p className="mb-0 mt-3 text-xs text-red-300">{t("merge.mainRequired")}</p>}
      {summary && <MergeSummary summary={summary} />}
    </section>
  );
}

interface RecordListProps<Source> {
  busy: boolean;
  finishDrag: (event: DragEndEvent) => void;
  moveRecord: (sourceId: string, targetId: string) => void;
  records: ManualMergeRecord<Source>[];
  removeRecord: (id: string) => void;
  sensors: ReturnType<typeof useSensors>;
  summaries: PatchMergeSummary["sources"];
}

function RecordList<Source>(props: RecordListProps<Source>) {
  const { t } = useI18n();
  if (!props.records.length) {
    return <p className="mb-0 mt-4 border border-hd2-line px-4 py-6 text-center text-sm text-hd2-muted">{t("merge.empty")}</p>;
  }
  return (
    <DndContext collisionDetection={closestCenter} onDragEnd={props.finishDrag} sensors={props.sensors}>
      <SortableContext items={props.records.map((record) => record.id)} strategy={verticalListSortingStrategy}>
        <div className="mt-4 space-y-2">
          {props.records.map((record, index) => (
            <MergeRecordRow
              busy={props.busy}
              index={index}
              key={record.id}
              onMove={(offset) => {
                const target = props.records[index + offset];
                if (target) props.moveRecord(record.id, target.id);
              }}
              onRemove={() => props.removeRecord(record.id)}
              record={record}
              sourceSummary={props.summaries[index]}
            />
          ))}
        </div>
      </SortableContext>
    </DndContext>
  );
}

function MergeRecordRow<Source>(props: {
  busy: boolean;
  index: number;
  onMove: (offset: number) => void;
  onRemove: () => void;
  record: ManualMergeRecord<Source>;
  sourceSummary?: PatchMergeSummary["sources"][number];
}) {
  const { t } = useI18n();
  const sortable = useSortable({ id: props.record.id });
  return (
    <div ref={sortable.setNodeRef} className={`flex items-center gap-2 border bg-hd2-surface/90 px-2 py-2 ${sortable.isDragging ? "relative z-10 border-hd2-yellow shadow-lg" : "border-hd2-line"}`} style={{ transform: CSS.Transform.toString(sortable.transform), transition: sortable.transition }}>
      <span aria-label={t("merge.drag")} className="flex cursor-grab touch-none items-center text-hd2-faint outline-none active:cursor-grabbing" {...sortable.attributes} {...sortable.listeners}><DragIndicatorIcon fontSize="small" /></span>
      <span className="w-6 shrink-0 text-center font-mono text-xs text-hd2-faint">{props.index + 1}</span>
      <div className="min-w-0 flex-1">
        <p className="m-0 truncate font-mono text-xs text-hd2-text" title={props.record.name}>{props.record.name}</p>
        <RecordDetails record={props.record} summary={props.sourceSummary} />
      </div>
      <IconButton disabled={props.busy || props.index === 0} aria-label={t("merge.moveUp")} onClick={() => props.onMove(-1)} size="small"><ArrowUpwardIcon fontSize="small" /></IconButton>
      <IconButton disabled={props.busy} aria-label={t("merge.moveDown")} onClick={() => props.onMove(1)} size="small"><ArrowDownwardIcon fontSize="small" /></IconButton>
      <IconButton disabled={props.busy} aria-label={t("merge.remove")} onClick={props.onRemove} size="small"><DeleteIcon fontSize="small" /></IconButton>
    </div>
  );
}

function RecordDetails<Source>({ record, summary }: {
  record: ManualMergeRecord<Source>;
  summary?: PatchMergeSummary["sources"][number];
}) {
  const { t } = useI18n();
  if (summary) return <p className="m-0 mt-1 text-[0.6875rem] text-hd2-muted">{t("merge.rowSummary", { repairs: summary.repairedMetadataCount, replaced: summary.replacedResourceCount, resources: summary.resourceCount })}</p>;
  if (record.fileRoles) return <p className="m-0 mt-1 flex gap-3 text-[0.6875rem] text-hd2-muted"><FileState label="TOC" present={record.fileRoles.toc} /><FileState label="GPU" present={record.fileRoles.gpu} /><FileState label="Stream" present={record.fileRoles.stream} /></p>;
  return <p className="m-0 mt-1 text-[0.6875rem] text-hd2-muted">{formatBytes(record.byteLength ?? 0)}</p>;
}

function FileState({ label, present }: { label: string; present: boolean }) {
  const { t } = useI18n();
  return <span className={present ? "text-green-300" : "text-hd2-faint"}>{label}: {t(present ? "merge.fileSet" : "merge.fileUnset")}</span>;
}

function MergeSummary({ summary }: { summary: PatchMergeSummary }) {
  const { t } = useI18n();
  return <p className="mb-0 mt-3 text-xs text-hd2-muted">{t("merge.summary", { conflicts: summary.conflictCount, duplicates: summary.duplicateCount, repairs: summary.repairedMetadataCount, resources: summary.resourceCount })}</p>;
}

function moveRecordToTarget<Source>(records: ManualMergeRecord<Source>[], sourceId: string, targetId: string) {
  const sourceIndex = records.findIndex((record) => record.id === sourceId);
  const targetIndex = records.findIndex((record) => record.id === targetId);
  if (sourceIndex < 0 || targetIndex < 0 || sourceIndex === targetIndex) return records;
  const next = [...records];
  const [record] = next.splice(sourceIndex, 1);
  next.splice(targetIndex, 0, record);
  return next;
}

function formatBytes(value: number): string {
  if (value < 1024 * 1024) return `${Math.ceil(value / 1024)} KiB`;
  return `${(value / (1024 * 1024)).toFixed(1)} MiB`;
}
