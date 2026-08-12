import { Alert } from "@mui/material";
import { useMemo } from "react";
import { useI18n } from "./i18n";
import type { DetectedModel, MigrationCategory } from "./types";

interface UnclaimedModelAlertProps {
  currentCategory: MigrationCategory;
  currentSourceName: string | null;
  detectedModels: DetectedModel[];
}

export function UnclaimedModelAlert(props: UnclaimedModelAlertProps) {
  const { t } = useI18n();
  const candidates = useMemo(
    () => unclaimedModels(props),
    [props.currentCategory, props.currentSourceName, props.detectedModels],
  );
  if (!props.currentSourceName || candidates.length === 0) return null;

  return (
    <div className="border-t border-hd2-border px-5 py-3">
      <Alert severity="warning" variant="outlined">
        <div className="font-bold">{t("mixedPatch.title")}</div>
        <div className="text-sm">{t("mixedPatch.body")}</div>
        <ul className="mb-0 mt-1 pl-5 text-sm">
          {candidates.map((model) => (
            <li key={`${model.category}:${model.name}`}>
              {t("mixedPatch.candidate", {
                category: categoryLabel(model.category, t),
                count: model.unitHits,
                name: model.name,
              })}
            </li>
          ))}
        </ul>
        <div className="mt-1 text-sm">{t("mixedPatch.action")}</div>
      </Alert>
    </div>
  );
}

function unclaimedModels(props: UnclaimedModelAlertProps): DetectedModel[] {
  return props.detectedModels.filter((model) => (
    model.category !== props.currentCategory || model.name !== props.currentSourceName
  ));
}

function categoryLabel(
  category: MigrationCategory,
  t: ReturnType<typeof useI18n>["t"],
): string {
  return t(category === "Armor" ? "mapping.armor" : "mapping.helmet");
}
