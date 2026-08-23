import CheckCircleIcon from "@mui/icons-material/CheckCircle";
import WarningAmberIcon from "@mui/icons-material/WarningAmber";
import { Button } from "@mui/material";
import {
  Hd2Dialog,
  Hd2DialogActions,
  Hd2DialogContent,
  Hd2DialogTitle,
} from "./Hd2Dialog";
import { useI18n, type Translate } from "./i18n";
import type {
  EquipmentOption,
  MigrationReportRow,
  MigrationSummary,
  UnitRepatchSummary,
} from "./types";
import type { CompletedTaskReport } from "./taskReportHistory";

interface ResultReportDialogProps {
  equipmentOptions: EquipmentOption[];
  onClose: () => void;
  onRevealOutput?: (output: string) => void;
  report: CompletedTaskReport | null;
}

export function ResultReportDialog(props: ResultReportDialogProps) {
  const { t } = useI18n();
  const hasWarnings = reportHasWarnings(props.report);
  return (
    <Hd2Dialog open={Boolean(props.report)} onClose={props.onClose} size="large">
      <Hd2DialogTitle>
        {props.report?.kind === "repatch" ? t("result.repatchTitle") : t("result.migrationTitle")}
      </Hd2DialogTitle>
      <Hd2DialogContent>
        {props.report && (
          <div className="hd2-scroll max-h-[70vh] space-y-4 overflow-y-auto pr-1 text-sm text-hd2-text">
            <StatusBanner hasWarnings={hasWarnings} t={t} />
            <OutputPath output={props.report.output} t={t} />
            {props.report.kind === "migration" ? (
              <MigrationDetails
                equipmentOptions={props.equipmentOptions}
                summary={props.report.summary}
                t={t}
              />
            ) : (
              <RepatchDetails summary={props.report.summary} t={t} />
            )}
          </div>
        )}
      </Hd2DialogContent>
      <Hd2DialogActions>
        {props.report && props.onRevealOutput && (
          <Button onClick={() => props.onRevealOutput?.(props.report!.output)} variant="outlined">
            {t("result.revealOutput")}
          </Button>
        )}
        <Button onClick={props.onClose} variant="contained">{t("result.close")}</Button>
      </Hd2DialogActions>
    </Hd2Dialog>
  );
}

function StatusBanner({ hasWarnings, t }: { hasWarnings: boolean; t: Translate }) {
  const Icon = hasWarnings ? WarningAmberIcon : CheckCircleIcon;
  return (
    <div className={`flex items-center gap-2 border px-3 py-2 ${hasWarnings ? "border-amber-600/60 text-amber-300" : "border-emerald-700/60 text-emerald-300"}`}>
      <Icon fontSize="small" />
      <span className="font-bold">
        {hasWarnings ? t("result.completedWithWarnings") : t("result.completed")}
      </span>
    </div>
  );
}

function OutputPath({ output, t }: { output: string; t: Translate }) {
  return (
    <div>
      <p className="mb-1 text-xs font-bold uppercase tracking-wide text-hd2-muted">{t("result.output")}</p>
      <p className="m-0 break-all border border-hd2-border bg-black/40 px-3 py-2 font-mono text-xs">{output}</p>
    </div>
  );
}

interface MigrationDetailsProps {
  equipmentOptions: EquipmentOption[];
  summary: MigrationSummary;
  t: Translate;
}

function MigrationDetails(props: MigrationDetailsProps) {
  return (
    <div className="space-y-3">
      {props.summary.reports.map((report, index) => (
        <MigrationTarget
          equipmentOptions={props.equipmentOptions}
          key={`${report.targetHash}-${index}`}
          report={report}
          t={props.t}
        />
      ))}
    </div>
  );
}

function MigrationTarget(props: {
  equipmentOptions: EquipmentOption[];
  report: MigrationReportRow;
  t: Translate;
}) {
  const { report, t } = props;
  return (
    <section className="border border-hd2-border bg-black/25">
      <h3 className="m-0 border-b border-hd2-border px-3 py-2 text-sm text-hd2-yellow">
        {report.targetName}
      </h3>
      <div className="space-y-3 p-3">
        <div className="space-y-1">
          {report.mappings.map((mapping) => (
            <div className="flex flex-wrap items-center gap-2 text-xs" key={`${mapping.category}-${mapping.sourceHash}-${mapping.targetHash}`}>
              <span className="text-hd2-muted">{mapping.category === "Armor" ? t("mapping.armor") : t("mapping.helmet")}</span>
              <span>{equipmentName(mapping.sourceHash, props.equipmentOptions)}</span>
              <span className="text-hd2-yellow">→</span>
              <span>{equipmentName(mapping.targetHash, props.equipmentOptions)}</span>
            </div>
          ))}
        </div>
        <MetricGrid metrics={migrationMetrics(report, t)} />
        <p className="m-0 text-xs text-hd2-muted">
          {report.unmatchedUnitPolicy === "keep"
            ? t("result.unmatchedKept", { count: report.unmatchedUnits })
            : t("result.unmatchedDropped", { count: report.unmatchedUnits })}
        </p>
        <WarningList warnings={report.warnings} t={t} />
      </div>
    </section>
  );
}

function RepatchDetails({ summary, t }: { summary: UnitRepatchSummary; t: Translate }) {
  return (
    <section className="space-y-3 border border-hd2-border bg-black/25 p-3">
      <MetricGrid metrics={[
        [t("result.updated"), summary.updatedUnits],
        [t("result.formats"), summary.convertedFormats ?? 0],
        [t("result.current"), summary.alreadyCurrentUnits],
        [t("result.removed"), summary.removedUnits],
        [t("result.archives"), summary.scannedArchives],
      ]} />
      <WarningList warnings={summary.warnings} t={t} />
    </section>
  );
}

function MetricGrid({ metrics }: { metrics: Array<[string, number]> }) {
  return (
    <div className="grid grid-cols-2 gap-2 min-[32rem]:grid-cols-4">
      {metrics.map(([label, value]) => (
        <div className="border border-hd2-border bg-black/30 px-2 py-2" key={label}>
          <div className="text-lg font-bold text-hd2-text">{value}</div>
          <div className="text-[0.6875rem] text-hd2-muted">{label}</div>
        </div>
      ))}
    </div>
  );
}

function WarningList({ warnings, t }: { warnings: string[]; t: Translate }) {
  if (!warnings.length) return null;
  return (
    <div>
      <p className="mb-1 text-xs font-bold text-amber-300">{t("result.warningDetails")}</p>
      <ul className="m-0 space-y-1 pl-5 text-xs text-hd2-muted">
        {warnings.map((warning, index) => <li key={`${warning}-${index}`}>{warning}</li>)}
      </ul>
    </div>
  );
}

function migrationMetrics(report: MigrationReportRow, t: Translate): Array<[string, number]> {
  return [
    [t("result.fileIds"), report.fileIdRemapped],
    [t("result.slots"), report.slotIdRemapped],
    [t("result.padded"), report.paddedUnits],
    [t("result.skipped"), report.skippedEntries],
  ];
}

function equipmentName(hash: string, options: EquipmentOption[]): string {
  const option = options.find((candidate) => candidate.hash === hash);
  return option ? `${option.name} · ${hash}` : hash;
}

function reportHasWarnings(report: CompletedTaskReport | null): boolean {
  if (!report) return false;
  if (report.kind === "migration") return report.summary.warningCount > 0;
  return report.summary.warnings.length > 0;
}
