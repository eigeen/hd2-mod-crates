import ArrowBackIosNewIcon from "@mui/icons-material/ArrowBackIosNew";
import ArrowForwardIosIcon from "@mui/icons-material/ArrowForwardIos";
import CloseIcon from "@mui/icons-material/Close";
import MenuBookIcon from "@mui/icons-material/MenuBook";
import { Button, CircularProgress, IconButton, Tooltip } from "@mui/material";
import { useEffect } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { Hd2Dialog } from "./Hd2Dialog";
import { useI18n } from "./i18n";
import type { UpdateInfoController } from "./useUpdateInfo";

export function UpdateInfoButton(props: Pick<UpdateInfoController, "available" | "openLatest">) {
  const { t } = useI18n();
  return (
    <Tooltip title={t("updateInfo.open")}>
      <span>
        <IconButton
          aria-label={t("updateInfo.open")}
          className="headerIconBtn"
          disabled={!props.available}
          onClick={props.openLatest}
          size="small"
        >
          <MenuBookIcon fontSize="small" />
        </IconButton>
      </span>
    </Tooltip>
  );
}

export function UpdateInfoDialog({ controller }: { controller: UpdateInfoController }) {
  const { language, t } = useI18n();
  const release = controller.currentRelease;
  useUpdateInfoKeyboard(controller);
  if (!release) return null;
  const title = t("updateInfo.title", { version: release.version });
  return (
    <Hd2Dialog ariaLabel={title} onClose={controller.close} open={controller.isOpen} size="guide">
      <div className="flex h-[min(84vh,44rem)] flex-col">
        <div className="hd2-stripes-accent flex items-center gap-3 border-b border-hd2-border px-4 py-3 min-[35rem]:px-6">
          <div className="min-w-0 flex-1">
            <div className="truncate text-sm font-bold uppercase tracking-[0.1em] text-hd2-yellow">{title}</div>
            <div className="mt-1 truncate text-xs text-hd2-muted">
              {release.titles[language]} · {t("updateInfo.releasedAt", { date: release.releasedAt })}
            </div>
          </div>
          <Tooltip title={t("updateInfo.close")}>
            <IconButton aria-label={t("updateInfo.close")} className="headerIconBtn" onClick={controller.close} size="small">
              <CloseIcon fontSize="small" />
            </IconButton>
          </Tooltip>
        </div>

        <div className="flex min-h-0 flex-1 items-stretch">
          <PageArrow
            disabled={controller.pageIndex === 0}
            label={t("updateInfo.newer")}
            onClick={() => controller.goToPage(controller.pageIndex - 1)}
            side="newer"
          />
          <main className="min-w-0 flex-1 overflow-x-hidden overflow-y-auto px-5 py-5 min-[35rem]:px-8" aria-live="polite">
            <div
              className={pageTransitionClass(controller.navigationDirection)}
              key={`${release.id}-${language}`}
            >
              <UpdatePage controller={controller} />
            </div>
          </main>
          <PageArrow
            disabled={controller.pageIndex >= controller.releases.length - 1}
            label={t("updateInfo.older")}
            onClick={() => controller.goToPage(controller.pageIndex + 1)}
            side="older"
          />
        </div>

        <div className="grid grid-cols-[1fr_auto_1fr] items-center gap-3 border-t border-hd2-border bg-hd2-pit px-4 py-3">
          <span className="text-xs text-hd2-muted">
            {t("updateInfo.page", { current: controller.pageIndex + 1, total: controller.releases.length })}
          </span>
          <PageDots controller={controller} />
          <Button className="justify-self-end" onClick={controller.close} size="small" variant="contained">
            {t("updateInfo.close")}
          </Button>
        </div>
      </div>
    </Hd2Dialog>
  );
}

function UpdatePage({ controller }: { controller: UpdateInfoController }) {
  const { t } = useI18n();
  if (controller.loading) {
    return <div className="flex h-full items-center justify-center gap-3 text-sm text-hd2-muted"><CircularProgress size="1.25rem" />{t("updateInfo.loading")}</div>;
  }
  if (controller.error || !controller.currentPage) {
    return <div className="flex h-full items-center justify-center text-sm text-hd2-muted">{t("updateInfo.loadFailed")}</div>;
  }
  const page = controller.currentPage;
  return (
    <div className="updateInfoMarkdown">
      <ReactMarkdown
        components={{
          a: ({ href, children }) => <a href={href} rel="noreferrer" target="_blank">{children}</a>,
          img: ({ alt, src }) => <img alt={alt ?? ""} loading="lazy" src={resolveContentUrl(page.sourceUrl, src)} />,
        }}
        remarkPlugins={[remarkGfm]}
      >
        {page.markdown}
      </ReactMarkdown>
    </div>
  );
}

interface PageArrowProps {
  disabled: boolean;
  label: string;
  onClick: () => void;
  side: "newer" | "older";
}

function PageArrow(props: PageArrowProps) {
  return (
    <span className="flex w-10 shrink-0 items-center justify-center border-hd2-border min-[35rem]:w-14">
      <IconButton aria-label={props.label} disabled={props.disabled} onClick={props.onClick}>
        {props.side === "newer" ? <ArrowBackIosNewIcon /> : <ArrowForwardIosIcon />}
      </IconButton>
    </span>
  );
}

function PageDots({ controller }: { controller: UpdateInfoController }) {
  const { t } = useI18n();
  return (
    <div className="flex items-center justify-center gap-2">
      {controller.releases.map((release, index) => (
        <button
          aria-label={t("updateInfo.goToVersion", { version: release.version })}
          aria-current={index === controller.pageIndex ? "page" : undefined}
          className={`h-2.5 w-2.5 border border-hd2-yellow transition-colors ${index === controller.pageIndex ? "bg-hd2-yellow" : "bg-transparent"}`}
          key={release.id}
          onClick={() => controller.goToPage(index)}
          type="button"
        />
      ))}
    </div>
  );
}

function useUpdateInfoKeyboard(controller: UpdateInfoController): void {
  useEffect(() => {
    if (!controller.isOpen) return;
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "ArrowLeft") controller.goToPage(controller.pageIndex - 1);
      if (event.key === "ArrowRight") controller.goToPage(controller.pageIndex + 1);
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [controller.goToPage, controller.isOpen, controller.pageIndex]);
}

function resolveContentUrl(sourceUrl: string, path: string | undefined): string | undefined {
  if (!path) return undefined;
  return new URL(path, sourceUrl).href;
}

function pageTransitionClass(direction: UpdateInfoController["navigationDirection"]): string {
  if (direction === "newer") return "updateInfoSlideFromLeft";
  return direction === "older" ? "updateInfoSlideFromRight" : "";
}
