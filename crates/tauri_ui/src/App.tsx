import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import type { DragDropEvent } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import FolderOpenIcon from "@mui/icons-material/FolderOpen";
import HelpOutlineIcon from "@mui/icons-material/HelpOutlineOutlined";
import PlayArrowIcon from "@mui/icons-material/PlayArrow";
import SearchIcon from "@mui/icons-material/Search";
import UploadFileIcon from "@mui/icons-material/UploadFile";
import {
  Alert,
  Box,
  Button,
  Checkbox,
  CircularProgress,
  Divider,
  FormControlLabel,
  IconButton,
  Stack,
  Tab,
  Tabs,
  TextField,
  Tooltip,
  Typography,
} from "@mui/material";
import { fieldFromPhysicalPosition } from "./dropTarget";
import SvdPanel from "./SvdPanel";
import TargetPicker from "./TargetPicker";
import type {
  GameDataDiscovery,
  MigrationTargetOption,
  MigrationProgressEvent,
  MigrationRequest,
  MigrationSummary,
  PathField,
  PathState,
} from "./types";
import "./App.css";

type ToolTab = "migration" | "svd";

const initialPaths: PathState = {
  patchPath: "",
  dataDir: "",
  outDir: "",
};

const pathFieldLabels: Record<PathField, string> = {
  patchPath: "Patch",
  dataDir: "Game data",
  outDir: "Output",
};

const pathFieldHelp: Record<PathField, string[]> = {
  patchPath: [
    "Choose the source mod patch file.",
    "Example file names: 9ba626afa44a3aa3.patch_0, 9ba626afa44a3aa3.stream, armor_export.xlsx.",
    "You can also drag the file onto this input.",
  ],
  dataDir: [
    "Choose the Helldivers 2 game data folder.",
    "Reference folder name: data.",
    "Typical path: C:\\Program Files (x86)\\Steam\\steamapps\\common\\Helldivers 2\\data.",
  ],
  outDir: [
    "Choose where migrated files should be written.",
    "Example folder names: migrated_armor, armor_out, mod_output.",
    "Use a new empty folder when comparing results between runs.",
  ],
};

const noPaddingHelp = [
  "Skip adding padding or empty mesh replacement data.",
  "Use this when you only want remap changes and want to keep source geometry untouched.",
  "Reference output: migrated files are written without padded unit replacements.",
];

const partialRemapHelp = [
  "Enable experimental partial remapping.",
  "Use this when only part of a target can be matched safely.",
  "Reference names: torso, helmet, shoulder, leg, body shadow variants.",
];

function App() {
  const [activeTool, setActiveTool] = useState<ToolTab>("migration");
  const [paths, setPaths] = useState<PathState>(initialPaths);
  const [targetOptions, setTargetOptions] = useState<MigrationTargetOption[]>([]);
  const [selectedTargets, setSelectedTargets] = useState<string[]>([]);
  const [targetSearch, setTargetSearch] = useState("");
  const [loadingTargets, setLoadingTargets] = useState(false);
  const [targetLoadError, setTargetLoadError] = useState("");
  const [noPadding, setNoPadding] = useState(false);
  const [partialRemap, setPartialRemap] = useState(false);
  const [hoveredField, setHoveredField] = useState<PathField | null>(null);
  const [detectingDataDir, setDetectingDataDir] = useState(false);
  const [running, setRunning] = useState(false);
  const [status, setStatus] = useState("Ready");
  const [summary, setSummary] = useState<MigrationSummary | null>(null);
  const [errorText, setErrorText] = useState("");
  const hoverRef = useRef<PathField | null>(null);
  const autoDetectRef = useRef(false);

  const setPath = useCallback((field: PathField, value: string) => {
    setPaths((current) => ({ ...current, [field]: value }));
  }, []);

  const choosePath = useCallback(
    async (field: PathField) => {
      const selected = await open(dialogOptions(field));
      if (typeof selected === "string") {
        setPath(field, selected);
      }
    },
    [setPath],
  );

  const detectGameDataDir = useCallback(
    async (mode: DetectMode) => {
      setDetectingDataDir(true);
      if (mode === "manual") {
        setErrorText("");
        setStatus("Searching game data directory");
      }
      try {
        const discovery = await invoke<GameDataDiscovery>("detect_game_data_dir");
        applyGameDataDiscovery(discovery, mode, { setPath, setPaths, setStatus });
      } catch (error) {
        if (mode === "manual") {
          setErrorText(String(error));
          setStatus("Game directory search failed");
        }
      } finally {
        setDetectingDataDir(false);
      }
    },
    [setPath],
  );

  const loadTargetOptions = useCallback(async () => {
    setLoadingTargets(true);
    setTargetLoadError("");
    try {
      setTargetOptions(await invoke<MigrationTargetOption[]>("load_migration_targets"));
    } catch (error) {
      setTargetLoadError(String(error));
    } finally {
      setLoadingTargets(false);
    }
  }, []);

  const toggleTarget = useCallback((hash: string) => {
    setSelectedTargets((current) => toggledTargetList(hash, current));
  }, []);

  const selectAllTargets = useCallback(() => {
    setSelectedTargets(targetOptions.map((target) => target.hash));
  }, [targetOptions]);

  const runMigration = useCallback(async () => {
    if (selectedTargets.length === 0) {
      setErrorText("Select at least one target.");
      setStatus("Select targets");
      return;
    }
    setRunning(true);
    setSummary(null);
    setErrorText("");
    setStatus("Preparing migration");
    try {
      const request = requestFromState(paths, selectedTargets, noPadding, partialRemap);
      setSummary(await invoke<MigrationSummary>("run_migration", { request }));
      setStatus("Migration complete");
    } catch (error) {
      setErrorText(String(error));
      setStatus("Failed");
    } finally {
      setRunning(false);
    }
  }, [noPadding, partialRemap, paths, selectedTargets]);

  useEffect(() => {
    return subscribeToProgress(setStatus);
  }, []);

  useEffect(() => {
    void loadTargetOptions();
  }, [loadTargetOptions]);

  useEffect(() => {
    if (autoDetectRef.current) {
      return;
    }
    autoDetectRef.current = true;
    void detectGameDataDir("auto");
  }, [detectGameDataDir]);

  useEffect(() => {
    return subscribeToPathDrop(setHoveredField, hoverRef, setPath);
  }, [setPath]);

  const reportLines = useMemo(() => formatReports(summary), [summary]);
  const toolbarTitle = activeTool === "migration" ? "HD2 Migrator" : "Super Variant Dist";
  const toolbarStatus = activeTool === "migration" ? status : "Compression packer and exporter";

  return (
    <Box className="appShell">
      <Stack className="toolbar" direction="row" spacing={2}>
        <Box>
          <Typography variant="h5" component="h1">
            {toolbarTitle}
          </Typography>
          <Typography className="statusText">{toolbarStatus}</Typography>
        </Box>
        <Tabs
          className="toolTabs"
          onChange={(_, value: ToolTab) => setActiveTool(value)}
          value={activeTool}
        >
          <Tab label="Migrator" value="migration" />
          <Tab label="SVD" value="svd" />
        </Tabs>
        <Box className="toolbarSpacer" />
        {activeTool === "migration" && running && <CircularProgress size={24} />}
        {activeTool === "migration" && (
          <Button
            disabled={running}
            onClick={runMigration}
            startIcon={<PlayArrowIcon />}
            variant="contained"
          >
            Run
          </Button>
        )}
      </Stack>

      <Stack className="content" spacing={2}>
        {activeTool === "migration" ? (
          <>
            <PathInput
              field="patchPath"
              hoveredField={hoveredField}
              label={pathFieldLabels.patchPath}
              onBrowse={choosePath}
              onChange={setPath}
              value={paths.patchPath}
            />
            <PathInput
              field="dataDir"
              hoveredField={hoveredField}
              label={pathFieldLabels.dataDir}
              detectingDataDir={detectingDataDir}
              onBrowse={choosePath}
              onChange={setPath}
              onDetectDataDir={() => void detectGameDataDir("manual")}
              value={paths.dataDir}
            />
            <PathInput
              field="outDir"
              hoveredField={hoveredField}
              label={pathFieldLabels.outDir}
              onBrowse={choosePath}
              onChange={setPath}
              value={paths.outDir}
            />

            <TargetPicker
              errorText={targetLoadError}
              loading={loadingTargets}
              onClear={() => setSelectedTargets([])}
              onQueryChange={setTargetSearch}
              onSelectAll={selectAllTargets}
              onToggle={toggleTarget}
              options={targetOptions}
              query={targetSearch}
              selectedHashes={selectedTargets}
            />

            <Stack direction="row" spacing={3}>
              <FormControlLabel
                control={
                  <Checkbox
                    checked={noPadding}
                    onChange={(event) => setNoPadding(event.target.checked)}
                  />
                }
                label={<OptionLabel help={noPaddingHelp} text="No padding" />}
              />
              <FormControlLabel
                control={
                  <Checkbox
                    checked={partialRemap}
                    onChange={(event) => setPartialRemap(event.target.checked)}
                  />
                }
                label={<OptionLabel help={partialRemapHelp} text="Partial remap" />}
              />
            </Stack>

            <Divider />

            <SummaryPanel errorText={errorText} summary={summary} />
            <Box className="logPanel">
              <pre>{reportLines}</pre>
            </Box>
          </>
        ) : (
          <SvdPanel />
        )}
      </Stack>
    </Box>
  );
}

interface PathInputProps {
  detectingDataDir?: boolean;
  field: PathField;
  hoveredField: PathField | null;
  label: string;
  onBrowse: (field: PathField) => void;
  onChange: (field: PathField, value: string) => void;
  onDetectDataDir?: () => void;
  value: string;
}

function PathInput(props: PathInputProps) {
  const active = props.hoveredField === props.field;
  const icon = props.field === "patchPath" ? <UploadFileIcon /> : <FolderOpenIcon />;
  return (
    <Box className={active ? "pathRow pathRowActive" : "pathRow"} data-drop-field={props.field}>
      <TextField
        className={active ? "pathField pathFieldActive" : "pathField"}
        fullWidth
        label={props.label}
        onChange={(event) => props.onChange(props.field, event.target.value)}
        size="small"
        value={props.value}
      />
      <HelpTooltip lines={pathFieldHelp[props.field]} title={`${props.label} help`} />
      {props.onDetectDataDir && (
        <Tooltip title="Find game data folder">
          <span>
            <IconButton disabled={props.detectingDataDir} onClick={props.onDetectDataDir}>
              {props.detectingDataDir ? <CircularProgress size={20} /> : <SearchIcon />}
            </IconButton>
          </span>
        </Tooltip>
      )}
      <Tooltip title={`Choose ${props.label}`}>
        <IconButton onClick={() => props.onBrowse(props.field)}>{icon}</IconButton>
      </Tooltip>
    </Box>
  );
}

interface HelpTooltipProps {
  lines: string[];
  title: string;
}

function HelpTooltip({ lines, title }: HelpTooltipProps) {
  return (
    <Tooltip arrow disableInteractive placement="top" title={<TooltipLines lines={lines} />}>
      <IconButton aria-label={title} className="helpButton" size="small">
        <HelpOutlineIcon fontSize="small" />
      </IconButton>
    </Tooltip>
  );
}

interface TooltipLinesProps {
  lines: string[];
}

function TooltipLines({ lines }: TooltipLinesProps) {
  return (
    <Box className="helpTooltip">
      {lines.map((line) => (
        <Typography className="helpTooltipLine" key={line}>
          {line}
        </Typography>
      ))}
    </Box>
  );
}

interface OptionLabelProps {
  help: string[];
  text: string;
}

function OptionLabel({ help, text }: OptionLabelProps) {
  return (
    <Box className="optionLabel">
      <Typography component="span">{text}</Typography>
      <HelpTooltip lines={help} title={`${text} help`} />
    </Box>
  );
}

interface SummaryPanelProps {
  errorText: string;
  summary: MigrationSummary | null;
}

function SummaryPanel({ errorText, summary }: SummaryPanelProps) {
  if (errorText) {
    return <Alert severity="error">{errorText}</Alert>;
  }
  if (!summary) {
    return <Alert severity="info">Ready</Alert>;
  }
  return (
    <Stack className="summaryStrip" direction="row" spacing={3}>
      <Metric label="Migrated" value={summary.migratedCount} />
      <Metric label="Warnings" value={summary.warningCount} />
    </Stack>
  );
}

interface MetricProps {
  label: string;
  value: number;
}

function Metric({ label, value }: MetricProps) {
  return (
    <Box className="metric">
      <Typography className="metricValue">{value}</Typography>
      <Typography className="metricLabel">{label}</Typography>
    </Box>
  );
}

function dialogOptions(field: PathField) {
  if (field === "patchPath") {
    return { multiple: false };
  }
  return { directory: true, multiple: false };
}

function requestFromState(
  paths: PathState,
  selectedTargets: string[],
  noPadding: boolean,
  experimentalPartialRemap: boolean,
): MigrationRequest {
  return {
    ...paths,
    targetFilter: selectedTargets.join(","),
    noPadding,
    experimentalPartialRemap,
  };
}

function toggledTargetList(hash: string, selected: string[]) {
  if (selected.includes(hash)) {
    return selected.filter((value) => value !== hash);
  }
  return [...selected, hash];
}

type DetectMode = "auto" | "manual";

function applyGameDataDiscovery(
  discovery: GameDataDiscovery,
  mode: DetectMode,
  actions: DiscoveryActions,
) {
  const dataDir = discovery.dataDir ?? discovery.candidates[0] ?? "";
  if (!dataDir) {
    updateDiscoveryMissStatus(mode, actions.setStatus);
    return;
  }
  setDetectedDataDir(dataDir, mode, actions);
  actions.setStatus("Game data directory found");
}

interface DiscoveryActions {
  setPath: (field: PathField, value: string) => void;
  setPaths: React.Dispatch<React.SetStateAction<PathState>>;
  setStatus: (status: string) => void;
}

function updateDiscoveryMissStatus(mode: DetectMode, setStatus: (status: string) => void) {
  if (mode === "manual") {
    setStatus("No game data directory found");
  }
}

function setDetectedDataDir(
  dataDir: string,
  mode: DetectMode,
  actions: DiscoveryActions,
) {
  if (mode === "manual") {
    actions.setPath("dataDir", dataDir);
    return;
  }
  actions.setPaths((current) => (current.dataDir ? current : { ...current, dataDir }));
}

function subscribeToProgress(setStatus: (status: string) => void) {
  let unlisten: UnlistenFn | undefined;
  listen<MigrationProgressEvent>("migration://progress", (event) => {
    setStatus(event.payload.status);
  }).then((handler) => {
    unlisten = handler;
  });
  return () => unlisten?.();
}

function subscribeToPathDrop(
  setHoveredField: (field: PathField | null) => void,
  hoverRef: React.MutableRefObject<PathField | null>,
  setPath: (field: PathField, value: string) => void,
) {
  let unlisten: UnlistenFn | undefined;
  getCurrentWebview().onDragDropEvent((event) => {
    handleDropEvent(event.payload, setHoveredField, hoverRef, setPath);
  }).then((handler) => {
    unlisten = handler;
  });
  return () => unlisten?.();
}

function handleDropEvent(
  event: DragDropEvent,
  setHoveredField: (field: PathField | null) => void,
  hoverRef: React.MutableRefObject<PathField | null>,
  setPath: (field: PathField, value: string) => void,
) {
  if (event.type === "leave") {
    updateHoveredField(null, setHoveredField, hoverRef);
    return;
  }
  const field = fieldFromPhysicalPosition(event.position);
  updateHoveredField(field, setHoveredField, hoverRef);
  if (event.type === "drop") {
    applyDroppedPath(event.paths, hoverRef.current, setPath);
    updateHoveredField(null, setHoveredField, hoverRef);
  }
}

function updateHoveredField(
  field: PathField | null,
  setHoveredField: (field: PathField | null) => void,
  hoverRef: React.MutableRefObject<PathField | null>,
) {
  hoverRef.current = field;
  setHoveredField(field);
}

function applyDroppedPath(
  paths: string[],
  field: PathField | null,
  setPath: (field: PathField, value: string) => void,
) {
  if (paths.length > 0 && field) {
    setPath(field, paths[0]);
  }
}

function formatReports(summary: MigrationSummary | null) {
  if (!summary) {
    return "";
  }
  return summary.reports.map(formatReport).join("\n");
}

function formatReport(report: MigrationSummary["reports"][number]) {
  const warnings = report.warnings.length > 0 ? ` warnings=${report.warnings.length}` : "";
  return `${report.targetName}: fileIds=${report.fileIdRemapped} slotIds=${report.slotIdRemapped} padded=${report.paddedUnits}${warnings}`;
}

export default App;
