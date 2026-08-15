import HistoryIcon from "@mui/icons-material/History";
import { Badge, IconButton, Menu, MenuItem, Tooltip } from "@mui/material";
import { useState } from "react";
import { useI18n } from "./i18n";
import type { CompletedTaskReport } from "./taskReportHistory";

interface TaskReportHistoryButtonProps {
  onSelect: (id: string) => void;
  reports: CompletedTaskReport[];
}

export function TaskReportHistoryButton(props: TaskReportHistoryButtonProps) {
  const { language, t } = useI18n();
  const [anchor, setAnchor] = useState<HTMLElement | null>(null);
  return (
    <>
      <Tooltip title={t("result.history")}>
        <span>
          <IconButton
            className="headerIconBtn"
            disabled={!props.reports.length}
            onClick={(event) => setAnchor(event.currentTarget)}
            size="small"
          >
            <Badge badgeContent={props.reports.length} color="primary" max={9}>
              <HistoryIcon fontSize="small" />
            </Badge>
          </IconButton>
        </span>
      </Tooltip>
      <Menu anchorEl={anchor} onClose={() => setAnchor(null)} open={Boolean(anchor)}>
        {props.reports.map((report) => (
          <MenuItem
            key={report.id}
            onClick={() => {
              props.onSelect(report.id);
              setAnchor(null);
            }}
          >
            <div className="min-w-0">
              <div className="max-w-[18rem] truncate text-xs text-hd2-text">{fileName(report.output)}</div>
              <div className="text-[0.6875rem] text-hd2-muted">
                {report.kind === "migration" ? t("result.migrationTitle") : t("result.repatchTitle")}
                {" · "}{new Intl.DateTimeFormat(language, { hour: "2-digit", minute: "2-digit" }).format(report.completedAt)}
              </div>
            </div>
          </MenuItem>
        ))}
      </Menu>
    </>
  );
}

function fileName(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
}
