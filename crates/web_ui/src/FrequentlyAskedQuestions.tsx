import CloseIcon from "@mui/icons-material/Close";
import HelpOutlineIcon from "@mui/icons-material/HelpOutlined";
import KeyboardArrowDownIcon from "@mui/icons-material/KeyboardArrowDown";
import { IconButton } from "@mui/material";
import { useEffect, useRef } from "react";
import { useI18n, type LanguageCode } from "./i18n";
import type { TranslationKey } from "./locales/translationKeys";

interface FaqEntry {
  answer: TranslationKey;
  href?: string;
  language?: LanguageCode;
  question: TranslationKey;
  recommendation?: TranslationKey;
}

const entries: FaqEntry[] = [
  {
    question: "faq.workaroundQuestion",
    recommendation: "faq.workaroundRecommendation",
    answer: "faq.workaroundAnswer",
  },
  { question: "faq.readOnlyQuestion", answer: "faq.readOnlyAnswer" },
  {
    question: "faq.partsTableQuestion",
    answer: "faq.partsTableAnswer",
    href: "https://www.kdocs.cn/l/cjYiKiuvtSoF",
    language: "zh-CN",
  },
];

interface FrequentlyAskedQuestionsProps {
  attentionQuestion: TranslationKey | null;
  onOpenChange: (open: boolean) => void;
  open: boolean;
}

export function FrequentlyAskedQuestions(props: FrequentlyAskedQuestionsProps) {
  const { t } = useI18n();

  return (
    <aside
      aria-label={t("faq.title")}
      className="fixed right-0 top-24 z-20 flex max-h-[calc(100vh-7rem)] items-start min-[96rem]:right-4"
      onKeyDown={(event) => closeOnEscape(event.key, props.onOpenChange)}
    >
      <button
        aria-controls="faq-panel"
        aria-expanded={props.open}
        aria-label={t(props.open ? "faq.close" : "faq.open")}
        className="flex w-11 cursor-pointer flex-col items-center gap-2 border border-r-0 border-hd2-border bg-hd2-surface/95 px-2 py-3 text-hd2-text hover:border-hd2-yellow hover:bg-hd2-yellow-bg hover:text-hd2-yellow"
        onClick={() => props.onOpenChange(!props.open)}
        title={t(props.open ? "faq.close" : "faq.open")}
        type="button"
      >
        <HelpOutlineIcon fontSize="small" />
        <span className="text-xs font-bold tracking-widest [writing-mode:vertical-rl]">
          {t("faq.title")}
        </span>
      </button>
      <div
        aria-hidden={!props.open}
        className={`faqDrawer ${props.open ? "faqDrawerOpen" : "faqDrawerClosed"}`}
        inert={!props.open}
      >
        <FaqPanel
          attentionQuestion={props.attentionQuestion}
          onClose={() => props.onOpenChange(false)}
        />
      </div>
    </aside>
  );
}

function FaqPanel(props: Pick<FrequentlyAskedQuestionsProps, "attentionQuestion"> & { onClose: () => void }) {
  const { language, t } = useI18n();
  const visibleEntries = entries.filter((entry) => !entry.language || entry.language === language);

  return (
    <section
      className="hd2-scroll max-h-[calc(100vh-7rem)] w-[min(22rem,calc(100vw-3.75rem))] overflow-y-auto border border-hd2-border bg-black/95 shadow-[-0.5rem_0_1.5rem_rgba(0,0,0,0.35)]"
      id="faq-panel"
      aria-labelledby="faq-title"
    >
      <div className="sticky top-0 z-[1] flex items-center gap-2 border-b border-hd2-border bg-hd2-surface px-4 py-3">
        <HelpOutlineIcon className="text-hd2-yellow" fontSize="small" />
        <h2 className="m-0 flex-1 text-sm font-bold text-hd2-text" id="faq-title">{t("faq.title")}</h2>
        <IconButton aria-label={t("faq.close")} onClick={props.onClose} size="small">
          <CloseIcon fontSize="small" />
        </IconButton>
      </div>
      {visibleEntries.map((entry) => (
        <FaqItem
          attention={props.attentionQuestion === entry.question}
          entry={entry}
          key={entry.question}
        />
      ))}
    </section>
  );
}

function FaqItem({ attention, entry }: { attention: boolean; entry: FaqEntry }) {
  const { t } = useI18n();
  const detailsRef = useRef<HTMLDetailsElement>(null);

  useEffect(() => {
    if (attention && detailsRef.current) detailsRef.current.open = true;
  }, [attention]);

  return (
    <details className="group border-b border-hd2-border last:border-b-0" ref={detailsRef}>
      <summary className="flex cursor-pointer list-none items-center gap-3 px-5 py-3 text-sm font-semibold text-hd2-text hover:bg-hd2-yellow-bg">
        <span className="flex-1">{t(entry.question)}</span>
        <KeyboardArrowDownIcon className="text-hd2-muted transition-transform group-open:rotate-180" fontSize="small" />
      </summary>
      <div className="border-t border-hd2-border px-5 py-3 text-xs leading-6">
        {entry.recommendation && (
          <p className="m-0 border-l-2 border-hd2-yellow bg-hd2-yellow-bg px-3 py-2 font-semibold text-hd2-text">
            {t(entry.recommendation)}
          </p>
        )}
        <p className={`m-0 text-hd2-muted ${entry.recommendation ? "mt-3" : ""}`}>
          {entry.href ? (
            <a
              className="font-semibold text-hd2-yellow underline underline-offset-2"
              href={entry.href}
              rel="noreferrer"
              target="_blank"
            >
              {t(entry.answer)}
            </a>
          ) : t(entry.answer)}
        </p>
      </div>
    </details>
  );
}

function closeOnEscape(key: string, onOpenChange: (open: boolean) => void): void {
  if (key === "Escape") onOpenChange(false);
}
