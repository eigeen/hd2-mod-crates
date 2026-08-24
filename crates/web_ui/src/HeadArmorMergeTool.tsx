import MergeTypeIcon from "@mui/icons-material/MergeType";
import UploadFileIcon from "@mui/icons-material/UploadFile";
import { Button, CircularProgress, FormControlLabel, MenuItem, Switch, TextField } from "@mui/material";
import { useCallback, useMemo, useRef, useState } from "react";
import { toast } from "sonner";
import {
  useI18n,
  headMergeCandidates,
  uniqueHeadMergeHash,
  type EquipmentOption,
  type PatchFiles,
  type Translate,
  type HeadArmorMergeSummary,
} from "@hd2-mod-tools/migrator-ui";
import { downloadRepatchedPatch, patchFilesFromList } from "./fileInputs";
import { inspectEquipmentContents, mergeHeadIntoArmor, patchSidecarRequirements } from "./wasmClient";

export function HeadArmorMergeTool() {
  const { t } = useI18n();
  const armorRef = useRef<PatchFiles | null>(null);
  const headRef = useRef<PatchFiles | null>(null);
  const [dualPatch, setDualPatch] = useState(false);
  const [armorName, setArmorName] = useState("");
  const [headName, setHeadName] = useState("");
  const [armorCandidates, setArmorCandidates] = useState<EquipmentOption[]>([]);
  const [helmetCandidates, setHelmetCandidates] = useState<EquipmentOption[]>([]);
  const [armorHelmets, setArmorHelmets] = useState<EquipmentOption[]>([]);
  const [headHelmets, setHeadHelmets] = useState<EquipmentOption[]>([]);
  const [armorHash, setArmorHash] = useState("");
  const [helmetHash, setHelmetHash] = useState("");
  const [busy, setBusy] = useState(false);
  const [summary, setSummary] = useState<HeadArmorMergeSummary | null>(null);
  const messages = useMemo(() => patchMessages(t), [t]);

  const importArmor = useCallback(async (files: FileList) => {
    try {
      const patch = await patchFilesFromList(files, messages, patchSidecarRequirements);
      armorRef.current = patch;
      setArmorName(patch.name);
      const inspection = await inspectEquipmentContents(patch);
      const armors = headMergeCandidates(inspection.sources, "Armor");
      const helmets = headMergeCandidates(inspection.sources, "Helmet");
      setArmorCandidates(armors);
      setArmorHelmets(helmets);
      setArmorHash(uniqueHeadMergeHash(armors));
      if (!dualPatch) {
        setHelmetCandidates(helmets);
        setHelmetHash(uniqueHeadMergeHash(helmets));
      }
      setSummary(null);
    } catch (error) {
      showError(error, t);
    }
  }, [dualPatch, messages, t]);

  const importHead = useCallback(async (files: FileList) => {
    try {
      const patch = await patchFilesFromList(files, messages, patchSidecarRequirements);
      headRef.current = patch;
      setHeadName(patch.name);
      const inspection = await inspectEquipmentContents(patch);
      const helmets = headMergeCandidates(inspection.sources, "Helmet");
      setHeadHelmets(helmets);
      setHelmetCandidates(helmets);
      setHelmetHash(uniqueHeadMergeHash(helmets));
      setSummary(null);
    } catch (error) {
      showError(error, t);
    }
  }, [messages, t]);

  const run = useCallback(async () => {
    const armor = armorRef.current;
    if (!armor) return;
    setBusy(true);
    try {
      const result = await mergeHeadIntoArmor(
        armor,
        dualPatch ? headRef.current : null,
        { armorHash, helmetHash },
      );
      downloadRepatchedPatch(result.patch, result.patch.toc, "hd2-head-merged-mod.zip");
      setSummary(result.summary);
      toast.success(t("headMerge.completed", { count: result.summary.mergedLods }));
    } catch (error) {
      showError(error, t);
    } finally {
      setBusy(false);
    }
  }, [armorHash, dualPatch, helmetHash, t]);

  const canRun = Boolean(armorName && armorHash && helmetHash && (!dualPatch || headName));
  return (
    <section className="border-t border-hd2-border bg-black/20 p-5 min-[51.25rem]:p-6">
      <FormControlLabel
        control={<Switch checked={dualPatch} onChange={(_, checked) => {
          setDualPatch(checked);
          setSummary(null);
          const helmets = checked ? headHelmets : armorHelmets;
          setHelmetCandidates(helmets);
          setHelmetHash(uniqueHeadMergeHash(helmets));
        }} />}
        label={t("headMerge.dualPatch")}
      />
      <div className="mt-3 grid gap-4 min-[51.25rem]:grid-cols-2">
        <PatchPicker label={t("headMerge.armorMod")} name={armorName} onFiles={importArmor} />
        {dualPatch && <PatchPicker label={t("headMerge.headMod")} name={headName} onFiles={importHead} />}
      </div>
      <div className="mt-4 grid gap-4 min-[51.25rem]:grid-cols-2">
        <CandidateSelect label={t("headMerge.armorChoice")} options={armorCandidates} value={armorHash} onChange={setArmorHash} />
        <CandidateSelect label={t("headMerge.helmetChoice")} options={helmetCandidates} value={helmetHash} onChange={setHelmetHash} />
      </div>
      <Button className="!mt-5" disabled={!canRun || busy} onClick={() => void run()} startIcon={busy ? <CircularProgress size={16} /> : <MergeTypeIcon />} variant="contained">
        {busy ? t("headMerge.running") : t("headMerge.run")}
      </Button>
      {summary && <MergeSummary summary={summary} />}
    </section>
  );
}

function PatchPicker(props: { label: string; name: string; onFiles: (files: FileList) => void }) {
  return <div className="border border-dashed border-hd2-line p-4">
    <Button component="label" startIcon={<UploadFileIcon />} variant="outlined">
      {props.label}<input hidden multiple type="file" onChange={(event) => {
        if (event.target.files) props.onFiles(event.target.files);
        event.target.value = "";
      }} />
    </Button>
    <p className="mb-0 mt-2 truncate text-xs text-hd2-muted">{props.name || "—"}</p>
  </div>;
}

function CandidateSelect(props: { label: string; options: EquipmentOption[]; value: string; onChange: (value: string) => void }) {
  return <TextField fullWidth label={props.label} select value={props.value} onChange={(event) => props.onChange(event.target.value)}>
    {props.options.map((option) => <MenuItem key={option.hash} value={option.hash}>{option.name}</MenuItem>)}
  </TextField>;
}

function MergeSummary({ summary }: { summary: HeadArmorMergeSummary }) {
  const { t } = useI18n();
  return <div className="mt-5 border border-hd2-line p-4 text-sm text-hd2-muted">
    <p className="m-0 text-hd2-text">{t("headMerge.summary", { units: summary.mergedBodyUnits, lods: summary.mergedLods })}</p>
    {summary.mergedParts.map((part) => <p className="mb-0 mt-2 text-green-300" key={`${part.bodyVariant}-${part.lod}`}>✓ {part.bodyVariant} · LOD {part.lod}</p>)}
    {summary.skippedParts.map((part, index) => <p className="mb-0 mt-2" key={`${part.bodyVariant}-${part.lod}-${index}`}>{part.bodyVariant} · LOD {part.lod}: {part.reason}</p>)}
    {summary.warnings.map((warning, index) => <p className="mb-0 mt-2" key={index}>{warning}</p>)}
    <p className="mb-0 mt-3 text-hd2-yellow">{t("headMerge.removeHelmetHint")}</p>
  </div>;
}

function showError(error: unknown, t: Translate) {
  toast.error(t("headMerge.failed"), { description: error instanceof Error ? error.message : String(error) });
}

function patchMessages(t: Translate) {
  return {
    noToc: t("patch.noToc"), multipleToc: t("patch.multipleToc"), missingIntro: t("patch.missingIntro"),
    missingAction: (toc: string, gpu: string, stream: string) => t("patch.missingAction", { toc, gpu, stream }),
    missingSidecar: (filename: string, expected: string) => t("patch.missingSidecar", { filename, expected }),
    shortSidecar: (filename: string, expected: string, actual: number) => t("patch.shortSidecar", { filename, expected, actual }),
  };
}
