import DeleteIcon from "@mui/icons-material/Delete";
import MergeTypeIcon from "@mui/icons-material/MergeType";
import UploadFileIcon from "@mui/icons-material/UploadFile";
import { Button, CircularProgress, IconButton, MenuItem, TextField } from "@mui/material";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { toast } from "sonner";
import {
  combineEquipmentChoices,
  headMergeCandidates,
  headMergeVariantLabel,
  uniqueHeadMergeHash,
  useI18n,
  type EquipmentOption,
  type HeadArmorMergeSummary,
  type Translate,
} from "@hd2-mod-tools/migrator-ui";
import {
  chooseOutputZip,
  choosePatchPaths,
  inspectPatch,
  startHeadArmorMerge,
  type DesktopTask,
} from "./desktopClient";

interface DesktopMergeRecord {
  id: string;
  name: string;
  paths: string[];
  armors: EquipmentOption[];
  helmets: EquipmentOption[];
}

export function DesktopHeadArmorMergeTool({ dataDir }: { dataDir: string | null }) {
  const { t } = useI18n();
  const task = useRef<DesktopTask<HeadArmorMergeSummary> | null>(null);
  const [records, setRecords] = useState<DesktopMergeRecord[]>([]);
  const [armorHash, setArmorHash] = useState("");
  const [helmetHash, setHelmetHash] = useState("");
  const [busy, setBusy] = useState(false);
  const [summary, setSummary] = useState<HeadArmorMergeSummary | null>(null);

  const addRecord = useCallback(async () => {
    const paths = await choosePatchPaths();
    if (!paths) return;
    try {
      const result = await inspectPatch(paths, dataDir);
      const record: DesktopMergeRecord = {
        id: crypto.randomUUID(),
        name: result.patch.name,
        paths,
        armors: headMergeCandidates(result.inspection.sources, "Armor"),
        helmets: headMergeCandidates(result.inspection.sources, "Helmet"),
      };
      setRecords((current) => [...current, record]);
      setSummary(null);
    } catch (error) {
      showError(error, t);
    }
  }, [dataDir, t]);

  const combined = useMemo(
    () => combineEquipmentChoices(records.map((record) => ({
      sourceName: record.name,
      armors: record.armors,
      helmets: record.helmets,
    }))),
    [records],
  );

  // Auto-select when only one option exists; drop selections that are no longer available.
  useEffect(() => {
    setArmorHash((current) => keepOrAuto(current, combined.armors));
    setHelmetHash((current) => keepOrAuto(current, combined.helmets));
  }, [combined.armors, combined.helmets]);

  const removeRecord = useCallback((id: string) => {
    setRecords((current) => current.filter((record) => record.id !== id));
    setSummary(null);
  }, []);

  const run = useCallback(async () => {
    const armorRecord = records.find((record) =>
      record.armors.some((option) => option.hash === armorHash));
    const helmetRecord = records.find((record) =>
      record.helmets.some((option) => option.hash === helmetHash));
    if (!armorRecord || !helmetRecord) return;
    const outputPath = await chooseOutputZip("hd2-head-merged-mod.zip");
    if (!outputPath) return;
    setBusy(true);
    const active = startHeadArmorMerge({
      armorPaths: armorRecord.paths,
      dataDir,
      headPaths: helmetRecord.id !== armorRecord.id ? helmetRecord.paths : null,
      outputPath,
      options: { armorHash, helmetHash },
    }, () => undefined);
    task.current = active;
    try {
      const result = await active.result;
      setSummary(result);
      toast.success(t("headMerge.completed", { count: result.mergedLods }));
    } catch (error) {
      showError(error, t);
    } finally {
      task.current = null;
      setBusy(false);
    }
  }, [armorHash, dataDir, helmetHash, records, t]);

  const blocker = records.length === 0
    ? t("headMerge.needMods")
    : combined.armors.length === 0
      ? t("headMerge.noArmor")
      : combined.helmets.length === 0
        ? t("headMerge.noHelmet")
        : !armorHash || !helmetHash
          ? t("headMerge.needSelection")
          : null;
  const canRun = !busy && blocker === null;

  return (
    <section className="border-t border-hd2-border bg-black/20 p-5 min-[51.25rem]:p-6">
      <div className="flex flex-wrap items-center gap-3">
        <Button disabled={busy} onClick={() => void addRecord()} startIcon={<UploadFileIcon />} variant="contained">
          {t("headMerge.pick")}
        </Button>
      </div>

      <div className="mt-4 space-y-2">
        {records.length === 0 && (
          <p className="m-0 border border-hd2-line px-4 py-6 text-center text-sm text-hd2-muted">
            {t("headMerge.empty")}
          </p>
        )}
        {records.map((record, index) => (
          <HeadMergeRecordRow
            key={record.id}
            index={index}
            record={record}
            onRemove={() => removeRecord(record.id)}
          />
        ))}
      </div>

      <div className="mt-5 grid gap-4 min-[51.25rem]:grid-cols-2">
        <TextField
          disabled={busy}
          fullWidth
          label={t("headMerge.armorChoice")}
          onChange={(event) => setArmorHash(event.target.value)}
          select
          value={armorHash}
        >
          {combined.armors.map((option) => (
            <MenuItem key={option.hash} value={option.hash}>
              {option.name}{records.length > 1 && ` · ${t("headMerge.fromMod", { name: option.sourceName })}`}
            </MenuItem>
          ))}
        </TextField>
        <TextField
          disabled={busy}
          fullWidth
          label={t("headMerge.helmetChoice")}
          onChange={(event) => setHelmetHash(event.target.value)}
          select
          value={helmetHash}
        >
          {combined.helmets.map((option) => (
            <MenuItem key={option.hash} value={option.hash}>
              {option.name}{records.length > 1 && ` · ${t("headMerge.fromMod", { name: option.sourceName })}`}
            </MenuItem>
          ))}
        </TextField>
      </div>

      <div className="mt-5 flex flex-col items-start gap-3">
        <div className="flex gap-3">
          <Button
            disabled={!canRun}
            onClick={() => void run()}
            startIcon={busy ? <CircularProgress size="1rem" /> : <MergeTypeIcon />}
            variant="contained"
          >
            {busy ? t("headMerge.running") : t("headMerge.run")}
          </Button>
          {busy && (
            <Button color="error" onClick={() => void task.current?.cancel()} variant="outlined">
              {t("task.cancel")}
            </Button>
          )}
        </div>
        {blocker && <p className="mb-0 text-xs text-hd2-muted">{blocker}</p>}
      </div>
      {summary && <MergeSummary summary={summary} />}
    </section>
  );
}

interface HeadMergeRecordRowProps {
  index: number;
  record: DesktopMergeRecord;
  onRemove: () => void;
}

function HeadMergeRecordRow(props: HeadMergeRecordRowProps) {
  const { t } = useI18n();
  const { index, record, onRemove } = props;
  const detected = record.armors.length + record.helmets.length;
  return (
    <div className="flex items-center gap-2 border border-hd2-line bg-hd2-surface/90 px-2 py-2">
      <span className="w-6 shrink-0 text-center font-mono text-xs text-hd2-faint">{index + 1}</span>
      <div className="min-w-0 flex-1">
        <p className="m-0 truncate font-mono text-xs text-hd2-text" title={record.name}>
          {record.name}
        </p>
        <p className="m-0 mt-1 text-[0.6875rem] text-hd2-muted">
          {detected > 0
            ? t("headMerge.recordDetected", { armors: record.armors.length, helmets: record.helmets.length })
            : t("headMerge.recordNone")}
        </p>
      </div>
      <div className="flex shrink-0 items-center">
        <IconButton aria-label={t("merge.remove")} onClick={onRemove} size="small">
          <DeleteIcon fontSize="small" />
        </IconButton>
      </div>
    </div>
  );
}

function MergeSummary({ summary }: { summary: HeadArmorMergeSummary }) {
  const { t } = useI18n();
  return (
    <div className="mt-5 border border-hd2-line p-4 text-sm text-hd2-muted">
      <p className="m-0 text-hd2-text">
        {t("headMerge.summary", { units: summary.mergedBodyUnits, lods: summary.mergedLods })}
      </p>
      {summary.restoredBodyUnits > 0 && (
        <p className="mb-0 mt-2">
          {t("headMerge.restoredBodies", { count: summary.restoredBodyUnits })}
        </p>
      )}
      {summary.mergedParts.map((part) => (
        <p className="mb-0 mt-2 text-green-300" key={`${part.bodyVariant}-${part.lod}`}>
          ✓ {headMergeVariantLabel(part.bodyVariant, t)} · {t("headMerge.lodLabel")} {part.lod}
        </p>
      ))}
      {summary.skippedParts.map((part, index) => (
        <p className="mb-0 mt-2" key={`${part.bodyVariant}-${part.lod}-${index}`}>
          {headMergeVariantLabel(part.bodyVariant, t)} · {t("headMerge.lodLabel")} {part.lod}: {part.reason}
        </p>
      ))}
      {summary.warnings.map((warning, index) => (
        <p className="mb-0 mt-2" key={index}>{warning}</p>
      ))}
    </div>
  );
}

function keepOrAuto(current: string, options: EquipmentOption[]): string {
  if (current && options.some((option) => option.hash === current)) return current;
  return uniqueHeadMergeHash(options);
}

function showError(error: unknown, t: Translate) {
  toast.error(t("headMerge.failed"), { description: error instanceof Error ? error.message : String(error) });
}
