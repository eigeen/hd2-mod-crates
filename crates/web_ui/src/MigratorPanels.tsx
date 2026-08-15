import ArchiveIcon from "@mui/icons-material/Archive";
import CompareArrowsIcon from "@mui/icons-material/CompareArrows";
import DescriptionIcon from "@mui/icons-material/Description";
import HelpOutlineIcon from "@mui/icons-material/HelpOutlined";
import SearchIcon from "@mui/icons-material/Search";
import TaskAltIcon from "@mui/icons-material/TaskAlt";
import UploadFileIcon from "@mui/icons-material/UploadFile";
import {
  Button,
  Checkbox,
  FormControlLabel,
  InputAdornment,
  MenuItem,
  Radio,
  Switch,
  TextField,
  Tooltip,
  ToggleButton,
  ToggleButtonGroup,
} from "@mui/material";
import { Hd2Dialog, Hd2DialogActions, Hd2DialogContent, Hd2DialogTitle } from "./Hd2Dialog";
import { useI18n } from "./i18n";
import { memo, useCallback, useMemo, useState } from "react";
import type { ReactNode } from "react";
import type {
  DetectedSource,
  EquipmentCategory,
  EquipmentOption,
  PatchInfo,
  UnmatchedUnitPolicy,
} from "./types";
import { EquipmentCategoryIcon } from "./EquipmentCategoryIcon";
import { patchSelectionFromDrop } from "./patchSelection";
import { useDropZone } from "./useDropZone";

const panelClass = "overflow-hidden";
const setupPanelClass = `${panelClass} flex flex-1 flex-col p-6`;
const metaLineClass =
  "flex items-center gap-[0.4375rem] text-xs text-hd2-muted [overflow-wrap:anywhere]";

interface PatchPanelProps {
  patch: PatchInfo | null;
  onPatchFiles: (files: FileList | File[] | null, originalName?: string) => void;
}

export function PatchPanel(props: PatchPanelProps) {
  const { t } = useI18n();
  const receiveDrop = useCallback(
    async (dataTransfer: DataTransfer) => {
      const selection = await patchSelectionFromDrop(dataTransfer);
      props.onPatchFiles(selection.files, selection.originalName);
    },
    [props.onPatchFiles],
  );
  const dropZone = useDropZone(receiveDrop);

  return (
    <div
      className={`${setupPanelClass} ${dropZone.dragging ? "bg-hd2-yellow-bg outline-1 outline-hd2-yellow -outline-offset-1" : ""}`}
      {...dropZone.handlers}
    >
      <SectionTitle icon={<ArchiveIcon />} title={t("patch.title")} />
      <div className="flex flex-wrap items-center gap-2.5">
        <Button component="label" startIcon={<UploadFileIcon />} variant="contained">
          {t("patch.pick")}
          <input hidden multiple type="file" onChange={(event) => props.onPatchFiles(event.target.files)} />
        </Button>
        <HelpHint title={t("patch.help")} />
      </div>
      <p className={`m-0 mt-3 border border-dashed px-3 py-2 text-xs ${dropZone.dragging ? "border-hd2-yellow text-hd2-yellow" : "border-hd2-line text-hd2-muted"}`}>
        {dropZone.dragging ? t("patch.dropActive") : t("patch.dropHint")}
      </p>
      <p className={`${metaLineClass} m-0 mt-auto pt-3`}>
        <span className="inline-flex h-[0.3125rem] w-[0.3125rem] bg-hd2-faint" />
        {props.patch ? props.patch.name : t("patch.empty")}
      </p>
    </div>
  );
}

interface TargetPanelProps {
  activeSourceId: string;
  equipmentOptions: EquipmentOption[];
  multiTarget: boolean;
  multiTargetEligible: boolean;
  onMultiTargetChange: (enabled: boolean) => void;
  onSinglePatchChange: (enabled: boolean) => void;
  onResolveSource: (sourceId: string, hash: string) => void;
  onSourceChange: (sourceId: string) => void;
  onTargetChange: (hash: string) => void;
  selectedTargets: string[];
  separateOutputMappingLimit: number;
  singlePatch: boolean;
  singlePatchMappingLimit: number;
  singlePatchRequired: boolean;
  sources: DetectedSource[];
  targetOptions: EquipmentOption[];
  targetSelectionEnabled: boolean;
  targetsBySource: Record<string, string[]>;
  showOutputLimits?: boolean;
}

export function TargetPanel(props: TargetPanelProps) {
  const { t } = useI18n();

  return (
    <div className={`${panelClass} flex flex-col`}>
      <div className="flex flex-col flex-wrap items-center gap-2 border-b border-hd2-border hd2-stripes-accent px-6 py-3.5 min-[51.25rem]:flex-row min-[51.25rem]:gap-4">
        <SectionTitle icon={<CompareArrowsIcon />} title={t("mapping.title")} inHeader />
        <div className="hidden flex-1 min-[51.25rem]:block" />
        {props.multiTargetEligible && <>
          <FormControlLabel
            className="multiToggle"
            control={<Switch checked={props.multiTarget} onChange={(event) => props.onMultiTargetChange(event.target.checked)} />}
            label={t("mapping.multiTarget")}
          />
          <HelpHint title={t("mapping.multiTargetHelp")} />
          <FormControlLabel
            className="multiToggle"
            control={(
              <Switch
                checked={props.singlePatch}
                disabled={props.singlePatchRequired}
                onChange={(event) => props.onSinglePatchChange(event.target.checked)}
              />
            )}
            label={t("mapping.singlePatch")}
          />
          <HelpHint title={t(
            props.singlePatchRequired
              ? "mapping.singlePatchForcedHelp"
              : "mapping.singlePatchHelp",
            { max: props.singlePatchMappingLimit },
          )} />
          {props.showOutputLimits !== false && (
            <span className="border border-hd2-line px-2 py-1 text-center text-[0.6875rem] text-hd2-muted">
              {t("mapping.outputLimitPreview", {
                separate: props.separateOutputMappingLimit,
                single: props.singlePatchMappingLimit,
              })}
            </span>
          )}
        </>}
      </div>

      <MappingGrid
        activeSourceId={props.activeSourceId}
        equipmentOptions={props.equipmentOptions}
        multiTarget={props.multiTarget}
        onResolveSource={props.onResolveSource}
        onSourceChange={props.onSourceChange}
        onTargetChange={props.onTargetChange}
        selectedTargets={props.selectedTargets}
        sources={props.sources}
        targetOptions={props.targetOptions}
        targetSelectionEnabled={props.targetSelectionEnabled}
        targetsBySource={props.targetsBySource}
      />
    </div>
  );
}

interface MappingGridProps {
  activeSourceId: string;
  equipmentOptions: EquipmentOption[];
  multiTarget: boolean;
  onResolveSource: (sourceId: string, hash: string) => void;
  onSourceChange: (hash: string) => void;
  onTargetChange: (hash: string) => void;
  selectedTargets: string[];
  sources: DetectedSource[];
  targetOptions: EquipmentOption[];
  targetSelectionEnabled: boolean;
  targetsBySource: Record<string, string[]>;
}

const MappingGrid = memo(function MappingGrid(props: MappingGridProps) {
  const { t } = useI18n();
  const selectedTargetSet = useMemo(() => new Set(props.selectedTargets), [props.selectedTargets]);
  const [targetQuery, setTargetQuery] = useState("");

  const filteredTargets = useMemo(
    () => filterTargets(props.targetOptions, targetQuery),
    [props.targetOptions, targetQuery],
  );

  return (
    <div className="m-3 flex min-h-[16.25rem] flex-col items-stretch bg-hd2-sunken min-[51.25rem]:flex-row">
      {/* 注意：故意不用 MUI FormControl 包裹 Radio 列表 —— FormControl 在 dev 模式下 childContext useMemo 每次都会变 ref，
          会导致内部所有 Radio 在每次父级 render 时都重渲染，对几百项列表非常卡。 */}
      <div className="flex min-w-0 flex-1 flex-col p-6 min-[51.25rem]:pr-9">
        <MappingSectionLabel>{t("mapping.source")}</MappingSectionLabel>
        {props.sources.length ? (
          <div className="hd2-scroll flex max-h-[22.5rem] flex-1 flex-col overflow-auto">
            {props.sources.map((source) => (
              <SourceChoice
                active={source.id === props.activeSourceId}
                key={source.id}
                onActivate={props.onSourceChange}
                onResolve={props.onResolveSource}
                selectedTargets={props.targetsBySource[source.id] ?? []}
                source={source}
                targetOptions={props.equipmentOptions}
              />
            ))}
          </div>
        ) : (
          <EmptyMapping icon={<DescriptionIcon />} text={t("mapping.importPatchFirst")} />
        )}
      </div>
      <div className="flex min-w-0 flex-1 flex-col border-t border-hd2-border p-6 min-[51.25rem]:border-t-0 min-[51.25rem]:border-l min-[51.25rem]:pl-4">
        {/* Row 1: label */}
        <div className="mb-2">
          <MappingSectionLabel inline>{t("mapping.target")}</MappingSectionLabel>
        </div>
        {/* Row 2: search */}
        <div className="mb-3">
          <TextField
            className="targetSearchField"
            onChange={(event) => setTargetQuery(event.target.value)}
            placeholder={t("mapping.search")}
            size="small"
            slotProps={{ input: { startAdornment: <InputAdornment position="start"><SearchIcon fontSize="small" /></InputAdornment> } }}
            value={targetQuery}
            variant="standard"
          />
        </div>
        <div className="hd2-scroll flex max-h-[22.5rem] flex-1 flex-col overflow-auto">
          {!props.activeSourceId || !props.targetSelectionEnabled ? (
            <EmptyMapping icon={<DescriptionIcon />} text={t("mapping.selectSource")} />
          ) : filteredTargets.length ? (
            filteredTargets.map((target) => (
              <TargetChoice
                key={target.hash}
                multiTarget={props.multiTarget}
                onChange={props.onTargetChange}
                selected={selectedTargetSet.has(target.hash)}
                target={target}
              />
            ))
          ) : (
            <EmptyMapping icon={<TaskAltIcon />} text={targetQuery ? t("mapping.noMatches") : t("mapping.output")} />
          )}
        </div>
      </div>
    </div>
  );
});

function MappingSectionLabel({ children, inline }: { children: ReactNode; inline?: boolean }) {
  return (
    <p
      className={`m-0 text-xs font-bold uppercase tracking-[0.08em] leading-none text-hd2-faint ${
        inline ? "" : "mb-3"
      }`}
    >
      {children}
    </p>
  );
}

function filterTargets(targets: EquipmentOption[], query: string) {
  const trimmed = query.trim().toLowerCase();
  if (!trimmed) {
    return targets;
  }
  return targets.filter((target) =>
    target.name.toLowerCase().includes(trimmed) || target.hash.toLowerCase().includes(trimmed),
  );
}

interface TargetChoiceProps {
  multiTarget: boolean;
  onChange: (hash: string) => void;
  selected: boolean;
  target: EquipmentOption;
}

const TargetChoice = memo(function TargetChoice(props: TargetChoiceProps) {
  return (
    <div
      className="mb-1.5 flex min-h-[3.625rem] cursor-pointer items-center border border-transparent px-2 py-1.5 transition-all hover:border-hd2-yellow hover:bg-hd2-yellow-bg hover:shadow-(--hd2-glow-sm)"
      onClick={() => props.onChange(props.target.hash)}
    >
      {props.multiTarget ? <Checkbox checked={props.selected} /> : <Radio checked={props.selected} />}
      <TargetText target={props.target} />
    </div>
  );
});

function TargetText({ target }: { target: EquipmentOption }) {
  return (
    <div className="flex min-w-0 items-center gap-2">
      <EquipmentIcon category={target.category} />
      <div className="min-w-0">
      <p className="m-0 text-[0.8125rem] font-bold leading-[1.35] text-hd2-text [overflow-wrap:anywhere]">{target.name}</p>
      <p className="m-0 font-mono text-[0.6875rem] leading-[1.45] text-hd2-faint [overflow-wrap:anywhere]">{target.hash}</p>
      </div>
    </div>
  );
}

interface SourceChoiceProps {
  active: boolean;
  onActivate: (sourceId: string) => void;
  onResolve: (sourceId: string, hash: string) => void;
  selectedTargets: string[];
  source: DetectedSource;
  targetOptions: EquipmentOption[];
}

function SourceChoice(props: SourceChoiceProps) {
  const { t } = useI18n();
  const resolved = props.source.candidates.find((candidate) => candidate.hash === props.source.resolvedHash);
  const selectedNames = props.selectedTargets
    .map((hash) => props.targetOptions.find((target) => target.hash === hash)?.name ?? hash);
  return (
    <div
      className={`mb-2 cursor-pointer border px-3 py-2 transition-colors ${
        props.active ? "border-hd2-yellow bg-hd2-yellow-bg" : "border-transparent hover:border-hd2-line"
      }`}
      onClick={() => props.onActivate(props.source.id)}
    >
      <div className="flex items-center gap-2">
        <Radio checked={props.active} size="small" />
        <EquipmentIcon category={props.source.category} />
        <div className="min-w-0 flex-1">
          <p className="m-0 text-[0.8125rem] font-bold text-hd2-text">
            {resolved?.name ?? props.source.candidates[0]?.name}
          </p>
          {resolved && selectedNames.length > 0 && (
            <p className="m-0 text-xs text-hd2-yellow">
              {resolved.name} → {selectedNames[0]}{selectedNames.length > 1 ? ` +${selectedNames.length - 1}` : ""}
            </p>
          )}
          {resolved && selectedNames.length === 0 && (
            <p className="m-0 text-xs text-hd2-muted">{t("mapping.sourceUnconfigured")}</p>
          )}
        </div>
      </div>
      {props.source.candidates.length > 1 && (
        <TextField
          className="mt-2"
          fullWidth
          label={t("mapping.sourceModel")}
          onChange={(event) => props.onResolve(props.source.id, event.target.value)}
          onClick={(event) => event.stopPropagation()}
          select
          size="small"
          value={resolved?.hash ?? props.source.candidates[0]?.hash ?? ""}
        >
          {props.source.candidates.map((candidate) => (
            <MenuItem key={candidate.hash} value={candidate.hash}>
              <span className="mr-2">{candidate.name}</span>
              <span className="font-mono text-xs text-hd2-faint">{candidate.hash}</span>
            </MenuItem>
          ))}
        </TextField>
      )}
    </div>
  );
}

function EquipmentIcon({ category }: { category: EquipmentCategory }) {
  const className = "shrink-0 text-hd2-faint";
  return <EquipmentCategoryIcon category={category} className={className} fontSize="small" />;
}

function EmptyMapping({ icon, text }: { icon: ReactNode; text: string }) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-2 border border-dashed border-hd2-border p-6 text-center text-hd2-muted min-h-[11.125rem] [&_svg]:text-hd2-line [&_svg]:text-[2rem]">
      {icon}
      <p className="m-0 text-xs">{text}</p>
    </div>
  );
}

interface OptionsPanelProps {
  noPadding: boolean;
  setNoPadding: (value: boolean) => void;
  setUnmatchedUnitPolicy: (value: UnmatchedUnitPolicy) => void;
  unmatchedUnitPolicy: UnmatchedUnitPolicy;
}

export const OptionsPanel = memo(function OptionsPanel(props: OptionsPanelProps) {
  const { t } = useI18n();

  return (
    <>
      <FormControlLabel
        className="optionsControl shrink-0"
        control={<Checkbox checked={props.noPadding} onChange={(event) => props.setNoPadding(event.target.checked)} />}
        label={t("options.noPadding")}
      />
      <HelpHint title={t("options.noPaddingHelp")} />
      <span className="shrink-0 whitespace-nowrap text-xs text-hd2-muted">{t("options.unmatchedUnits")}</span>
      <ToggleButtonGroup
        className="shrink-0"
        exclusive
        onChange={(_, value: UnmatchedUnitPolicy | null) => {
          if (value) props.setUnmatchedUnitPolicy(value);
        }}
        size="small"
        value={props.unmatchedUnitPolicy}
      >
        <ToggleButton value="drop">{t("options.unmatchedDrop")}</ToggleButton>
        <ToggleButton value="keep">{t("options.unmatchedKeep")}</ToggleButton>
      </ToggleButtonGroup>
      <HelpHint title={t("options.unmatchedUnitsHelp")} />
    </>
  );
});

interface PerformanceDialogProps {
  open: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}

export function PerformanceDialog(props: PerformanceDialogProps) {
  const { t } = useI18n();

  return (
    <Hd2Dialog open={props.open} onClose={props.onCancel}>
      <Hd2DialogTitle>{t("performance.title")}</Hd2DialogTitle>
      <Hd2DialogContent>
        <div className="border border-hd2-yellow/30 bg-hd2-yellow-bg px-4 py-3 text-sm text-hd2-muted">
          {t("performance.body")}
        </div>
      </Hd2DialogContent>
      <Hd2DialogActions>
        <Button onClick={props.onCancel}>{t("dialog.cancel")}</Button>
        <Button onClick={props.onConfirm} variant="contained">{t("dialog.continue")}</Button>
      </Hd2DialogActions>
    </Hd2Dialog>
  );
}

function HelpHint({ title }: { title: string }) {
  return (
    <Tooltip arrow title={title} placement="top">
      <HelpOutlineIcon className="shrink-0 cursor-help text-hd2-faint hover:text-hd2-yellow" sx={{ fontSize: "1rem" }} />
    </Tooltip>
  );
}

function SectionTitle({ icon, title, inHeader }: { icon: ReactNode; title: string; inHeader?: boolean }) {
  return (
    <div
      className={`flex flex-row items-center gap-2 [&_svg]:text-hd2-yellow [&_svg]:text-[1.125rem] ${
        inHeader ? "" : "mb-4"
      }`}
    >
      {icon}
      <h2 className="m-0 text-sm font-bold text-hd2-text">{title}</h2>
    </div>
  );
}
