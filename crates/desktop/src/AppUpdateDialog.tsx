import SystemUpdateAltIcon from "@mui/icons-material/SystemUpdateAlt";
import { Button, CircularProgress, IconButton, Tooltip } from "@mui/material";
import {
  Hd2Dialog,
  Hd2DialogActions,
  Hd2DialogContent,
  Hd2DialogTitle,
  useI18n,
} from "@hd2-mod-tools/migrator-ui";
import type { AppUpdateController } from "./useAppUpdate";

export function AppUpdateButton({ controller }: { controller: AppUpdateController }) {
  const { t } = useI18n();
  const label = controller.available ? t("appUpdate.open") : t("appUpdate.check");
  return (
    <Tooltip title={controller.checking ? t("appUpdate.checking") : label}>
      <span>
        <IconButton
          aria-label={label}
          className="headerIconBtn"
          disabled={controller.checking}
          onClick={controller.open}
          size="small"
        >
          {controller.checking
            ? <CircularProgress size="1rem" />
            : <SystemUpdateAltIcon fontSize="small" />}
        </IconButton>
      </span>
    </Tooltip>
  );
}

export function AppUpdateDialog(props: {
  controller: AppUpdateController;
  taskBusy: boolean;
}) {
  const { t } = useI18n();
  const { available, error, installing } = props.controller;
  if (!available) return null;
  const canClose = !installing;
  return (
    <Hd2Dialog
      ariaLabel={t("appUpdate.title")}
      onClose={canClose ? props.controller.close : undefined}
      open={props.controller.isOpen}
    >
      <Hd2DialogTitle>{t("appUpdate.title")}</Hd2DialogTitle>
      <Hd2DialogContent>
        <p className="m-0 text-sm text-hd2-text">
          {t("appUpdate.available", {
            current: available.currentVersion,
            latest: available.version,
          })}
        </p>
        <p className="mt-3 text-xs text-hd2-muted">
          {t("appUpdate.target", { target: available.target })}
        </p>
        {available.notes && <p className="mt-4 max-h-36 overflow-y-auto whitespace-pre-wrap text-xs text-hd2-muted">{available.notes}</p>}
        {props.taskBusy && <p className="mt-4 text-xs text-hd2-yellow">{t("appUpdate.taskBusy")}</p>}
        {error && <p className="mt-4 break-words text-xs text-red-300">{t("appUpdate.failed", { error })}</p>}
      </Hd2DialogContent>
      <Hd2DialogActions>
        <Button disabled={!canClose} onClick={props.controller.close}>{t("appUpdate.later")}</Button>
        <Button
          disabled={props.taskBusy || installing}
          onClick={() => void props.controller.install()}
          startIcon={installing ? <CircularProgress size="1rem" /> : <SystemUpdateAltIcon />}
          variant="contained"
        >
          {installing ? t("appUpdate.installing") : t("appUpdate.install")}
        </Button>
      </Hd2DialogActions>
    </Hd2Dialog>
  );
}
