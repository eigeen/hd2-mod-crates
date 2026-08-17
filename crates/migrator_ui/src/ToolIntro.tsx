import InfoOutlinedIcon from "@mui/icons-material/InfoOutlined";
import WarningAmberIcon from "@mui/icons-material/WarningAmber";
import { useI18n } from "./i18n";

type ToolMode = "migrate" | "repatch" | "merge";

export function ToolIntro({ mode }: { mode: ToolMode }) {
  const { t } = useI18n();
  const intro = introKeys(mode);

  return (
    <section className="border-t border-hd2-border bg-hd2-surface/35 px-5 py-4 min-[51.25rem]:px-6">
      <div className="flex items-start gap-3">
        <InfoOutlinedIcon className="mt-0.5 shrink-0 text-hd2-yellow" fontSize="small" />
        <div className="min-w-0">
          <h2 className="m-0 text-sm font-bold text-hd2-text">
            {t(intro.title)}
          </h2>
          {intro.principle && (
            <p className="mb-0 mt-1.5 text-xs leading-5 text-hd2-muted">
              {t(intro.principle)}
            </p>
          )}
          <p className="mb-0 mt-2 flex items-start gap-1.5 text-xs leading-5 text-hd2-yellow/90">
            <WarningAmberIcon className="mt-0.5 shrink-0" sx={{ fontSize: "0.875rem" }} />
            <span>{t(intro.warning)}</span>
          </p>
          {mode === "repatch" && <RepatchCredit />}
          {mode === "migrate" && <MigrationCredit />}
        </div>
      </div>
    </section>
  );
}

function introKeys(mode: ToolMode) {
  if (mode === "merge") {
    return {
      title: "intro.mergeTitle",
      principle: null,
      warning: "intro.mergeWarning",
    } as const;
  }
  if (mode === "repatch") {
    return {
      title: "intro.repatchTitle",
      principle: "intro.repatchPrinciple",
      warning: "intro.repatchWarning",
    } as const;
  }
  return {
    title: "intro.migrateTitle",
    principle: "intro.migratePrinciple",
    warning: "intro.migrateWarning",
  } as const;
}

function MigrationCredit() {
  const { t } = useI18n();

  return (
    <p className="mb-0 mt-2 text-xs leading-5 text-hd2-faint">
      {t("intro.migrateCreditPrefix")}
      <a
        className="text-hd2-yellow underline underline-offset-2"
        href="https://space.bilibili.com/263230957"
        rel="noreferrer"
        target="_blank"
      >
        @大紫
      </a>
      {t("intro.migrateCreditMappingSuffix")}
      {t("intro.migrateCreditDesignPrefix")}
      <a
        className="text-hd2-yellow underline underline-offset-2"
        href="https://github.com/S1lverAkatsuki/"
        rel="noreferrer"
        target="_blank"
      >
        @S1lverAkatsuki
      </a>
      {t("intro.migrateCreditSuffix")}
    </p>
  );
}

function RepatchCredit() {
  const { t } = useI18n();

  return (
    <p className="mb-0 mt-2 text-xs leading-5 text-hd2-faint">
      {t("intro.repatchCreditPrefix")}
      <a
        className="text-hd2-yellow underline underline-offset-2"
        href="https://github.com/RaidingForPants/hd2-repatcher"
        rel="noreferrer"
        target="_blank"
      >
        {t("intro.repatchCreditLink")}
      </a>
      {t("intro.repatchCreditSuffix")}
    </p>
  );
}
