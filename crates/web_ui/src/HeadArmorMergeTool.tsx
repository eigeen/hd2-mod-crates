import DeleteIcon from "@mui/icons-material/Delete";
import MergeTypeIcon from "@mui/icons-material/MergeType";
import UploadFileIcon from "@mui/icons-material/UploadFile";
import { Button, CircularProgress, IconButton, MenuItem, TextField } from "@mui/material";
import { useCallback, useEffect, useMemo, useState } from "react";
import { toast } from "sonner";
import {
  combineEquipmentChoices,
  headMergeCandidates,
  headMergeVariantLabel,
  uniqueHeadMergeHash,
  useDropZone,
  useI18n,
  type EquipmentOption,
  type HeadArmorMergeSummary,
  type PatchFiles,
  type Translate,
} from "@hd2-mod-tools/migrator-ui";
import { downloadRepatchedPatch } from "./fileInputs";
import {
  appendManualMergeBatch,
  mergeInputErrorMessage,
  type ManualMergeRecord,
} from "./manualMergeInputs";
import { inspectEquipmentContents, mergeHeadIntoArmor } from "./wasmClient";
import type { GameDataSource } from "./gameDataSource";

interface ParsedRecord {
  armors: EquipmentOption[];
  helmets: EquipmentOption[];
}

export function HeadArmorMergeTool({ dataSource }: { dataSource?: GameDataSource }) {
  const { t } = useI18n();
  const [records, setRecords] = useState<ManualMergeRecord[]>([]);
  const [parsed, setParsed] = useState<Record<string, ParsedRecord>>({});
  const [parsing, setParsing] = useState<Record<string, boolean>>({});
  const [failed, setFailed] = useState<Record<string, string>>({});
  const [armorHash, setArmorHash] = useState("");
  const [helmetHash, setHelmetHash] = useState("");
  const [busy, setBusy] = useState(false);
  const [summary, setSummary] = useState<HeadArmorMergeSummary | null>(null);

  const parseRecord = useCallback(async (record: ManualMergeRecord) => {
    setParsing((current) => ({ ...current, [record.id]: true }));
    try {
      const patch = await readRecordToc(record);
      const inspection = await inspectEquipmentContents(patch, dataSource);
      setParsed((current) => ({
        ...current,
        [record.id]: {
          armors: headMergeCandidates(inspection.sources, "Armor"),
          helmets: headMergeCandidates(inspection.sources, "Helmet"),
        },
      }));
      setFailed((current) => dropKey(current, record.id));
    } catch (error) {
      setFailed((current) => ({ ...current, [record.id]: errorMessage(error) }));
    } finally {
      setParsing((current) => dropKey(current, record.id));
    }
  }, [dataSource]);

  const addFiles = useCallback((files: FileList | File[]) => {
    try {
      const next = appendManualMergeBatch(records, files);
      const added = next.filter((record) => !records.some((old) => old.id === record.id));
      setRecords(next);
      setSummary(null);
      for (const record of added) void parseRecord(record);
    } catch (error) {
      toast.error(mergeInputErrorMessage(error, t));
    }
  }, [parseRecord, records, t]);

  const receiveDrop = useCallback((transfer: DataTransfer) => addFiles(transfer.files), [addFiles]);
  const dropZone = useDropZone(receiveDrop);

  const combined = useMemo(
    () => combineEquipmentChoices(records.map((record) => {
      const info = parsed[record.id];
      return {
        sourceName: record.name,
        armors: info?.armors ?? [],
        helmets: info?.helmets ?? [],
      };
    })),
    [parsed, records],
  );

  // Auto-select when only one option exists; drop selections that are no longer available.
  useEffect(() => {
    setArmorHash((current) => keepOrAuto(current, combined.armors));
    setHelmetHash((current) => keepOrAuto(current, combined.helmets));
  }, [combined.armors, combined.helmets]);

  const removeRecord = useCallback((id: string) => {
    setRecords((current) => current.filter((record) => record.id !== id));
    setParsed((current) => dropKey(current, id));
    setFailed((current) => dropKey(current, id));
    setSummary(null);
  }, []);

  const run = useCallback(async () => {
    const armorRecord = records.find((record) =>
      parsed[record.id]?.armors.some((option) => option.hash === armorHash));
    const helmetRecord = records.find((record) =>
      parsed[record.id]?.helmets.some((option) => option.hash === helmetHash));
    if (!armorRecord || !helmetRecord) return;
    setBusy(true);
    try {
      const armorPatch = await readRecordPatch(armorRecord);
      const headPatch = helmetRecord.id !== armorRecord.id ? await readRecordPatch(helmetRecord) : null;
      const result = await mergeHeadIntoArmor(
        armorPatch,
        headPatch,
        { armorHash, helmetHash },
        dataSource,
      );
      downloadRepatchedPatch(result.patch, result.patch.toc, "hd2-head-merged-mod.zip");
      setSummary(result.summary);
      toast.success(t("headMerge.completed", { count: result.summary.mergedLods }));
    } catch (error) {
      showError(error, t);
    } finally {
      setBusy(false);
    }
  }, [armorHash, dataSource, helmetHash, parsed, records, t]);

  const anyParsing = records.some((record) => Boolean(parsing[record.id]));
  const blocker = !anyParsing
    ? records.length === 0
      ? t("headMerge.needMods")
      : combined.armors.length === 0
        ? t("headMerge.noArmor")
        : combined.helmets.length === 0
          ? t("headMerge.noHelmet")
          : !armorHash || !helmetHash
            ? t("headMerge.needSelection")
            : null
    : null;
  const canRun = !busy && !anyParsing && blocker === null;

  return (
    <section className="border-t border-hd2-border bg-black/20 p-5 min-[51.25rem]:p-6">
      <div
        className={`border border-dashed px-4 py-4 ${dropZone.dragging ? "border-hd2-yellow bg-hd2-yellow-bg" : "border-hd2-line"}`}
        {...dropZone.handlers}
      >
        <div className="flex flex-wrap items-center gap-3">
          <Button component="label" startIcon={<UploadFileIcon />} variant="contained">
            {t("headMerge.pick")}
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
            {dropZone.dragging ? t("headMerge.dropActive") : t("headMerge.dropHint")}
          </span>
        </div>
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
            info={parsed[record.id]}
            parsing={Boolean(parsing[record.id])}
            failed={failed[record.id]}
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
        <Button
          disabled={!canRun}
          onClick={() => void run()}
          startIcon={busy ? <CircularProgress size="1rem" /> : <MergeTypeIcon />}
          variant="contained"
        >
          {busy ? t("headMerge.running") : t("headMerge.run")}
        </Button>
        {blocker && <p className="mb-0 text-xs text-hd2-muted">{blocker}</p>}
      </div>
      {summary && <MergeSummary summary={summary} />}
    </section>
  );
}

interface HeadMergeRecordRowProps {
  index: number;
  record: ManualMergeRecord;
  info?: ParsedRecord;
  parsing: boolean;
  failed?: string;
  onRemove: () => void;
}

function HeadMergeRecordRow(props: HeadMergeRecordRowProps) {
  const { t } = useI18n();
  const { index, record, info, parsing, failed, onRemove } = props;
  return (
    <div className="flex items-center gap-2 border border-hd2-line bg-hd2-surface/90 px-2 py-2">
      <span className="w-6 shrink-0 text-center font-mono text-xs text-hd2-faint">{index + 1}</span>
      <div className="min-w-0 flex-1">
        <p className="m-0 truncate font-mono text-xs text-hd2-text" title={record.name}>
          {record.name}
        </p>
        <p className="m-0 mt-1 flex flex-wrap gap-x-3 gap-y-1 text-[0.6875rem] text-hd2-muted">
          <FileState label="TOC" present={Boolean(record.toc)} />
          <FileState label="GPU" present={Boolean(record.gpu)} />
          <FileState label="Stream" present={Boolean(record.stream)} />
          {parsing && (
            <span className="inline-flex items-center gap-1 text-hd2-yellow">
              <CircularProgress size="0.625rem" />
              {t("headMerge.parsing")}
            </span>
          )}
          {!parsing && failed && <span className="text-red-300">{failed}</span>}
          {!parsing && !failed && info && (
            info.armors.length + info.helmets.length > 0
              ? t("headMerge.recordDetected", { armors: info.armors.length, helmets: info.helmets.length })
              : t("headMerge.recordNone")
          )}
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

function FileState({ label, present }: { label: string; present: boolean }) {
  const { t } = useI18n();
  return (
    <span className={present ? "text-green-300" : "text-hd2-faint"}>
      {label}: {t(present ? "merge.fileSet" : "merge.fileUnset")}
    </span>
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

function dropKey<T>(record: Record<string, T>, key: string): Record<string, T> {
  const next = { ...record };
  delete next[key];
  return next;
}

function showError(error: unknown, t: Translate) {
  toast.error(t("headMerge.failed"), { description: error instanceof Error ? error.message : String(error) });
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

async function readRecordToc(record: ManualMergeRecord): Promise<PatchFiles> {
  const toc = record.toc;
  if (!toc) throw new Error(`Missing patch main file: ${record.name}`);
  // 识别装备只需要 TOC；gpu/stream 可能有数百 MB，留到合并时再读。
  return {
    name: record.name,
    toc: new Uint8Array(await toc.arrayBuffer()),
    gpu: new Uint8Array(),
    stream: new Uint8Array(),
  };
}

async function readRecordPatch(record: ManualMergeRecord): Promise<PatchFiles> {
  const toc = record.toc;
  if (!toc) throw new Error(`Missing patch main file: ${record.name}`);
  return {
    name: record.name,
    toc: new Uint8Array(await toc.arrayBuffer()),
    gpu: record.gpu ? new Uint8Array(await record.gpu.arrayBuffer()) : new Uint8Array(),
    stream: record.stream ? new Uint8Array(await record.stream.arrayBuffer()) : new Uint8Array(),
  };
}
