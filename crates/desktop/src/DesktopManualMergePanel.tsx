import ArrowDownwardIcon from "@mui/icons-material/ArrowDownward";
import ArrowUpwardIcon from "@mui/icons-material/ArrowUpward";
import DeleteIcon from "@mui/icons-material/Delete";
import MergeTypeIcon from "@mui/icons-material/MergeType";
import UploadFileIcon from "@mui/icons-material/UploadFile";
import { Button, CircularProgress, IconButton, TextField } from "@mui/material";
import {
  useCallback,
  useRef,
  useState,
  type Dispatch,
  type SetStateAction,
} from "react";
import { toast } from "sonner";
import {
  useI18n,
  type PatchMergeSummary,
  type Translate,
} from "@hd2-mod-tools/migrator-ui";
import {
  chooseOutputZip,
  choosePatchPaths,
  describePatch,
  startPatchMerge,
  type DesktopTask,
} from "./desktopClient";
import {
  moveDesktopMergeRecord,
  type DesktopMergeRecord,
} from "./desktopMergeRecords";

export function DesktopManualMergePanel() {
  const { t } = useI18n();
  const task = useRef<DesktopTask<PatchMergeSummary> | null>(null);
  const [records, setRecords] = useState<DesktopMergeRecord[]>([]);
  const [outputName, setOutputName] = useState("");
  const [adding, setAdding] = useState(false);
  const [busy, setBusy] = useState(false);
  const [summary, setSummary] = useState<PatchMergeSummary | null>(null);

  const addRecord = useCallback(async () => {
    const paths = await choosePatchPaths();
    if (!paths) return;
    setAdding(true);
    try {
      const patch = await describePatch(paths);
      setRecords((current) => [...current, createRecord(patch, paths)]);
      setOutputName((current) => current || patch.name);
      setSummary(null);
    } catch (error) {
      showError(error, t);
    } finally {
      setAdding(false);
    }
  }, [t]);

  const runMerge = useCallback(async () => {
    const outputPath = await chooseOutputZip("hd2-merged-mod.zip");
    if (!outputPath) return;
    setBusy(true);
    const active = startPatchMerge({
      inputPaths: records.map((record) => record.paths),
      outputName: outputName.trim(),
      outputPath,
    }, () => undefined);
    task.current = active;
    await finishMerge(active, setSummary, t).finally(() => {
      task.current = null;
      setBusy(false);
    });
  }, [outputName, records, t]);

  const canMerge = records.length > 0 && outputName.trim() !== "";
  return (
    <section className="border-t border-hd2-border bg-black/20 p-5 min-[51.25rem]:p-6">
      <Button
        disabled={adding || busy}
        onClick={() => void addRecord()}
        startIcon={adding ? <CircularProgress size="1rem" /> : <UploadFileIcon />}
        variant="contained"
      >
        {t("merge.pick")}
      </Button>
      <MergeRecordList
        busy={busy}
        records={records}
        summary={summary}
        onMove={(id, offset) => {
          setRecords((current) => moveDesktopMergeRecord(current, id, offset));
          setSummary(null);
        }}
        onRemove={(id) => removeRecord(id, setRecords, setSummary)}
      />
      <div className="mt-5 flex flex-col gap-3 border-t border-hd2-border pt-4 min-[40rem]:flex-row min-[40rem]:items-center">
        <TextField
          disabled={busy}
          fullWidth
          label={t("merge.outputName")}
          onChange={(event) => setOutputName(event.target.value)}
          size="small"
          value={outputName}
        />
        <MergeActions
          busy={busy}
          canMerge={canMerge}
          onCancel={() => void task.current?.cancel()}
          onRun={() => void runMerge()}
        />
      </div>
      {summary && <MergeSummary summary={summary} />}
    </section>
  );
}

interface MergeRecordListProps {
  busy: boolean;
  records: DesktopMergeRecord[];
  summary: PatchMergeSummary | null;
  onMove: (id: string, offset: number) => void;
  onRemove: (id: string) => void;
}

function MergeRecordList(props: MergeRecordListProps) {
  const { t } = useI18n();
  if (!props.records.length) {
    return <p className="mb-0 mt-4 border border-hd2-line px-4 py-6 text-center text-sm text-hd2-muted">{t("merge.empty")}</p>;
  }
  return (
    <div className="mt-4 space-y-2">
      {props.records.map((record, index) => (
        <MergeRecordRow
          busy={props.busy}
          index={index}
          last={index === props.records.length - 1}
          key={record.id}
          onMove={props.onMove}
          onRemove={props.onRemove}
          record={record}
          sourceSummary={props.summary?.sources[index]}
        />
      ))}
    </div>
  );
}

interface MergeRecordRowProps {
  busy: boolean;
  index: number;
  last: boolean;
  record: DesktopMergeRecord;
  sourceSummary?: PatchMergeSummary["sources"][number];
  onMove: (id: string, offset: number) => void;
  onRemove: (id: string) => void;
}

function MergeRecordRow(props: MergeRecordRowProps) {
  const { t } = useI18n();
  return (
    <div className="flex items-center gap-2 border border-hd2-line bg-hd2-surface/90 px-2 py-2">
      <span className="w-6 shrink-0 text-center font-mono text-xs text-hd2-faint">{props.index + 1}</span>
      <div className="min-w-0 flex-1">
        <p className="m-0 truncate font-mono text-xs text-hd2-text" title={props.record.name}>{props.record.name}</p>
        <RecordDetails record={props.record} summary={props.sourceSummary} />
      </div>
      <IconButton disabled={props.busy || props.index === 0} aria-label={t("merge.moveUp")} onClick={() => props.onMove(props.record.id, -1)} size="small"><ArrowUpwardIcon fontSize="small" /></IconButton>
      <IconButton disabled={props.busy || props.last} aria-label={t("merge.moveDown")} onClick={() => props.onMove(props.record.id, 1)} size="small"><ArrowDownwardIcon fontSize="small" /></IconButton>
      <IconButton disabled={props.busy} aria-label={t("merge.remove")} onClick={() => props.onRemove(props.record.id)} size="small"><DeleteIcon fontSize="small" /></IconButton>
    </div>
  );
}

function RecordDetails({ record, summary }: { record: DesktopMergeRecord; summary?: PatchMergeSummary["sources"][number] }) {
  const { t } = useI18n();
  const details = summary ? t("merge.rowSummary", {
    repairs: summary.repairedMetadataCount,
    replaced: summary.replacedResourceCount,
    resources: summary.resourceCount,
  }) : formatBytes(record.byteLength);
  return <p className="m-0 mt-1 text-[0.6875rem] text-hd2-muted">{details}</p>;
}

function MergeActions(props: { busy: boolean; canMerge: boolean; onCancel: () => void; onRun: () => void }) {
  const { t } = useI18n();
  return props.busy ? (
    <Button color="error" className="shrink-0" onClick={props.onCancel} variant="outlined">{t("task.cancel")}</Button>
  ) : (
    <Button className="shrink-0" disabled={!props.canMerge} onClick={props.onRun} startIcon={<MergeTypeIcon />} variant="contained">{t("merge.save")}</Button>
  );
}

function MergeSummary({ summary }: { summary: PatchMergeSummary }) {
  const { t } = useI18n();
  return <p className="mb-0 mt-3 text-xs text-hd2-muted">{t("merge.summary", {
    conflicts: summary.conflictCount,
    repairs: summary.repairedMetadataCount,
    resources: summary.resourceCount,
  })}</p>;
}

function createRecord(patch: { name: string; byteLength: number }, paths: string[]): DesktopMergeRecord {
  return { byteLength: patch.byteLength, id: crypto.randomUUID(), name: patch.name, paths };
}

function removeRecord(
  id: string,
  setRecords: Dispatch<SetStateAction<DesktopMergeRecord[]>>,
  setSummary: Dispatch<SetStateAction<PatchMergeSummary | null>>,
) {
  setRecords((current) => current.filter((record) => record.id !== id));
  setSummary(null);
}

async function finishMerge(
  task: DesktopTask<PatchMergeSummary>,
  setSummary: (summary: PatchMergeSummary) => void,
  t: Translate,
) {
  try {
    const result = await task.result;
    setSummary(result);
    toast.success(t("merge.completed", { count: result.resourceCount }));
  } catch (error) {
    showError(error, t);
  }
}

function formatBytes(value: number): string {
  if (value < 1024 * 1024) return `${Math.ceil(value / 1024)} KiB`;
  return `${(value / (1024 * 1024)).toFixed(1)} MiB`;
}

function showError(error: unknown, t: Translate) {
  toast.error(t("merge.failed"), { description: error instanceof Error ? error.message : String(error) });
}
