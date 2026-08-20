import DownloadIcon from "@mui/icons-material/Download";
import MemoryIcon from "@mui/icons-material/Memory";
import WarningAmberIcon from "@mui/icons-material/WarningAmber";
import { Button } from "@mui/material";
import {
  Hd2Dialog,
  Hd2DialogActions,
  Hd2DialogContent,
  Hd2DialogTitle,
  useI18n,
  type TranslationKey,
} from "@hd2-mod-tools/migrator-ui";
import type { ReactNode } from "react";
import { DESKTOP_RELEASE_URL } from "./desktopGuidancePolicy";

export function DesktopDownloadEntry() {
  const { t } = useI18n();

  return (
    <div className="flex items-center gap-3 border-b border-hd2-border bg-hd2-pit px-4 py-3 min-[35rem]:px-5">
      <MemoryIcon className="shrink-0 text-hd2-yellow" fontSize="small" />
      <div className="min-w-0 flex-1">
        <p className="m-0 text-xs font-bold text-hd2-text">{t("desktopGuide.entryTitle")}</p>
        <p className="m-0 mt-0.5 hidden text-[0.6875rem] text-hd2-muted min-[35rem]:block">
          {t("desktopGuide.entryBody")}
        </p>
      </div>
      <DesktopDownloadButton />
    </div>
  );
}

export function DesktopDownloadButton({ fullWidth = false }: { fullWidth?: boolean }) {
  const { t } = useI18n();

  return (
    <Button
      className="shrink-0"
      component="a"
      fullWidth={fullWidth}
      href={DESKTOP_RELEASE_URL}
      rel="noreferrer"
      size="small"
      startIcon={<DownloadIcon />}
      target="_blank"
      variant="outlined"
    >
      {t("desktopGuide.download")}
    </Button>
  );
}

interface DesktopRecommendationProps {
  body: TranslationKey;
  title: TranslationKey;
  values?: Record<string, number>;
}

export function DesktopRecommendation(props: DesktopRecommendationProps) {
  const { t } = useI18n();

  return (
    <aside className="mx-3 mb-3 flex flex-col gap-3 border border-hd2-yellow/40 bg-hd2-yellow-bg px-4 py-3 min-[35rem]:flex-row min-[35rem]:items-center">
      <WarningAmberIcon className="hidden shrink-0 text-hd2-yellow min-[35rem]:block" fontSize="small" />
      <div className="min-w-0 flex-1">
        <p className="m-0 text-xs font-bold text-hd2-text">{t(props.title, props.values)}</p>
        <p className="m-0 mt-1 text-xs leading-5 text-hd2-muted">{t(props.body, props.values)}</p>
      </div>
      <DesktopDownloadButton />
    </aside>
  );
}

interface ContinueWebGuidanceDialogProps {
  onCancel: () => void;
  onContinue: () => void;
  open: boolean;
}

export function MultiTargetGuidanceDialog(props: ContinueWebGuidanceDialogProps) {
  const { t } = useI18n();

  return (
    <Hd2Dialog onClose={props.onCancel} open={props.open}>
      <Hd2DialogTitle>{t("performance.title")}</Hd2DialogTitle>
      <Hd2DialogContent>
        <GuidanceBody>
          <p className="m-0">{t("performance.body")}</p>
          <p className="m-0 mt-3 border-l-2 border-hd2-yellow pl-3 font-semibold text-hd2-text">
            {t("desktopGuide.multiTargetRecommendation")}
          </p>
        </GuidanceBody>
      </Hd2DialogContent>
      <ContinueWebDialogActions onCancel={props.onCancel} onContinue={props.onContinue} />
    </Hd2Dialog>
  );
}

export function RepatchGuidanceDialog(props: ContinueWebGuidanceDialogProps) {
  const { t } = useI18n();

  return (
    <Hd2Dialog onClose={props.onCancel} open={props.open}>
      <Hd2DialogTitle>{t("desktopGuide.repatchTitle")}</Hd2DialogTitle>
      <Hd2DialogContent>
        <GuidanceBody>{t("desktopGuide.repatchBody")}</GuidanceBody>
      </Hd2DialogContent>
      <ContinueWebDialogActions onCancel={props.onCancel} onContinue={props.onContinue} />
    </Hd2Dialog>
  );
}

interface WebMappingLimitDialogProps {
  count: number;
  max: number;
  onClose: () => void;
  open: boolean;
}

export function WebMappingLimitDialog(props: WebMappingLimitDialogProps) {
  const { t } = useI18n();
  const values = { count: props.count, max: props.max };

  return (
    <Hd2Dialog onClose={props.onClose} open={props.open}>
      <Hd2DialogTitle>{t("desktopGuide.limitTitle")}</Hd2DialogTitle>
      <Hd2DialogContent>
        <GuidanceBody>
          <p className="m-0">{t("app.blockerWebMappingLimit", values)}</p>
          <p className="m-0 mt-3 text-hd2-text">{t("desktopGuide.limitBody")}</p>
        </GuidanceBody>
      </Hd2DialogContent>
      <Hd2DialogActions>
        <Button onClick={props.onClose}>{t("desktopGuide.adjustSelection")}</Button>
        <DesktopDownloadButton />
      </Hd2DialogActions>
    </Hd2Dialog>
  );
}

export function DesktopErrorRecommendation({ children }: { children: ReactNode }) {
  const { t } = useI18n();

  return (
    <div>
      <div>{children}</div>
      <a
        className="mt-2 inline-block font-semibold text-hd2-yellow underline underline-offset-2"
        href={DESKTOP_RELEASE_URL}
        rel="noreferrer"
        target="_blank"
      >
        {t("desktopGuide.memoryFallback")}
      </a>
    </div>
  );
}

function GuidanceBody({ children }: { children: ReactNode }) {
  return (
    <div className="border border-hd2-yellow/30 bg-hd2-yellow-bg px-4 py-3 text-sm leading-6 text-hd2-muted">
      {children}
    </div>
  );
}

function ContinueWebDialogActions(props: Pick<ContinueWebGuidanceDialogProps, "onCancel" | "onContinue">) {
  const { t } = useI18n();
  return (
    <Hd2DialogActions>
      <Button onClick={props.onCancel}>{t("dialog.cancel")}</Button>
      <DesktopDownloadButton />
      <Button onClick={props.onContinue} variant="contained">
        {t("desktopGuide.continueWeb")}
      </Button>
    </Hd2DialogActions>
  );
}
