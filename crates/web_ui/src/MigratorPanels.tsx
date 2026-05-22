import ArchiveIcon from "@mui/icons-material/Archive";
import CompareArrowsIcon from "@mui/icons-material/CompareArrows";
import DescriptionIcon from "@mui/icons-material/Description";
import DownloadIcon from "@mui/icons-material/Download";
import FolderOpenIcon from "@mui/icons-material/FolderOpen";
import HelpOutlineIcon from "@mui/icons-material/HelpOutlined";
import InventoryIcon from "@mui/icons-material/Inventory2";
import RestartAltIcon from "@mui/icons-material/RestartAlt";
import TaskAltIcon from "@mui/icons-material/TaskAlt";
import UploadFileIcon from "@mui/icons-material/UploadFile";
import {
  Alert,
  Button,
  Checkbox,
  Dialog,
  DialogActions,
  DialogContent,
  DialogTitle,
  FormControl,
  FormControlLabel,
  FormLabel,
  InputAdornment,
  Radio,
  RadioGroup,
  Switch,
  TextField,
  Tooltip,
} from "@mui/material";
import { memo, useMemo } from "react";
import type { ReactNode } from "react";
import { downloadZip } from "./fileInputs";
import type { MetadataState, MigrationResult, PatchFiles, TargetOption } from "./types";

const panelClass =
  "rounded-2xl border border-slate-200/85 bg-white shadow-[0_1px_2px_rgb(15_23_42_/_0.04)] overflow-hidden";
const setupPanelClass = `${panelClass} flex flex-1 flex-col p-6`;
const metaLineClass =
  "flex items-center gap-[7px] text-xs text-slate-500 [overflow-wrap:anywhere]";

interface MetadataPanelProps {
  busy: boolean;
  metadata: MetadataState | null;
  onLoadPublicMetadata: () => void;
  onDirectoryMetadata: () => void;
  onMetadataFile: (files: FileList | null) => void;
}

export function MetadataPanel(props: MetadataPanelProps) {
  return (
    <div className={setupPanelClass}>
      <SectionTitle icon={<InventoryIcon />} title="元数据" />
      <div className="mb-3 flex flex-wrap items-center gap-2.5">
        <Button disabled={props.busy} onClick={props.onLoadPublicMetadata} startIcon={<DownloadIcon />} variant="outlined">
          加载内置
        </Button>
        <HelpHint title="使用应用内置的元数据 JSON（推荐快速上手时使用）。" />
        <Button component="label" disabled={props.busy} startIcon={<UploadFileIcon />} variant="outlined">
          导入 JSON
          <input hidden accept="application/json,.json" type="file" onChange={(event) => props.onMetadataFile(event.target.files)} />
        </Button>
        <HelpHint title="导入自定义元数据 JSON 文件（适用于内置元数据未覆盖的版本）。" />
        <Button disabled={props.busy} onClick={props.onDirectoryMetadata} startIcon={<FolderOpenIcon />} variant="outlined">
          浏览器目录
        </Button>
        <HelpHint title="选择本地游戏 data 目录，由浏览器读取存档后实时生成元数据（仅 Chromium 内核浏览器支持）。" />
      </div>
      <p className={`${metaLineClass} m-0 mt-auto pt-3`}>
        {props.metadata ? `${props.metadata.targetCount} 个可用目标` : "尚未加载元数据"}
      </p>
    </div>
  );
}

interface PatchPanelProps {
  patch: PatchFiles | null;
  onPatchFiles: (files: FileList | null) => void;
}

export function PatchPanel(props: PatchPanelProps) {
  return (
    <div className={setupPanelClass}>
      <SectionTitle icon={<ArchiveIcon />} title="补丁" />
      <div className="flex flex-wrap items-center gap-2.5">
        <Button component="label" startIcon={<UploadFileIcon />} variant="outlined">
          选择补丁三件套
          <input hidden multiple type="file" onChange={(event) => props.onPatchFiles(event.target.files)} />
        </Button>
        <HelpHint title="同时选择 patch 主文件、.gpu_resources、.stream 三个文件（缺失的辅助文件可省略）。" />
      </div>
      <p className={`${metaLineClass} m-0 mt-auto pt-3`}>
        <span className="inline-flex h-[5px] w-[5px] rounded-full bg-slate-300" />
        {props.patch ? props.patch.name : "尚未选择补丁"}
      </p>
    </div>
  );
}

interface TargetPanelProps {
  multiTarget: boolean;
  noPadding: boolean;
  onMultiTargetChange: (enabled: boolean) => void;
  onSourceChange: (hash: string) => void;
  onSourceReset: () => void;
  onTargetChange: (hash: string) => void;
  partialRemap: boolean;
  patchSuffix: string;
  selectedTargets: string[];
  setNoPadding: (value: boolean) => void;
  setPartialRemap: (value: boolean) => void;
  sourceChoices: TargetOption[];
  sourceHash: string;
  targetOptions: TargetOption[];
}

export function TargetPanel(props: TargetPanelProps) {
  return (
    <div className={`${panelClass} flex flex-col`}>
      <div className="flex flex-col flex-wrap items-center gap-2 border-b border-slate-100/95 bg-slate-50/80 px-6 py-3.5 min-[820px]:flex-row min-[820px]:gap-4">
        <SectionTitle icon={<CompareArrowsIcon />} title="迁移映射" inHeader />
        <div className="hidden flex-1 min-[820px]:block" />
        <FormControlLabel
          className="multiToggle"
          control={<Switch checked={props.multiTarget} onChange={(event) => props.onMultiTargetChange(event.target.checked)} />}
          label="多目标"
        />
        <HelpHint title="一次性向多个目标版本生成迁移补丁。注意：大量目标会显著增加 CPU 和内存占用。" />
      </div>

      <MappingGrid
        multiTarget={props.multiTarget}
        onSourceChange={props.onSourceChange}
        onSourceReset={props.onSourceReset}
        onTargetChange={props.onTargetChange}
        selectedTargets={props.selectedTargets}
        sourceChoices={props.sourceChoices}
        sourceHash={props.sourceHash}
        targetOptions={props.targetOptions}
      />

      <OptionsPanel
        noPadding={props.noPadding}
        partialRemap={props.partialRemap}
        patchSuffix={props.patchSuffix}
        setNoPadding={props.setNoPadding}
        setPartialRemap={props.setPartialRemap}
      />
    </div>
  );
}

interface MappingGridProps {
  multiTarget: boolean;
  onSourceChange: (hash: string) => void;
  onSourceReset: () => void;
  onTargetChange: (hash: string) => void;
  selectedTargets: string[];
  sourceChoices: TargetOption[];
  sourceHash: string;
  targetOptions: TargetOption[];
}

const MappingGrid = memo(function MappingGrid(props: MappingGridProps) {
  const selectedTargetSet = useMemo(() => new Set(props.selectedTargets), [props.selectedTargets]);

  return (
    <div className="m-3 flex min-h-[260px] flex-col items-stretch bg-slate-50/30 min-[820px]:flex-row">
      <FormControl className="flex min-w-0 flex-1 flex-col p-6 min-[820px]:pr-9">
        <div className="flex flex-row items-center gap-2">
          <FormLabel className="mappingFormLabel">源版本</FormLabel>
          <div className="flex-1" />
          {props.sourceHash && (
            <Button onClick={props.onSourceReset} size="small" startIcon={<RestartAltIcon />} variant="text">
              更改
            </Button>
          )}
        </div>
        {props.sourceChoices.length ? (
          <div className="flex max-h-[360px] flex-1 flex-col overflow-auto">
            <RadioGroup value={props.sourceHash} onChange={(event) => props.onSourceChange(event.target.value)}>
              {props.sourceChoices.map((target) => (
                <TargetRadio key={target.hash} target={target} />
              ))}
            </RadioGroup>
          </div>
        ) : (
          <EmptyMapping icon={<DescriptionIcon />} text="请先导入补丁文件" />
        )}
      </FormControl>
      <FormControl className="flex min-w-0 flex-1 flex-col border-t border-slate-100/95 p-6 min-[820px]:border-t-0 min-[820px]:border-l min-[820px]:pl-4">
        <FormLabel className="mappingFormLabel">目标版本</FormLabel>
        <div className="flex max-h-[360px] flex-1 flex-col overflow-auto">
          {props.targetOptions.length ? (
            props.targetOptions.map((target) => (
              <TargetChoice
                key={target.hash}
                multiTarget={props.multiTarget}
                onChange={props.onTargetChange}
                selected={selectedTargetSet.has(target.hash)}
                target={target}
              />
            ))
          ) : (
            <EmptyMapping icon={<TaskAltIcon />} text="目标版本输出" />
          )}
        </div>
      </FormControl>
    </div>
  );
});

interface TargetChoiceProps {
  multiTarget: boolean;
  onChange: (hash: string) => void;
  selected: boolean;
  target: TargetOption;
}

const TargetChoice = memo(function TargetChoice(props: TargetChoiceProps) {
  return (
    <div
      className="mb-1.5 flex min-h-[58px] cursor-pointer items-center rounded-xl border border-transparent px-2 py-1.5 transition-colors hover:border-slate-200 hover:bg-white"
      onClick={() => props.onChange(props.target.hash)}
    >
      {props.multiTarget ? <Checkbox checked={props.selected} /> : <Radio checked={props.selected} />}
      <TargetText target={props.target} />
    </div>
  );
});

const TargetRadio = memo(function TargetRadio({ target }: { target: TargetOption }) {
  return (
    <FormControlLabel
      className="sourceChoice"
      control={<Radio />}
      label={<TargetText target={target} />}
      value={target.hash}
    />
  );
});

function TargetText({ target }: { target: TargetOption }) {
  return (
    <div className="min-w-0">
      <p className="m-0 text-[13px] font-bold leading-[1.35] text-slate-700 [overflow-wrap:anywhere]">{target.name}</p>
      <p className="m-0 font-mono text-[11px] leading-[1.45] text-slate-400 [overflow-wrap:anywhere]">{target.hash}</p>
    </div>
  );
}

function EmptyMapping({ icon, text }: { icon: ReactNode; text: string }) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-2 rounded-xl border border-dashed border-slate-200 p-6 text-center text-slate-400 min-h-[178px] [&_svg]:text-slate-300 [&_svg]:text-[32px]">
      {icon}
      <p className="m-0 text-xs">{text}</p>
    </div>
  );
}

interface OptionsPanelProps {
  noPadding: boolean;
  partialRemap: boolean;
  patchSuffix: string;
  setNoPadding: (value: boolean) => void;
  setPartialRemap: (value: boolean) => void;
}

const OptionsPanel = memo(function OptionsPanel(props: OptionsPanelProps) {
  return (
    <div className="flex flex-wrap items-center gap-x-6 gap-y-3 border-t border-slate-100/95 bg-white px-6 py-4">
      <FormControlLabel
        className="optionsControl"
        control={<Checkbox checked={props.noPadding} onChange={(event) => props.setNoPadding(event.target.checked)} />}
        label="禁用填充"
      />
      <HelpHint title="不在仅目标版本存在的 Unit 槽位中填充空网格模板。可能导致目标缺失部件出现异常，仅在调试时建议启用。" />
      <FormControlLabel
        className="optionsControl"
        control={<Checkbox checked={props.partialRemap} onChange={(event) => props.setPartialRemap(event.target.checked)} />}
        label="部分重映射"
      />
      <HelpHint title="实验功能：即使某些 Unit 不完整也输出重映射结果，仅用于调试不完整的元数据，正常使用请勿勾选。" />
      <TextField
        className="suffixField max-[819px]:min-w-full! min-[820px]:ml-auto! min-[820px]:min-w-[280px]!"
        disabled
        label="补丁后缀"
        size="small"
        slotProps={{ input: { startAdornment: <InputAdornment position="start">文件</InputAdornment> } }}
        value={props.patchSuffix}
      />
      <HelpHint title="输出到每个目标版本目录中的补丁文件名。固定为该值以确保游戏可识别。" />
    </div>
  );
});

interface ResultPanelProps {
  errorText: string;
  result: MigrationResult | null;
}

export function ResultPanel({ errorText, result }: ResultPanelProps) {
  if (errorText) {
    return <Alert severity="error">{errorText}</Alert>;
  }
  if (!result) {
    return null;
  }
  return (
    <div className={`${panelClass} mb-6 p-5`}>
      <div className="flex flex-row items-center gap-4">
        <div>
          <p className="m-0 text-base font-bold text-slate-900">已迁移 {result.summary.migratedCount} 个目标</p>
          <p className={`${metaLineClass} m-0 mt-3`}>{result.summary.warningCount} 条警告</p>
        </div>
        <div className="flex-1" />
        <Button
          onClick={() => downloadZip(result.zipBytes, "hd2-migrated-patch.zip")}
          startIcon={<DownloadIcon />}
          variant="contained"
        >
          下载 ZIP
        </Button>
      </div>
      <div className="mt-3 rounded-xl border border-slate-100 bg-slate-50 p-3">
        {result.summary.reports.map((report) => (
          <p
            className="m-0 font-mono text-xs text-slate-700 [overflow-wrap:anywhere]"
            key={report.targetHash}
          >
            {report.targetName}：文件ID重映射={report.fileIdRemapped} 已填充={report.paddedUnits} 已跳过={report.skippedEntries}
          </p>
        ))}
      </div>
    </div>
  );
}

interface PerformanceDialogProps {
  open: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}

export function PerformanceDialog(props: PerformanceDialogProps) {
  return (
    <Dialog open={props.open} onClose={props.onCancel}>
      <DialogTitle>多目标</DialogTitle>
      <DialogContent>
        <Alert severity="warning">
          WASM 在内存中构建输出 ZIP。大量多目标迁移可能占用较多的 CPU 和内存。
        </Alert>
      </DialogContent>
      <DialogActions>
        <Button onClick={props.onCancel}>取消</Button>
        <Button onClick={props.onConfirm} variant="contained">继续</Button>
      </DialogActions>
    </Dialog>
  );
}

function HelpHint({ title }: { title: string }) {
  return (
    <Tooltip arrow title={title} placement="top">
      <HelpOutlineIcon className="cursor-help text-slate-400 hover:text-slate-600" sx={{ fontSize: 16 }} />
    </Tooltip>
  );
}

function SectionTitle({ icon, title, inHeader }: { icon: ReactNode; title: string; inHeader?: boolean }) {
  return (
    <div
      className={`flex flex-row items-center gap-2 [&_svg]:text-slate-500 [&_svg]:text-[18px] ${
        inHeader ? "" : "mb-4"
      }`}
    >
      {icon}
      <h2 className="m-0 text-sm font-bold text-slate-800">{title}</h2>
    </div>
  );
}
