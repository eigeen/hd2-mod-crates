import { Checkbox, FormControl, FormControlLabel, InputLabel, MenuItem, Select } from "@mui/material";
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
    <div className="m-3 flex flex-col gap-3 bg-hd2-sunken p-6">
      <h2 className="m-0 text-xs font-bold uppercase tracking-[0.08em] text-hd2-faint">
        {t("repatch.settings")}
      </h2>
      <div className="border-l-2 border-hd2-yellow/50 pl-3">
        <p className="m-0 mb-1 text-[0.6875rem] font-bold uppercase tracking-[0.08em] text-hd2-faint">
          {t("options.advanced")}
        </p>
        <FormControlLabel
          control={(
            <Checkbox
              checked={props.cullingPolicy === "target"}
              onChange={(event) => props.onCullingPolicyChange(event.target.checked ? "target" : "patch")}
            />
          )}
          label={t("options.patchCulling")}
        />
        <p className="m-0 max-w-[42rem] text-xs text-hd2-muted">{t("options.patchCullingHelp")}</p>
        {props.cullingSummary && (
          <div className="mt-2 flex flex-col gap-1 text-xs text-hd2-muted">
            <span>{t("repatch.patchCullingSummary", { ...props.cullingSummary.patch })}</span>
            <span>
              {props.cullingSummary.target
                ? t("repatch.targetCullingSummary", { ...props.cullingSummary.target })
                : t("repatch.targetCullingUnavailable")}
            </span>
          </div>
        )}
      </div>
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
