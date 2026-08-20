import { ToggleButton, ToggleButtonGroup } from "@mui/material";
import {
  AdvancedCheckboxOption,
  AdvancedOptionRow,
  AdvancedOptionsPopover,
} from "./AdvancedOptionsPopover";
import { useI18n } from "./i18n";
import type { CullingPolicy, MissingUnitPolicy, RepatchCullingSummary } from "./types";

interface UnitUpdaterPanelProps {
  cullingPolicy: CullingPolicy;
  missingUnitPolicy: MissingUnitPolicy;
  cullingSummary: RepatchCullingSummary | null;
  onCullingPolicyChange: (policy: CullingPolicy) => void;
  onMissingUnitPolicyChange: (policy: MissingUnitPolicy) => void;
}

export function UnitUpdaterPanel(props: UnitUpdaterPanelProps) {
  const { t } = useI18n();

  return (
    <AdvancedOptionsPopover id="repatch-advanced-options" label={t("options.advanced")}>
      <AdvancedCheckboxOption
        checked={props.cullingPolicy === "target"}
        help={t("options.patchCullingHelp")}
        label={t("options.patchCulling")}
        onChange={(checked) => props.onCullingPolicyChange(checked ? "target" : "patch")}
      />
      {props.cullingSummary && <CullingSummary summary={props.cullingSummary} />}
      <MissingUnitPolicyOption {...props} />
    </AdvancedOptionsPopover>
  );
}

function MissingUnitPolicyOption(props: UnitUpdaterPanelProps) {
  const { t } = useI18n();
  return (
    <AdvancedOptionRow label={t("repatch.missingPolicy")}>
      <ToggleButtonGroup
        exclusive
        onChange={(_, value: MissingUnitPolicy | null) => value && props.onMissingUnitPolicyChange(value)}
        size="small"
        value={props.missingUnitPolicy}
      >
        <ToggleButton value="drop">{t("repatch.missingDrop")}</ToggleButton>
        <ToggleButton value="keep">{t("repatch.missingKeep")}</ToggleButton>
        <ToggleButton value="fail">{t("repatch.missingFail")}</ToggleButton>
      </ToggleButtonGroup>
    </AdvancedOptionRow>
  );
}

function CullingSummary({ summary }: { summary: RepatchCullingSummary }) {
  const { t } = useI18n();
  return (
    <div className="flex flex-col gap-1 px-1 text-xs text-hd2-muted">
      <span>{t("repatch.patchCullingSummary", { ...summary.patch })}</span>
      <span>
        {summary.target
          ? t("repatch.targetCullingSummary", { ...summary.target })
          : t("repatch.targetCullingUnavailable")}
      </span>
    </div>
  );
}
