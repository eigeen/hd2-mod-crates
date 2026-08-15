import { FormControl, InputLabel, MenuItem, Select } from "@mui/material";
import { useI18n } from "./i18n";
import type { MissingUnitPolicy } from "./types";

interface UnitUpdaterPanelProps {
  missingUnitPolicy: MissingUnitPolicy;
  onMissingUnitPolicyChange: (policy: MissingUnitPolicy) => void;
}

export function UnitUpdaterPanel(props: UnitUpdaterPanelProps) {
  const { t } = useI18n();

  return (
    <div className="m-3 flex flex-col gap-3 bg-hd2-sunken p-6">
      <h2 className="m-0 text-xs font-bold uppercase tracking-[0.08em] text-hd2-faint">
        {t("repatch.settings")}
      </h2>
      <FormControl className="max-w-[24rem]" size="small">
        <InputLabel id="missing-unit-policy-label">{t("repatch.missingPolicy")}</InputLabel>
        <Select
          label={t("repatch.missingPolicy")}
          labelId="missing-unit-policy-label"
          onChange={(event) => props.onMissingUnitPolicyChange(event.target.value as MissingUnitPolicy)}
          value={props.missingUnitPolicy}
        >
          <MenuItem value="drop">{t("repatch.missingDrop")}</MenuItem>
          <MenuItem value="keep">{t("repatch.missingKeep")}</MenuItem>
          <MenuItem value="fail">{t("repatch.missingFail")}</MenuItem>
        </Select>
      </FormControl>
    </div>
  );
}
