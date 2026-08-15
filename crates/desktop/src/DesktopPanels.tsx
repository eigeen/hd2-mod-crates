import ArchiveIcon from "@mui/icons-material/Archive";
import CheckCircleIcon from "@mui/icons-material/CheckCircle";
import FolderOffIcon from "@mui/icons-material/FolderOff";
import FolderOpenIcon from "@mui/icons-material/FolderOpen";
import UploadFileIcon from "@mui/icons-material/UploadFile";
import { Button, Tooltip } from "@mui/material";
import HelpOutlineIcon from "@mui/icons-material/HelpOutlined";
import { useI18n } from "../../web_ui/src/i18n";
import type { PatchDescriptor } from "./types";

const panelClass = "flex flex-1 flex-col p-6";

interface DesktopGameDataPanelProps {
  dataDir: string | null;
  dragging: boolean;
  onChange: () => void;
  onClear: () => void;
}

export function DesktopGameDataPanel(props: DesktopGameDataPanelProps) {
  const { t } = useI18n();
  return (
    <div
      className={`${panelClass} ${props.dragging ? "bg-hd2-yellow-bg outline-1 outline-hd2-yellow -outline-offset-1" : ""}`}
      data-drop-zone="gameData"
    >
      <SectionTitle icon={<FolderOpenIcon />} title={t("gameData.title")} />
      <p className="m-0 mt-3 text-xs text-hd2-muted">
        {t("gameData.descriptionPrefix")} <code className="bg-hd2-ink px-1 py-0.5">data</code>{" "}
        {t("gameData.descriptionSuffix")}
      </p>
      <div className="mt-3 flex flex-wrap gap-2">
        <Button onClick={props.onChange} startIcon={<FolderOpenIcon />} variant="contained">
          {props.dataDir ? t("gameData.change") : t("gameData.pick")}
        </Button>
        {props.dataDir && (
          <Button color="warning" onClick={props.onClear} startIcon={<FolderOffIcon />}>
            {t("gameData.clear")}
          </Button>
        )}
      </div>
      <p className={`m-0 mt-3 border border-dashed px-3 py-2 text-xs ${props.dragging ? "border-hd2-yellow text-hd2-yellow" : "border-hd2-line text-hd2-muted"}`}>
        {props.dragging ? t("gameData.dropActive") : t("gameData.dropHint")}
      </p>
      <p className="m-0 mt-auto flex items-center gap-2 pt-3 font-mono text-xs text-hd2-muted [overflow-wrap:anywhere]">
        {props.dataDir && <CheckCircleIcon sx={{ color: "rgb(22 163 74)", fontSize: "1rem" }} />}
        <span>{props.dataDir ?? t("app.blockerGameData")}</span>
      </p>
    </div>
  );
}

interface DesktopPatchPanelProps {
  dragging: boolean;
  onChoose: () => void;
  patch: PatchDescriptor | null;
}

export function DesktopPatchPanel({ dragging, onChoose, patch }: DesktopPatchPanelProps) {
  const { t } = useI18n();
  return (
    <div
      className={`${panelClass} ${dragging ? "bg-hd2-yellow-bg outline-1 outline-hd2-yellow -outline-offset-1" : ""}`}
      data-drop-zone="patch"
    >
      <SectionTitle icon={<ArchiveIcon />} title={t("patch.title")} />
      <div className="mt-3 flex items-center gap-2.5">
        <Button onClick={onChoose} startIcon={<UploadFileIcon />} variant="contained">
          {t("patch.pick")}
        </Button>
        <Tooltip arrow title={t("patch.help")} placement="top">
          <HelpOutlineIcon className="cursor-help text-hd2-faint hover:text-hd2-yellow" sx={{ fontSize: "1rem" }} />
        </Tooltip>
      </div>
      <p className={`m-0 mt-3 border border-dashed px-3 py-2 text-xs ${dragging ? "border-hd2-yellow text-hd2-yellow" : "border-hd2-line text-hd2-muted"}`}>
        {dragging ? t("patch.dropActive") : t("patch.dropHint")}
      </p>
      <p className="m-0 mt-auto flex items-center gap-[0.4375rem] pt-3 text-xs text-hd2-muted [overflow-wrap:anywhere]">
        <span className="inline-flex h-[0.3125rem] w-[0.3125rem] bg-hd2-faint" />
        {patch?.name ?? t("patch.empty")}
      </p>
    </div>
  );
}

function SectionTitle({ icon, title }: { icon: React.ReactNode; title: string }) {
  return (
    <div className="flex items-center gap-2 [&_svg]:text-hd2-yellow [&_svg]:text-[1.125rem]">
      {icon}
      <h2 className="m-0 text-sm font-bold text-hd2-text">{title}</h2>
    </div>
  );
}
