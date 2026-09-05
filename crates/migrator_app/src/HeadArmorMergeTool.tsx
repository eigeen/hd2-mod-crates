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

export interface HeadMergeRecord<Source> {
  armors: EquipmentOption[];
  helmets: EquipmentOption[];
  id: string;
  name: string;
  source: Source;
}

export interface HeadMergeAdapter<Source> {
  cancellable: boolean;
  pickRecords: () => Promise<HeadMergeRecord<Source>[] | null>;
  recordsFromDrop?: (transfer: DataTransfer) => Promise<HeadMergeRecord<Source>[]>;
  runMerge: (request: {
    armorHash: string;
    armorRecord: HeadMergeRecord<Source>;
    helmetHash: string;
    helmetRecord: HeadMergeRecord<Source>;
    signal: AbortSignal;
  }) => Promise<{ output: string; summary: HeadArmorMergeSummary } | null>;
}

export function HeadArmorMergeTool<Source>({ adapter }: { adapter: HeadMergeAdapter<Source> }) {
  const { t } = useI18n();
  const [records, setRecords] = useState<HeadMergeRecord<Source>[]>([]);
  const [armorHash, setArmorHash] = useState("");
  const [helmetHash, setHelmetHash] = useState("");
  const [adding, setAdding] = useState(false);
  const [busy, setBusy] = useState(false);
  const [task, setTask] = useState<AbortController | null>(null);
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
    try {
      const selected = await adapter.pickRecords();
      if (selected) addRecords(selected);
    } catch (error) {
      showError(error, t("headMerge.failed"));
    } finally {
      setAdding(false);
    }
  }, [adapter, addRecords, t]);

  const receiveDrop = useCallback(async (transfer: DataTransfer) => {
    if (!adapter.recordsFromDrop) return;
    setAdding(true);
    try {
      addRecords(await adapter.recordsFromDrop(transfer));
    } catch (error) {
      showError(error, t("headMerge.failed"));
    } finally {
      setAdding(false);
    }
  }, [adapter, addRecords, t]);
  const dropZone = useDropZone(receiveDrop);
  const dropEnabled = Boolean(adapter.recordsFromDrop);
  const blocker = headMergeBlocker(records, combined, armorHash, helmetHash, t);
  const canRun = !busy && !adding && blocker === null;

  const run = useCallback(async () => {
    const armorRecord = records.find((record) => record.armors.some((option) => option.hash === armorHash));
    const helmetRecord = records.find((record) => record.helmets.some((option) => option.hash === helmetHash));
    if (!armorRecord || !helmetRecord) return;
    const controller = new AbortController();
    setTask(controller);
    setBusy(true);
    try {
      const result = await adapter.runMerge({ armorHash, armorRecord, helmetHash, helmetRecord, signal: controller.signal });
      if (!result) return;
      setSummary(result.summary);
      toast.success(t("headMerge.completed", { count: result.summary.mergedLods }));
    } catch (error) {
      showError(error, t("headMerge.failed"));
    } finally {
      setTask(null);
      setBusy(false);
    }
  }, [adapter, armorHash, helmetHash, records, t]);

  return (
    <section className="border-t border-hd2-border bg-black/20 p-5 min-[51.25rem]:p-6">
      <div className={dropEnabled ? `border border-dashed px-4 py-4 ${dropZone.dragging ? "border-hd2-yellow bg-hd2-yellow-bg" : "border-hd2-line"}` : ""} {...(dropEnabled ? dropZone.handlers : {})}>
        <div className="flex flex-wrap items-center gap-3">
          <Button disabled={adding || busy} onClick={() => void pickRecords()} startIcon={adding ? <CircularProgress size="1rem" /> : <UploadFileIcon />} variant="contained">{t("headMerge.pick")}</Button>
          {dropEnabled && <span className="text-xs text-hd2-muted">{dropZone.dragging ? t("headMerge.dropActive") : t("headMerge.dropHint")}</span>}
        </div>
      </div>
      <HeadMergeRecordList records={records} removeRecord={(id) => {
        setRecords((current) => current.filter((record) => record.id !== id));
        setSummary(null);
      }} />
      <div className="mt-5 grid gap-4 min-[51.25rem]:grid-cols-2">
        <EquipmentChoice disabled={busy} label={t("headMerge.armorChoice")} onChange={setArmorHash} options={combined.armors} records={records.length} value={armorHash} />
        <EquipmentChoice disabled={busy} label={t("headMerge.helmetChoice")} onChange={setHelmetHash} options={combined.helmets} records={records.length} value={helmetHash} />
      </div>
      <div className="mt-5 flex flex-col items-start gap-3">
        <div className="flex gap-3">
          <Button disabled={!canRun} onClick={() => void run()} startIcon={busy ? <CircularProgress size="1rem" /> : <MergeTypeIcon />} variant="contained">{busy ? t("headMerge.running") : t("headMerge.run")}</Button>
          {busy && adapter.cancellable && <Button color="error" onClick={() => task?.abort()} variant="outlined">{t("task.cancel")}</Button>}
        </div>
        {blocker && <p className="mb-0 text-xs text-hd2-muted">{blocker}</p>}
      </div>
      {summary && <MergeSummary summary={summary} />}
    </section>
  );
}

function HeadMergeRecordList<Source>({ records, removeRecord }: {
  records: HeadMergeRecord<Source>[];
  removeRecord: (id: string) => void;
}) {
  const { t } = useI18n();
  if (!records.length) return <p className="m-0 mt-4 border border-hd2-line px-4 py-6 text-center text-sm text-hd2-muted">{t("headMerge.empty")}</p>;
  return <div className="mt-4 space-y-2">{records.map((record, index) => <HeadMergeRecordRow index={index} key={record.id} onRemove={() => removeRecord(record.id)} record={record} />)}</div>;
}

function HeadMergeRecordRow<Source>({ index, onRemove, record }: {
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
      <IconButton aria-label={t("merge.remove")} onClick={onRemove} size="small"><DeleteIcon fontSize="small" /></IconButton>
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

function showError(error: unknown, title: string) {
  toast.error(title, { description: error instanceof Error ? error.message : String(error) });
}
