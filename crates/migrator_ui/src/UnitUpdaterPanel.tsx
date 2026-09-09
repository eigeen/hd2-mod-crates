import { ToggleButton, ToggleButtonGroup } from "@mui/material";
import {
  AdvancedOptionRow,
  AdvancedOptionsPopover,
} from "./AdvancedOptionsPopover";
import { useI18n } from "./i18n";
import type { MissingUnitPolicy } from "./types";

interface UnitUpdaterPanelProps {
  missingUnitPolicy: MissingUnitPolicy;
  onMissingUnitPolicyChange: (policy: MissingUnitPolicy) => void;
}

export function UnitUpdaterPanel(props: UnitUpdaterPanelProps) {
  const { t } = useI18n();

  return (
    <AdvancedOptionsPopover id="repatch-advanced-options" label={t("options.advanced")}>
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
