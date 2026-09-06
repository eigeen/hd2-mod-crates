import DeleteIcon from "@mui/icons-material/Delete";
import MergeTypeIcon from "@mui/icons-material/MergeType";
import UploadFileIcon from "@mui/icons-material/UploadFile";
import { Button, CircularProgress, IconButton, MenuItem, TextField } from "@mui/material";
import { useCallback, useEffect, useMemo, useState } from "react";
import { toast } from "sonner";
import {
  combineEquipmentChoices,
  headMergeVariantLabel,
  uniqueHeadMergeHash,
  useDropZone,
  useI18n,
  type EquipmentOption,
  type HeadArmorMergeSummary,
} from "@hd2-mod-tools/migrator-ui";
import { useTaskController } from "./taskController";
import type { MergeInputFile } from "./mergeInputs";

export interface HeadMergeRecord<Source> {
  armors: EquipmentOption[];
  helmets: EquipmentOption[];
  id: string;
  name: string;
  source: Source;
}

export interface HeadMergeAdapter<Source, FileSource = never> {
  browserFileReference?: (file: File) => MergeInputFile<FileSource>;
  cancellable: boolean;
  pickRecords: (signal: AbortSignal) => Promise<HeadMergeRecord<Source>[] | null>;
  recordsFromFiles?: (files: MergeInputFile<FileSource>[], signal: AbortSignal) => Promise<HeadMergeRecord<Source>[]>;
  subscribeToFileDrop?: (
    receive: (files: MergeInputFile<FileSource>[]) => void,
    reject: (error: unknown) => void,
  ) => () => void;
  runMerge: (request: {
    armorHash: string;
    armorRecord: HeadMergeRecord<Source>;
    helmetHash: string;
    helmetRecord: HeadMergeRecord<Source>;
    signal: AbortSignal;
  }) => Promise<{ output: string; summary: HeadArmorMergeSummary } | null>;
}

export function HeadArmorMergeTool<Source, FileSource>({ adapter }: { adapter: HeadMergeAdapter<Source, FileSource> }) {
  const { t } = useI18n();
  const [records, setRecords] = useState<HeadMergeRecord<Source>[]>([]);
  const [armorHash, setArmorHash] = useState("");
  const [helmetHash, setHelmetHash] = useState("");
  const [adding, setAdding] = useState(false);
  const task = useTaskController();
  const runTask = task.run;
  const [summary, setSummary] = useState<HeadArmorMergeSummary | null>(null);
  const combined = useMemo(() => combineEquipmentChoices(records.map((record) => ({
    sourceName: record.name,
    armors: record.armors,
    helmets: record.helmets,
  }))), [records]);

  useEffect(() => {
    setArmorHash((current) => keepOrAuto(current, combined.armors));
    setHelmetHash((current) => keepOrAuto(current, combined.helmets));
  }, [combined.armors, combined.helmets]);

  const addRecords = useCallback((added: HeadMergeRecord<Source>[]) => {
    if (!added.length) return;
    setRecords((current) => [...current, ...added]);
    setSummary(null);
  }, []);

  const pickRecords = useCallback(async () => {
    setAdding(true);
    await runTask({
      execute: (signal) => adapter.pickRecords(signal),
      fallbackErrorCode: "headMerge.failed",
      onSuccess: addRecords,
    });
    setAdding(false);
  }, [adapter, addRecords, runTask]);

  const receiveFiles = useCallback(async (files: MergeInputFile<FileSource>[]) => {
    if (!adapter.recordsFromFiles) return;
    setAdding(true);
    await runTask({
      execute: (signal) => adapter.recordsFromFiles!(files, signal),
      fallbackErrorCode: "headMerge.failed",
      onSuccess: addRecords,
    });
    setAdding(false);
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
    (error) => { void runTask({ execute: () => Promise.reject(error), fallbackErrorCode: "headMerge.failed" }); },
  ), [adapter, receiveFiles, runTask]);
  const blocker = headMergeBlocker(records, combined, armorHash, helmetHash, t);
  const canRun = !task.busy && !adding && blocker === null;

  const run = useCallback(async () => {
    const armorRecord = records.find((record) => record.armors.some((option) => option.hash === armorHash));
    const helmetRecord = records.find((record) => record.helmets.some((option) => option.hash === helmetHash));
    if (!armorRecord || !helmetRecord) return;
    await runTask({
      acceptResultAfterCancel: true,
      execute: (signal) => adapter.runMerge({ armorHash, armorRecord, helmetHash, helmetRecord, signal }),
      fallbackErrorCode: "headMerge.failed",
      onSuccess: (result) => {
        setSummary(result.summary);
        toast.success(t("headMerge.completed", { count: result.summary.mergedLods }));
      },
      report: (result) => ({ kind: "headMerge", ...result }),
    });
  }, [adapter, armorHash, helmetHash, records, runTask, t]);

  return (
    <section className="border-t border-hd2-border bg-black/20 p-5 min-[51.25rem]:p-6">
      <div className={dropEnabled ? `border border-dashed px-4 py-4 ${dropZone.dragging ? "border-hd2-yellow bg-hd2-yellow-bg" : "border-hd2-line"}` : ""} {...(browserDropEnabled ? dropZone.handlers : {})}>
        <div className="flex flex-wrap items-center gap-3">
          <Button disabled={adding || task.busy} onClick={() => void pickRecords()} startIcon={adding ? <CircularProgress size="1rem" /> : <UploadFileIcon />} variant="contained">{t("headMerge.pick")}</Button>
          {dropEnabled && <span className="text-xs text-hd2-muted">{dropZone.dragging ? t("headMerge.dropActive") : t("headMerge.dropHint")}</span>}
        </div>
      </div>
      <HeadMergeRecordList busy={task.busy} records={records} removeRecord={(id) => {
        setRecords((current) => current.filter((record) => record.id !== id));
        setSummary(null);
      }} />
      <div className="mt-5 grid gap-4 min-[51.25rem]:grid-cols-2">
        <EquipmentChoice disabled={task.busy} label={t("headMerge.armorChoice")} onChange={setArmorHash} options={combined.armors} records={records.length} value={armorHash} />
        <EquipmentChoice disabled={task.busy} label={t("headMerge.helmetChoice")} onChange={setHelmetHash} options={combined.helmets} records={records.length} value={helmetHash} />
      </div>
      <div className="mt-5 flex flex-col items-start gap-3">
        <div className="flex gap-3">
          <Button disabled={!canRun} onClick={() => void run()} startIcon={task.busy ? <CircularProgress size="1rem" /> : <MergeTypeIcon />} variant="contained">{task.busy ? t("headMerge.running") : t("headMerge.run")}</Button>
          {task.busy && adapter.cancellable && <Button color="error" onClick={task.cancel} variant="outlined">{t("task.cancel")}</Button>}
        </div>
        {blocker && <p className="mb-0 text-xs text-hd2-muted">{blocker}</p>}
      </div>
      {summary && <MergeSummary summary={summary} />}
    </section>
  );
}

function HeadMergeRecordList<Source>({ busy, records, removeRecord }: {
  busy: boolean;
  records: HeadMergeRecord<Source>[];
  removeRecord: (id: string) => void;
}) {
  const { t } = useI18n();
  if (!records.length) return <p className="m-0 mt-4 border border-hd2-line px-4 py-6 text-center text-sm text-hd2-muted">{t("headMerge.empty")}</p>;
  return <div className="mt-4 space-y-2">{records.map((record, index) => <HeadMergeRecordRow busy={busy} index={index} key={record.id} onRemove={() => removeRecord(record.id)} record={record} />)}</div>;
}

function HeadMergeRecordRow<Source>({ busy, index, onRemove, record }: {
  busy: boolean;
  index: number;
  onRemove: () => void;
  record: HeadMergeRecord<Source>;
}) {
  const { t } = useI18n();
  const detected = record.armors.length + record.helmets.length;
  return (
    <div className="flex items-center gap-2 border border-hd2-line bg-hd2-surface/90 px-2 py-2">
      <span className="w-6 shrink-0 text-center font-mono text-xs text-hd2-faint">{index + 1}</span>
      <div className="min-w-0 flex-1">
        <p className="m-0 truncate font-mono text-xs text-hd2-text" title={record.name}>{record.name}</p>
        <p className="m-0 mt-1 text-[0.6875rem] text-hd2-muted">{detected > 0 ? t("headMerge.recordDetected", { armors: record.armors.length, helmets: record.helmets.length }) : t("headMerge.recordNone")}</p>
      </div>
      <IconButton aria-label={t("merge.remove")} disabled={busy} onClick={onRemove} size="small"><DeleteIcon fontSize="small" /></IconButton>
    </div>
  );
}

function EquipmentChoice({ disabled, label, onChange, options, records, value }: {
  disabled: boolean;
  label: string;
  onChange: (hash: string) => void;
  options: ReturnType<typeof combineEquipmentChoices>["armors"];
  records: number;
  value: string;
}) {
  const { t } = useI18n();
  return (
    <TextField disabled={disabled} fullWidth label={label} onChange={(event) => onChange(event.target.value)} select value={value}>
      {options.map((option) => <MenuItem key={`${option.hash}-${option.sourceName}`} value={option.hash}>{option.name}{records > 1 && ` · ${t("headMerge.fromMod", { name: option.sourceName })}`}</MenuItem>)}
    </TextField>
  );
}

function MergeSummary({ summary }: { summary: HeadArmorMergeSummary }) {
  const { t } = useI18n();
  return (
    <div className="mt-5 border border-hd2-line p-4 text-sm text-hd2-muted">
      <p className="m-0 text-hd2-text">{t("headMerge.summary", { units: summary.mergedBodyUnits, lods: summary.mergedLods })}</p>
      {summary.mergedParts.map((part) => <p className="mb-0 mt-2 text-green-300" key={`${part.bodyVariant}-${part.lod}`}>✓ {headMergeVariantLabel(part.bodyVariant, t)} · {t("headMerge.lodLabel")} {part.lod}</p>)}
      {summary.skippedParts.map((part, index) => <p className="mb-0 mt-2" key={`${part.bodyVariant}-${part.lod}-${index}`}>{headMergeVariantLabel(part.bodyVariant, t)} · {t("headMerge.lodLabel")} {part.lod}: {part.reason}</p>)}
      {summary.warnings.map((warning, index) => <p className="mb-0 mt-2" key={index}>{warning}</p>)}
    </div>
  );
}

function headMergeBlocker<Source>(
  records: HeadMergeRecord<Source>[],
  combined: ReturnType<typeof combineEquipmentChoices>,
  armorHash: string,
  helmetHash: string,
  t: ReturnType<typeof useI18n>["t"],
): string | null {
  if (!records.length) return t("headMerge.needMods");
  if (!combined.armors.length) return t("headMerge.noArmor");
  if (!combined.helmets.length) return t("headMerge.noHelmet");
  if (!armorHash || !helmetHash) return t("headMerge.needSelection");
  return null;
}

function keepOrAuto(current: string, options: EquipmentOption[]): string {
  if (current && options.some((option) => option.hash === current)) return current;
  return uniqueHeadMergeHash(options);
}
