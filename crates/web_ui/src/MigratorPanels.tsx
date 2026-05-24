import ArchiveIcon from "@mui/icons-material/Archive";
import CompareArrowsIcon from "@mui/icons-material/CompareArrows";
import DescriptionIcon from "@mui/icons-material/Description";
import DownloadIcon from "@mui/icons-material/Download";
import HelpOutlineIcon from "@mui/icons-material/HelpOutlined";
import InventoryIcon from "@mui/icons-material/Inventory2";
import SearchIcon from "@mui/icons-material/Search";
import TaskAltIcon from "@mui/icons-material/TaskAlt";
import UploadFileIcon from "@mui/icons-material/UploadFile";
import VerifiedIcon from "@mui/icons-material/Verified";
import {
  Alert,
  Button,
  Checkbox,
  Dialog,
  DialogActions,
  DialogContent,
  DialogTitle,
  FormControlLabel,
  InputAdornment,
  Radio,
  RadioGroup,
  Switch,
  TextField,
  Tooltip,
} from "@mui/material";
import { memo, useMemo, useState } from "react";
import type { ReactNode } from "react";
import type { AuthorityMappings, MigrationSummary, PatchInfo, TargetOption } from "./types";

const panelClass = "overflow-hidden";
const setupPanelClass = `${panelClass} flex flex-1 flex-col p-6`;
const metaLineClass =
  "flex items-center gap-[0.4375rem] text-xs text-hd2-muted [overflow-wrap:anywhere]";

interface AuthorityPanelProps {
  authority: AuthorityMappings | null;
  targetCount: number;
  crossArchiveReady: boolean;
}

export function AuthorityPanel(props: AuthorityPanelProps) {
  return (
    <div className={setupPanelClass}>
      <SectionTitle icon={<InventoryIcon />} title="数据来源" />
      {props.crossArchiveReady ? (
        <p className="m-0 text-xs leading-[1.55] text-emerald-400">
          已连接游戏 data 目录，浏览器版可执行完整跨版本迁移（与桌面/命令行版本结果一致）。
        </p>
      ) : (
        <p className="m-0 text-xs leading-[1.55] text-amber-400">
          必须先选择游戏 data 目录（下方面板）才能进行迁移。需读取源 / 目标 archive 字节；浏览器无法内置游戏数据。
        </p>
      )}
      <div className="mt-auto flex flex-col gap-1.5 pt-4">
        <p className={`${metaLineClass} m-0`}>
          <span className="inline-flex h-[0.3125rem] w-[0.3125rem] bg-hd2-faint" />
          内置 archive 索引：{props.targetCount} 个条目
        </p>
        {props.authority && (
          <p className={`${metaLineClass} m-0 text-emerald-400`}>
            <VerifiedIcon sx={{ fontSize: "0.875rem" }} />
            人工校对映射：{props.authority.armorCount} 件甲
          </p>
        )}
      </div>
    </div>
  );
}

interface PatchPanelProps {
  patch: PatchInfo | null;
  onPatchFiles: (files: FileList | null) => void;
}

export function PatchPanel(props: PatchPanelProps) {
  return (
    <div className={setupPanelClass}>
      <SectionTitle icon={<ArchiveIcon />} title="补丁" />
      <div className="flex flex-wrap items-center gap-2.5">
        <Button component="label" startIcon={<UploadFileIcon />} variant="contained">
          选择patch文件
          <input hidden multiple type="file" onChange={(event) => props.onPatchFiles(event.target.files)} />
        </Button>
        <HelpHint title="同时选择 patch 主文件、.gpu_resources、.stream 三个文件（缺失的辅助文件可省略）。" />
      </div>
      <p className={`${metaLineClass} m-0 mt-auto pt-3`}>
        <span className="inline-flex h-[0.3125rem] w-[0.3125rem] bg-hd2-faint" />
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
  onTargetChange: (hash: string) => void;
  partialRemap: boolean;
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
      <div className="flex flex-col flex-wrap items-center gap-2 border-b border-hd2-border hd2-stripes-accent px-6 py-3.5 min-[51.25rem]:flex-row min-[51.25rem]:gap-4">
        <SectionTitle icon={<CompareArrowsIcon />} title="迁移映射" inHeader />
        <div className="hidden flex-1 min-[51.25rem]:block" />
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
        onTargetChange={props.onTargetChange}
        selectedTargets={props.selectedTargets}
        sourceChoices={props.sourceChoices}
        sourceHash={props.sourceHash}
        targetOptions={props.targetOptions}
      />

      <OptionsPanel
        noPadding={props.noPadding}
        partialRemap={props.partialRemap}
        setNoPadding={props.setNoPadding}
        setPartialRemap={props.setPartialRemap}
      />
    </div>
  );
}

interface MappingGridProps {
  multiTarget: boolean;
  onSourceChange: (hash: string) => void;
  onTargetChange: (hash: string) => void;
  selectedTargets: string[];
  sourceChoices: TargetOption[];
  sourceHash: string;
  targetOptions: TargetOption[];
}

const MappingGrid = memo(function MappingGrid(props: MappingGridProps) {
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
        <MappingSectionLabel>源版本</MappingSectionLabel>
        {props.sourceChoices.length ? (
          <div className="flex max-h-[22.5rem] flex-1 flex-col overflow-auto">
            <RadioGroup value={props.sourceHash} onChange={(event) => props.onSourceChange(event.target.value)}>
              {props.sourceChoices.map((target) => (
                <TargetRadio key={target.hash} target={target} />
              ))}
            </RadioGroup>
          </div>
        ) : (
          <EmptyMapping icon={<DescriptionIcon />} text="请先导入补丁文件" />
        )}
      </div>
      <div className="flex min-w-0 flex-1 flex-col border-t border-hd2-border p-6 min-[51.25rem]:border-t-0 min-[51.25rem]:border-l min-[51.25rem]:pl-4">
        <div className="mb-3 flex items-center gap-3">
          <MappingSectionLabel inline>目标版本</MappingSectionLabel>
          <div className="flex-1" />
          <TextField
            className="targetSearchField"
            onChange={(event) => setTargetQuery(event.target.value)}
            placeholder="搜索名称或 ID"
            size="small"
            slotProps={{ input: { startAdornment: <InputAdornment position="start"><SearchIcon fontSize="small" /></InputAdornment> } }}
            value={targetQuery}
            variant="standard"
          />
        </div>
        <div className="flex max-h-[22.5rem] flex-1 flex-col overflow-auto">
          {filteredTargets.length ? (
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
            <EmptyMapping icon={<TaskAltIcon />} text={targetQuery ? "没有匹配的目标版本" : "目标版本输出"} />
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

function filterTargets(targets: TargetOption[], query: string) {
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
  target: TargetOption;
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
      <p className="m-0 text-[0.8125rem] font-bold leading-[1.35] text-hd2-text [overflow-wrap:anywhere]">{target.name}</p>
      <p className="m-0 font-mono text-[0.6875rem] leading-[1.45] text-hd2-faint [overflow-wrap:anywhere]">{target.hash}</p>
    </div>
  );
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
  partialRemap: boolean;
  setNoPadding: (value: boolean) => void;
  setPartialRemap: (value: boolean) => void;
}

const OptionsPanel = memo(function OptionsPanel(props: OptionsPanelProps) {
  return (
    <div className="flex flex-wrap items-center gap-x-6 gap-y-3 border-t border-hd2-border bg-hd2-pit px-6 py-4">
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
    </div>
  );
});

interface ResultPanelProps {
  errorText: string;
  onDownload: () => void;
  summary: MigrationSummary | null;
}

export function ResultPanel({ errorText, onDownload, summary }: ResultPanelProps) {
  if (errorText) {
    return <Alert severity="error">{errorText}</Alert>;
  }
  if (!summary) {
    return null;
  }
  return (
    <div className={`${panelClass} p-5`}>
      <div className="flex flex-row items-center gap-4">
        <div>
          <p className="m-0 text-base font-bold text-hd2-text">已迁移 {summary.migratedCount} 个目标</p>
          <p className={`${metaLineClass} m-0 mt-3`}>{summary.warningCount} 条警告</p>
        </div>
        <div className="flex-1" />
        <Button
          onClick={onDownload}
          startIcon={<DownloadIcon />}
          variant="contained"
        >
          下载 ZIP
        </Button>
      </div>
      <div className="mt-3 border border-hd2-border bg-hd2-sunken p-3">
        {summary.reports.map((report) => (
          <p
            className="m-0 font-mono text-xs text-hd2-muted [overflow-wrap:anywhere]"
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
      <HelpOutlineIcon className="cursor-help text-hd2-faint hover:text-hd2-yellow" sx={{ fontSize: "1rem" }} />
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
