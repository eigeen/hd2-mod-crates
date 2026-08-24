import MergeTypeIcon from "@mui/icons-material/MergeType";
import UploadFileIcon from "@mui/icons-material/UploadFile";
import { Button, CircularProgress, FormControlLabel, MenuItem, Switch, TextField } from "@mui/material";
import { useCallback, useRef, useState } from "react";
import { toast } from "sonner";
import { headMergeCandidates, uniqueHeadMergeHash, useI18n, type EquipmentOption, type HeadArmorMergeSummary } from "@hd2-mod-tools/migrator-ui";
import { chooseOutputZip, choosePatchPaths, inspectPatch, startHeadArmorMerge, type DesktopTask } from "./desktopClient";

export function DesktopHeadArmorMergeTool() {
  const { t } = useI18n();
  const armorPaths = useRef<string[]>([]);
  const headPaths = useRef<string[]>([]);
  const task = useRef<DesktopTask<HeadArmorMergeSummary> | null>(null);
  const [dualPatch, setDualPatch] = useState(false);
  const [armorName, setArmorName] = useState("");
  const [headName, setHeadName] = useState("");
  const [armorOptions, setArmorOptions] = useState<EquipmentOption[]>([]);
  const [helmetOptions, setHelmetOptions] = useState<EquipmentOption[]>([]);
  const [armorHelmets, setArmorHelmets] = useState<EquipmentOption[]>([]);
  const [headHelmets, setHeadHelmets] = useState<EquipmentOption[]>([]);
  const [armorHash, setArmorHash] = useState("");
  const [helmetHash, setHelmetHash] = useState("");
  const [busy, setBusy] = useState(false);
  const [summary, setSummary] = useState<HeadArmorMergeSummary | null>(null);

  const chooseArmor = useCallback(async () => {
    const paths = await choosePatchPaths();
    if (!paths) return;
    try {
      const result = await inspectPatch(paths, null);
      armorPaths.current = paths;
      setArmorName(result.patch.name);
      const armors = headMergeCandidates(result.inspection.sources, "Armor");
      const helmets = headMergeCandidates(result.inspection.sources, "Helmet");
      setArmorOptions(armors);
      setArmorHelmets(helmets);
      setArmorHash(uniqueHeadMergeHash(armors));
      if (!dualPatch) {
        setHelmetOptions(helmets);
        setHelmetHash(uniqueHeadMergeHash(helmets));
      }
      setSummary(null);
    } catch (error) { showError(error, t("headMerge.failed")); }
  }, [dualPatch, t]);

  const chooseHead = useCallback(async () => {
    const paths = await choosePatchPaths();
    if (!paths) return;
    try {
      const result = await inspectPatch(paths, null);
      headPaths.current = paths;
      setHeadName(result.patch.name);
      const helmets = headMergeCandidates(result.inspection.sources, "Helmet");
      setHeadHelmets(helmets);
      setHelmetOptions(helmets);
      setHelmetHash(uniqueHeadMergeHash(helmets));
      setSummary(null);
    } catch (error) { showError(error, t("headMerge.failed")); }
  }, [t]);

  const run = useCallback(async () => {
    const outputPath = await chooseOutputZip("hd2-head-merged-mod.zip");
    if (!outputPath) return;
    setBusy(true);
    const active = startHeadArmorMerge({
      armorPaths: armorPaths.current,
      headPaths: dualPatch ? headPaths.current : null,
      outputPath,
      options: { armorHash, helmetHash },
    }, () => undefined);
    task.current = active;
    try {
      const result = await active.result;
      setSummary(result);
      toast.success(t("headMerge.completed", { count: result.mergedLods }));
    } catch (error) { showError(error, t("headMerge.failed")); }
    finally { task.current = null; setBusy(false); }
  }, [armorHash, dualPatch, helmetHash, t]);

  const canRun = Boolean(armorName && armorHash && helmetHash && (!dualPatch || headName));
  return <section className="border-t border-hd2-border bg-black/20 p-5 min-[51.25rem]:p-6">
    <FormControlLabel control={<Switch checked={dualPatch} onChange={(_, checked) => {
      setDualPatch(checked); setSummary(null);
      const helmets = checked ? headHelmets : armorHelmets;
      setHelmetOptions(helmets); setHelmetHash(uniqueHeadMergeHash(helmets));
    }} />} label={t("headMerge.dualPatch")} />
    <div className="mt-3 grid gap-4 min-[51.25rem]:grid-cols-2">
      <Picker label={t("headMerge.armorMod")} name={armorName} onClick={chooseArmor} />
      {dualPatch && <Picker label={t("headMerge.headMod")} name={headName} onClick={chooseHead} />}
    </div>
    <div className="mt-4 grid gap-4 min-[51.25rem]:grid-cols-2">
      <Select label={t("headMerge.armorChoice")} options={armorOptions} value={armorHash} onChange={setArmorHash} />
      <Select label={t("headMerge.helmetChoice")} options={helmetOptions} value={helmetHash} onChange={setHelmetHash} />
    </div>
    <div className="mt-5 flex gap-3">
      <Button disabled={!canRun || busy} onClick={() => void run()} startIcon={busy ? <CircularProgress size={16} /> : <MergeTypeIcon />} variant="contained">{busy ? t("headMerge.running") : t("headMerge.run")}</Button>
      {busy && <Button color="error" onClick={() => void task.current?.cancel()} variant="outlined">{t("task.cancel")}</Button>}
    </div>
    {summary && <div className="mt-5 border border-hd2-line p-4 text-sm text-hd2-muted">
      <p className="m-0 text-hd2-text">{t("headMerge.summary", { units: summary.mergedBodyUnits, lods: summary.mergedLods })}</p>
      {summary.mergedParts.map((part) => <p className="mb-0 mt-2 text-green-300" key={`${part.bodyVariant}-${part.lod}`}>✓ {part.bodyVariant} · LOD {part.lod}</p>)}
      {summary.skippedParts.map((part, index) => <p className="mb-0 mt-2" key={`${part.bodyVariant}-${part.lod}-${index}`}>{part.bodyVariant} · LOD {part.lod}: {part.reason}</p>)}
      {summary.warnings.map((warning, index) => <p className="mb-0 mt-2" key={index}>{warning}</p>)}
      <p className="mb-0 mt-3 text-hd2-yellow">{t("headMerge.removeHelmetHint")}</p>
    </div>}
  </section>;
}

function Picker(props: { label: string; name: string; onClick: () => Promise<void> }) {
  return <div className="border border-dashed border-hd2-line p-4"><Button onClick={() => void props.onClick()} startIcon={<UploadFileIcon />} variant="outlined">{props.label}</Button><p className="mb-0 mt-2 truncate text-xs text-hd2-muted">{props.name || "—"}</p></div>;
}

function Select(props: { label: string; options: EquipmentOption[]; value: string; onChange: (value: string) => void }) {
  return <TextField fullWidth label={props.label} select value={props.value} onChange={(event) => props.onChange(event.target.value)}>{props.options.map((option) => <MenuItem key={option.hash} value={option.hash}>{option.name}</MenuItem>)}</TextField>;
}

function showError(error: unknown, title: string) { toast.error(title, { description: error instanceof Error ? error.message : String(error) }); }
