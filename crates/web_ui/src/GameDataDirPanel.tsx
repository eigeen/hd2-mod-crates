import CheckCircleIcon from "@mui/icons-material/CheckCircle";
import FolderOpenIcon from "@mui/icons-material/FolderOpen";
import FolderOffIcon from "@mui/icons-material/FolderOff";
import { Alert, Button } from "@mui/material";
import { useCallback, useEffect, useState } from "react";
import { useI18n } from "./i18n";
import {
  ensureReadPermission,
  forgetRememberedDirectory,
  type GameDirStatus,
  inspectGameDirectory,
  isDirectoryAccessSupported,
  loadRememberedDirectory,
  pickGameDirectory,
  queryReadPermission,
} from "./directoryAccess";

const panelClass = "p-6";

export interface GameDirSelection {
  handle: FileSystemDirectoryHandle;
  status: GameDirStatus;
}

interface GameDataDirPanelProps {
  selection: GameDirSelection | null;
  onChange: (selection: GameDirSelection | null) => void;
}

type PanelState =
  | { kind: "unsupported" }
  | { kind: "empty" }
  | { kind: "ready"; selection: GameDirSelection };

export function GameDataDirPanel({ selection, onChange }: GameDataDirPanelProps) {
  const { t } = useI18n();
  const [state, setState] = useState<PanelState>(() =>
    isDirectoryAccessSupported() ? { kind: "empty" } : { kind: "unsupported" },
  );
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  // 当外部 selection 改变（例如 App 主动清除），同步本地状态。
  useEffect(() => {
    if (!isDirectoryAccessSupported()) {
      return;
    }
    if (selection) {
      setState({ kind: "ready", selection });
    }
  }, [selection]);

  // 启动时尝试恢复 IDB 中的句柄。
  useEffect(() => {
    if (!isDirectoryAccessSupported() || selection) {
      return;
    }
    void restoreRemembered(setState, onChange);
  }, [selection, onChange]);

  const handlePick = useCallback(async () => {
    setError("");
    setBusy(true);
    try {
      const handle = await pickGameDirectory();
      const ready = await activate(handle, t("gameData.permissionDenied"));
      setState({ kind: "ready", selection: ready });
      onChange(ready);
    } catch (e) {
      if (!isCancelError(e)) {
        setError(messageOf(e));
      }
    } finally {
      setBusy(false);
    }
  }, [onChange, t]);

  const handleForget = useCallback(async () => {
    setError("");
    await forgetRememberedDirectory();
    setState({ kind: "empty" });
    onChange(null);
  }, [onChange]);

  if (state.kind === "unsupported") {
    return (
      <div className={panelClass}>
        <Header />
        <Alert severity="info" sx={{ mt: 2 }}>
          {t("gameData.unsupported")}
        </Alert>
      </div>
    );
  }

  return (
    <div className={panelClass}>
      <Header />
      <Body
        busy={busy}
        onForget={handleForget}
        onPick={handlePick}
        state={state}
      />
      {error && (
        <Alert severity="error" sx={{ mt: 2 }}>
          {error}
        </Alert>
      )}
    </div>
  );
}

function Header() {
  const { t } = useI18n();

  return (
    <div className="flex items-center gap-2 [&_svg]:text-hd2-yellow [&_svg]:text-[1.125rem]">
      <FolderOpenIcon />
      <h2 className="m-0 text-sm font-bold text-hd2-text">{t("gameData.title")}</h2>
    </div>
  );
}

type ActiveState = Exclude<PanelState, { kind: "unsupported" }>;

interface BodyProps {
  busy: boolean;
  onForget: () => void;
  onPick: () => void;
  state: ActiveState;
}

function Body({ busy, onForget, onPick, state }: BodyProps) {
  const { t } = useI18n();

  if (state.kind === "empty") {
    return (
      <div className="mt-3 flex flex-col gap-2 text-xs text-hd2-muted">
        <p className="m-0">
          {t("gameData.descriptionPrefix")}{" "}
          <code className="bg-hd2-ink px-1 py-0.5">data</code>{" "}
          {t("gameData.descriptionSuffix")}
        </p>
        <div>
          <Button
            disabled={busy}
            onClick={onPick}
            startIcon={<FolderOpenIcon />}
          variant="contained"
        >
            {t("gameData.pick")}
          </Button>
        </div>
      </div>
    );
  }
  return (
    <div className="mt-3 flex flex-col gap-2 text-xs text-hd2-muted">
      <div className="flex items-center gap-2">
        <CheckCircleIcon sx={{ fontSize: "1rem", color: "rgb(22 163 74)" }} />
        <span className="font-mono text-hd2-text">{state.selection.handle.name}</span>
      </div>
      <div className="flex flex-wrap gap-2">
        <Button disabled={busy} onClick={onPick} size="small" variant="outlined">
          {t("gameData.change")}
        </Button>
        <Button color="warning" disabled={busy} onClick={onForget} size="small" startIcon={<FolderOffIcon />}>
          {t("gameData.clear")}
        </Button>
      </div>
    </div>
  );
}


async function activate(
  handle: FileSystemDirectoryHandle,
  permissionDeniedMessage: string,
): Promise<GameDirSelection> {
  const granted = await ensureReadPermission(handle);
  if (!granted) {
    throw new Error(permissionDeniedMessage);
  }
  const status = await inspectGameDirectory(handle);
  return { handle, status };
}

async function restoreRemembered(
  setState: (s: PanelState) => void,
  onChange: (selection: GameDirSelection | null) => void,
) {
  const handle = await loadRememberedDirectory();
  if (!handle) {
    return;
  }
  const permission = await queryReadPermission(handle);
  if (permission !== "granted") {
    setState({ kind: "empty" });
    return;
  }
  try {
    const status = await inspectGameDirectory(handle);
    const ready: GameDirSelection = { handle, status };
    setState({ kind: "ready", selection: ready });
    onChange(ready);
  } catch {
    setState({ kind: "empty" });
  }
}

function isCancelError(error: unknown): boolean {
  return error instanceof DOMException && error.name === "AbortError";
}

function messageOf(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}
