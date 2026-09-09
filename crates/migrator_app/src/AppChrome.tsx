import AccountTreeIcon from "@mui/icons-material/AccountTree";
import GitHubIcon from "@mui/icons-material/GitHub";
import { IconButton, Tab, Tabs, Tooltip } from "@mui/material";
import type { ReactNode } from "react";
import {
  AdvancedToolsMenu,
  LanguageMenu,
  TaskReportHistoryButton,
  ToolIntro,
  ToolModeNav,
  UpdateInfoButton,
  titleUrl,
  useI18n,
  type CompletedTaskReport,
} from "@hd2-mod-tools/migrator-ui";
import type { ToolMode } from "./platform";

interface AppChromeProps {
  afterHeader?: ReactNode;
  children: ReactNode;
  footer?: ReactNode;
  headerAction?: ReactNode;
  layout: "web" | "desktop";
  navigationDisabled: boolean;
  onClearReports: () => void;
  onOpenReport: (id: string) => void;
  onOpenUpdateInfo: () => void;
  onToolModeChange: (mode: ToolMode) => void;
  reports: CompletedTaskReport[];
  toolMode: ToolMode;
  updateInfoAvailable: boolean;
}

export function AppChrome(props: AppChromeProps) {
  const desktop = props.layout === "desktop";
  const mainClass = desktop
    ? "min-h-screen w-full"
    : "mx-auto w-full max-w-[56rem] px-4 py-6 min-[51.25rem]:px-6 min-[51.25rem]:py-10";
  const panelClass = desktop
    ? "min-h-screen overflow-hidden bg-black/60"
    : "overflow-hidden border-2 border-hd2-border bg-black/60";

  return (
    <div className={desktop ? "min-h-screen bg-hd2-bg" : "min-h-screen"}>
      <div className="fixed inset-0 z-0 bg-center bg-cover" style={{ backgroundImage: "var(--hd2-app-background)", filter: "brightness(0.4)" }} />
      <div className="relative z-[1] min-h-screen">
        <main className={mainClass}>
          <div className={panelClass} data-desktop-shell={desktop || undefined}>
            <Header {...props} />
            {props.afterHeader}
            <ToolNavigation {...props} />
            <ToolIntro mode={props.toolMode} />
            {props.children}
          </div>
          {props.footer}
        </main>
      </div>
    </div>
  );
}

function Header(props: AppChromeProps) {
  const { t } = useI18n();
  return (
    <div className="flex flex-col items-center border-b border-hd2-border bg-hd2-surface/70 px-4 py-5">
      <div className="flex w-full items-center gap-3">
        <Tooltip title={t("github.revision", { hash: __GIT_HASH__ })}>
          <div aria-label={t("github.revision", { hash: __GIT_HASH__ })} className="flex w-28 shrink-0 items-center gap-1 font-mono text-[0.625rem] tracking-wide text-hd2-faint min-[35rem]:w-32 min-[35rem]:text-[0.6875rem]">
            <AccountTreeIcon sx={{ fontSize: "0.875rem" }} /><span>{__GIT_HASH__}</span>
          </div>
        </Tooltip>
        <div className="flex min-w-0 flex-1 items-center justify-center gap-3">
          <img alt="" className="hidden min-[40rem]:block" draggable={false} src={titleUrl} style={{ height: "2rem", transform: "scaleX(-1)" }} />
          <h1 className="m-0 text-center text-lg font-bold text-hd2-yellow min-[35rem]:text-xl min-[51.25rem]:text-2xl">{t("app.title")}</h1>
          <img alt="" className="hidden min-[40rem]:block" draggable={false} src={titleUrl} style={{ height: "2rem" }} />
        </div>
        <div className="flex w-28 shrink-0 items-center justify-end min-[35rem]:w-32">
          <Tooltip title={t("github.openRepository")}>
            <IconButton aria-label={t("github.openRepository")} className="headerIconBtn" component="a" href="https://github.com/eigeen/hd2-mod-crates" rel="noreferrer" size="small" target="_blank">
              <GitHubIcon fontSize="small" />
            </IconButton>
          </Tooltip>
          {props.headerAction}
          <UpdateInfoButton available={props.updateInfoAvailable} openLatest={props.onOpenUpdateInfo} />
          <TaskReportHistoryButton onClear={props.onClearReports} onSelect={props.onOpenReport} reports={props.reports} />
          <LanguageMenu />
        </div>
      </div>
    </div>
  );
}

function ToolNavigation(props: AppChromeProps) {
  const { t } = useI18n();
  return (
    <ToolModeNav>
      <Tabs value={props.toolMode === "migrate" || props.toolMode === "repatch" ? props.toolMode : false} onChange={(_, mode: ToolMode) => {
        if (!props.navigationDisabled) props.onToolModeChange(mode);
      }}>
        <Tab data-tool-nav-active={props.toolMode === "migrate"} disabled={props.navigationDisabled} label={t("mode.migrate")} value="migrate" />
        <Tab data-tool-nav-active={props.toolMode === "repatch"} disabled={props.navigationDisabled} label={t("mode.repatch")} value="repatch" />
      </Tabs>
      <AdvancedToolsMenu
        active={props.toolMode}
        disabled={props.navigationDisabled}
        items={[{ mode: "merge", label: t("mode.merge") }, { mode: "head-merge", label: t("mode.headMerge") }]}
        onSelect={(mode) => props.onToolModeChange(mode as ToolMode)}
      />
    </ToolModeNav>
  );
}
