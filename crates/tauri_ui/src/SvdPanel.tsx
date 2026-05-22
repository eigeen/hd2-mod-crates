import { useCallback, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import ArchiveIcon from "@mui/icons-material/Archive";
import FileOpenIcon from "@mui/icons-material/FileOpen";
import FolderOpenIcon from "@mui/icons-material/FolderOpen";
import InventoryIcon from "@mui/icons-material/Inventory2";
import SaveIcon from "@mui/icons-material/Save";
import {
  Alert,
  Box,
  Button,
  Checkbox,
  CircularProgress,
  Divider,
  FormControlLabel,
  FormGroup,
  IconButton,
  Stack,
  TextField,
  Tooltip,
  Typography,
} from "@mui/material";
import type {
  SvdExportRequest,
  SvdExportSummary,
  SvdPackageSummary,
  SvdPackRequest,
  SvdPackSummary,
} from "./types";

interface PackState {
  inputDir: string;
  baseVariant: string;
  outputDir: string;
  packagePath: string;
  compressionLevel: string;
  jobs: string;
}

interface ExportState {
  packagePath: string;
  outputZip: string;
  allVariants: boolean;
  selectedVariants: string[];
  jobs: string;
}

type BusyTask = "pack" | "summary" | "export" | null;

const initialPack: PackState = {
  inputDir: "",
  baseVariant: "",
  outputDir: "",
  packagePath: "",
  compressionLevel: "3",
  jobs: "",
};

const initialExport: ExportState = {
  packagePath: "",
  outputZip: "",
  allVariants: false,
  selectedVariants: [],
  jobs: "",
};

function SvdPanel() {
  const [pack, setPack] = useState<PackState>(initialPack);
  const [exportForm, setExportForm] = useState<ExportState>(initialExport);
  const [packageSummary, setPackageSummary] = useState<SvdPackageSummary | null>(null);
  const [packSummary, setPackSummary] = useState<SvdPackSummary | null>(null);
  const [exportSummary, setExportSummary] = useState<SvdExportSummary | null>(null);
  const [busyTask, setBusyTask] = useState<BusyTask>(null);
  const [errorText, setErrorText] = useState("");

  const selectedCount = useMemo(
    () => selectedVariantCount(exportForm, packageSummary),
    [exportForm, packageSummary],
  );

  const runPack = useCallback(async () => {
    await runTask("pack", setBusyTask, setErrorText, async () => {
      const request = buildPackRequest(pack);
      setPackSummary(await invoke<SvdPackSummary>("run_svd_pack", { request }));
    });
  }, [pack]);

  const loadSummary = useCallback(async () => {
    await runTask("summary", setBusyTask, setErrorText, async () => {
      const summary = await invoke<SvdPackageSummary>("load_svd_package_summary", {
        packagePath: exportForm.packagePath,
      });
      setPackageSummary(summary);
      setExportForm((current) => ({ ...current, allVariants: false, selectedVariants: [] }));
    });
  }, [exportForm.packagePath]);

  const runExport = useCallback(async () => {
    await runTask("export", setBusyTask, setErrorText, async () => {
      const request = buildExportRequest(exportForm);
      setExportSummary(await invoke<SvdExportSummary>("run_svd_export", { request }));
    });
  }, [exportForm]);

  return (
    <Stack className="svdPanel" spacing={2}>
      <StatusAlert
        errorText={errorText}
        exportSummary={exportSummary}
        packSummary={packSummary}
        selectedCount={selectedCount}
      />
      <Box className="toolSection">
        <SectionTitle icon={<InventoryIcon />} title="Pack" />
        <Stack spacing={2}>
          <PathRow
            label="Source mod directory"
            onBrowse={() => chooseDirectory((value) => updatePack("inputDir", value, setPack))}
            value={pack.inputDir}
            onChange={(value) => updatePack("inputDir", value, setPack)}
          />
          <TextField
            fullWidth
            label="Base variant"
            onChange={(event) => updatePack("baseVariant", event.target.value, setPack)}
            size="small"
            value={pack.baseVariant}
          />
          <PathRow
            label="Package directory"
            onBrowse={() => chooseDirectory((value) => updatePack("outputDir", value, setPack))}
            value={pack.outputDir}
            onChange={(value) => updatePack("outputDir", value, setPack)}
          />
          <PathRow
            icon="save"
            label="SVD file"
            onBrowse={() => choosePackageSave((value) => updatePack("packagePath", value, setPack))}
            value={pack.packagePath}
            onChange={(value) => updatePack("packagePath", value, setPack)}
          />
          <Box className="numericGrid">
            <TextField
              label="Compression level"
              onChange={(event) => updatePack("compressionLevel", event.target.value, setPack)}
              size="small"
              type="number"
              value={pack.compressionLevel}
            />
            <TextField
              label="Jobs"
              onChange={(event) => updatePack("jobs", event.target.value, setPack)}
              size="small"
              type="number"
              value={pack.jobs}
            />
          </Box>
          <Button
            disabled={busyTask !== null}
            onClick={runPack}
            startIcon={busyTask === "pack" ? <CircularProgress size={18} /> : <ArchiveIcon />}
            variant="contained"
          >
            Pack
          </Button>
        </Stack>
      </Box>

      <Divider />

      <Box className="toolSection">
        <SectionTitle icon={<ArchiveIcon />} title="Export" />
        <Stack spacing={2}>
          <PathRow
            icon="file"
            label="SVD package"
            onBrowse={() => choosePackageFile((value) => updateExport("packagePath", value, setExportForm))}
            value={exportForm.packagePath}
            onChange={(value) => updateExport("packagePath", value, setExportForm)}
          />
          <Stack direction="row" spacing={1}>
            <Button disabled={busyTask !== null} onClick={loadSummary} variant="outlined">
              Load variants
            </Button>
            {busyTask === "summary" && <CircularProgress size={24} />}
          </Stack>
          <PackageSummary
            allVariants={exportForm.allVariants}
            onAllVariantsChange={(value) => updateExport("allVariants", value, setExportForm)}
            onVariantToggle={(name) => toggleVariant(name, setExportForm)}
            selected={exportForm.selectedVariants}
            summary={packageSummary}
          />
          <PathRow
            icon="save"
            label="Export zip"
            onBrowse={() => chooseZipSave((value) => updateExport("outputZip", value, setExportForm))}
            value={exportForm.outputZip}
            onChange={(value) => updateExport("outputZip", value, setExportForm)}
          />
          <TextField
            className="jobsField"
            label="Jobs"
            onChange={(event) => updateExport("jobs", event.target.value, setExportForm)}
            size="small"
            type="number"
            value={exportForm.jobs}
          />
          <Button
            disabled={busyTask !== null}
            onClick={runExport}
            startIcon={busyTask === "export" ? <CircularProgress size={18} /> : <SaveIcon />}
            variant="contained"
          >
            Export
          </Button>
        </Stack>
      </Box>
    </Stack>
  );
}

interface StatusAlertProps {
  errorText: string;
  exportSummary: SvdExportSummary | null;
  packSummary: SvdPackSummary | null;
  selectedCount: number;
}

function StatusAlert(props: StatusAlertProps) {
  if (props.errorText) {
    return <Alert severity="error">{props.errorText}</Alert>;
  }
  if (props.exportSummary) {
    return <Alert severity="success">Exported {props.selectedCount} variants.</Alert>;
  }
  if (props.packSummary) {
    return <Alert severity="success">Packed {props.packSummary.outputDir}</Alert>;
  }
  return <Alert severity="info">Ready</Alert>;
}

interface SectionTitleProps {
  icon: React.ReactNode;
  title: string;
}

function SectionTitle({ icon, title }: SectionTitleProps) {
  return (
    <Stack className="sectionTitle" direction="row" spacing={1}>
      {icon}
      <Typography variant="h6">{title}</Typography>
    </Stack>
  );
}

interface PathRowProps {
  icon?: "directory" | "file" | "save";
  label: string;
  onBrowse: () => void;
  onChange: (value: string) => void;
  value: string;
}

function PathRow(props: PathRowProps) {
  return (
    <Box className="pathRow">
      <TextField
        fullWidth
        label={props.label}
        onChange={(event) => props.onChange(event.target.value)}
        size="small"
        value={props.value}
      />
      <Tooltip title={`Choose ${props.label}`}>
        <IconButton onClick={props.onBrowse}>{pathIcon(props.icon)}</IconButton>
      </Tooltip>
    </Box>
  );
}

interface PackageSummaryProps {
  allVariants: boolean;
  onAllVariantsChange: (value: boolean) => void;
  onVariantToggle: (name: string) => void;
  selected: string[];
  summary: SvdPackageSummary | null;
}

function PackageSummary(props: PackageSummaryProps) {
  if (!props.summary) {
    return null;
  }
  return (
    <Box className="variantPanel">
      <Typography className="packageName">{packageName(props.summary)}</Typography>
      <Typography className="baseVariant">Base: {props.summary.baseVariant}</Typography>
      <FormControlLabel
        control={
          <Checkbox
            checked={props.allVariants}
            onChange={(event) => props.onAllVariantsChange(event.target.checked)}
          />
        }
        label="All variants"
      />
      {!props.allVariants && (
        <FormGroup className="variantList">
          {props.summary.variants.map((name) => (
            <FormControlLabel
              control={
                <Checkbox
                  checked={props.selected.includes(name)}
                  onChange={() => props.onVariantToggle(name)}
                />
              }
              key={name}
              label={name}
            />
          ))}
        </FormGroup>
      )}
    </Box>
  );
}

async function runTask(
  task: BusyTask,
  setBusyTask: (task: BusyTask) => void,
  setErrorText: (text: string) => void,
  action: () => Promise<void>,
) {
  setBusyTask(task);
  setErrorText("");
  try {
    await action();
  } catch (error) {
    setErrorText(String(error));
  } finally {
    setBusyTask(null);
  }
}

function buildPackRequest(pack: PackState): SvdPackRequest {
  return {
    inputDir: pack.inputDir,
    baseVariant: pack.baseVariant,
    outputDir: pack.outputDir,
    packagePath: optionalText(pack.packagePath),
    compressionLevel: parseCompressionLevel(pack.compressionLevel),
    jobs: parseJobs(pack.jobs),
  };
}

function buildExportRequest(form: ExportState): SvdExportRequest {
  return {
    packagePath: form.packagePath,
    outputZip: form.outputZip,
    allVariants: form.allVariants,
    variants: form.allVariants ? [] : form.selectedVariants,
    jobs: parseJobs(form.jobs),
  };
}

function parseCompressionLevel(value: string): number {
  const level = Number(value);
  if (Number.isInteger(level)) {
    return level;
  }
  throw new Error("Compression level must be an integer");
}

function parseJobs(value: string): number | null {
  const trimmed = value.trim();
  if (!trimmed) {
    return null;
  }
  const jobs = Number(trimmed);
  if (Number.isInteger(jobs) && jobs > 0) {
    return jobs;
  }
  throw new Error("Jobs must be a positive integer");
}

function optionalText(value: string): string | null {
  const trimmed = value.trim();
  return trimmed ? trimmed : null;
}

function updatePack<K extends keyof PackState>(
  field: K,
  value: PackState[K],
  setPack: React.Dispatch<React.SetStateAction<PackState>>,
) {
  setPack((current) => ({ ...current, [field]: value }));
}

function updateExport<K extends keyof ExportState>(
  field: K,
  value: ExportState[K],
  setExportForm: React.Dispatch<React.SetStateAction<ExportState>>,
) {
  setExportForm((current) => ({ ...current, [field]: value }));
}

function toggleVariant(
  name: string,
  setExportForm: React.Dispatch<React.SetStateAction<ExportState>>,
) {
  setExportForm((current) => ({
    ...current,
    selectedVariants: toggledVariantList(name, current.selectedVariants),
  }));
}

function toggledVariantList(name: string, selected: string[]) {
  if (selected.includes(name)) {
    return selected.filter((value) => value !== name);
  }
  return [...selected, name];
}

function selectedVariantCount(form: ExportState, summary: SvdPackageSummary | null) {
  if (!summary) {
    return 0;
  }
  return form.allVariants ? summary.variants.length : form.selectedVariants.length;
}

function packageName(summary: SvdPackageSummary) {
  return summary.modName ?? "SVD package";
}

function pathIcon(icon: PathRowProps["icon"]) {
  if (icon === "file") {
    return <FileOpenIcon />;
  }
  if (icon === "save") {
    return <SaveIcon />;
  }
  return <FolderOpenIcon />;
}

async function chooseDirectory(apply: (value: string) => void) {
  const selected = await open({ directory: true, multiple: false });
  applySelectedPath(selected, apply);
}

async function choosePackageFile(apply: (value: string) => void) {
  const selected = await open({ filters: [svdFilter()], multiple: false });
  applySelectedPath(selected, apply);
}

async function choosePackageSave(apply: (value: string) => void) {
  const selected = await save({ filters: [svdFilter()] });
  applySelectedPath(selected, apply);
}

async function chooseZipSave(apply: (value: string) => void) {
  const selected = await save({ filters: [zipFilter()] });
  applySelectedPath(selected, apply);
}

function applySelectedPath(
  selected: string | string[] | null,
  apply: (value: string) => void,
) {
  if (typeof selected === "string") {
    apply(selected);
  }
}

function svdFilter() {
  return { name: "SVD package", extensions: ["svd"] };
}

function zipFilter() {
  return { name: "Zip archive", extensions: ["zip"] };
}

export default SvdPanel;
