import CheckCircleIcon from "@mui/icons-material/CheckCircle";
import FolderOpenIcon from "@mui/icons-material/FolderOpen";
import FolderOffIcon from "@mui/icons-material/FolderOff";
import { Alert, Button, Chip } from "@mui/material";
import { useCallback, useEffect, useState } from "react";
import {
  ensureReadPermission,
  forgetRememberedDirectory,
  type GameDirStatus,
  type InstallKind,
  inspectGameDirectory,
  isDirectoryAccessSupported,
  loadRememberedDirectory,
  pickGameDirectory,
  queryReadPermission,
} from "./directoryAccess";

const panelClass =
  "border border-hd2-border bg-hd2-surface overflow-hidden p-6";

export interface GameDirSelection {
  handle: FileSystemDirectoryHandle;
  status: GameDirStatus;
}

interface GameDataDirPanelProps {
  selection: GameDirSelection | null;
  onChange: (selection: GameDirSelection | null) => void;
}

// 状态机：未选择 / 已记住但未授权 / 已就绪 / 不支持。
type PanelState =
  | { kind: "unsupported" }
  | { kind: "empty" }
  | { kind: "needs-permission"; handle: FileSystemDirectoryHandle }
  | { kind: "ready"; selection: GameDirSelection };

export function GameDataDirPanel({ selection, onChange }: GameDataDirPanelProps) {
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
      const ready = await activate(handle);
      setState({ kind: "ready", selection: ready });
      onChange(ready);
    } catch (e) {
      if (!isCancelError(e)) {
        setError(messageOf(e));
      }
    } finally {
      setBusy(false);
    }
  }, [onChange]);

  const handleAuthorize = useCallback(
    async (handle: FileSystemDirectoryHandle) => {
      setError("");
      setBusy(true);
      try {
        const granted = await ensureReadPermission(handle);
        if (!granted) {
          setError("读取权限被拒绝。");
          return;
        }
        const ready = await activate(handle);
        setState({ kind: "ready", selection: ready });
        onChange(ready);
      } catch (e) {
        setError(messageOf(e));
      } finally {
        setBusy(false);
      }
    },
    [onChange],
  );

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
          当前浏览器不支持目录访问 API（仅 Chrome / Edge / 其他 Chromium 浏览器可用）。可继续使用同源版本输出。
        </Alert>
      </div>
    );
  }

  return (
    <div className={panelClass}>
      <Header />
      <Body
        busy={busy}
        onAuthorize={handleAuthorize}
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
  return (
    <div className="flex items-center gap-2 [&_svg]:text-hd2-yellow [&_svg]:text-[1.125rem]">
      <FolderOpenIcon />
      <h2 className="m-0 text-sm font-bold text-hd2-text">游戏 data 目录</h2>
      <span className="ml-2 border border-hd2-yellow/30 bg-hd2-yellow/10 px-2 py-0.5 text-[0.6875rem] font-bold text-hd2-yellow">
        跨版本迁移
      </span>
    </div>
  );
}

type ActiveState = Exclude<PanelState, { kind: "unsupported" }>;

interface BodyProps {
  busy: boolean;
  onAuthorize: (handle: FileSystemDirectoryHandle) => void;
  onForget: () => void;
  onPick: () => void;
  state: ActiveState;
}

function Body({ busy, onAuthorize, onForget, onPick, state }: BodyProps) {
  if (state.kind === "empty") {
    return (
      <div className="mt-3 flex flex-col gap-2 text-xs text-hd2-muted">
        <p className="m-0">选择 Helldivers 2 的 <code className="bg-hd2-ink px-1 py-0.5">data</code> 目录以启用跨版本迁移。仅读取，不修改任何文件。</p>
        <div>
          <Button
            disabled={busy}
            onClick={onPick}
            startIcon={<FolderOpenIcon />}
            variant="contained"
          >
            选择目录
          </Button>
        </div>
      </div>
    );
  }
  if (state.kind === "needs-permission") {
    return (
      <div className="mt-3 flex flex-col gap-2 text-xs text-hd2-muted">
        <p className="m-0">
          上次使用的目录：<span className="font-mono text-hd2-text">{state.handle.name}</span>
        </p>
        <p className="m-0">需要重新授权读取权限。</p>
        <div className="flex flex-wrap gap-2">
          <Button disabled={busy} onClick={() => onAuthorize(state.handle)} variant="contained">
            授权访问
          </Button>
          <Button disabled={busy} onClick={onPick}>
            选择其他目录
          </Button>
          <Button color="warning" disabled={busy} onClick={onForget} startIcon={<FolderOffIcon />}>
            清除
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
        <InstallKindChip kind={state.selection.status.kind} chunkCount={state.selection.status.bundleChunkCount} />
      </div>
      <div className="flex flex-wrap gap-2">
        <Button disabled={busy} onClick={onPick} size="small" variant="outlined">
          更换
        </Button>
        <Button color="warning" disabled={busy} onClick={onForget} size="small" startIcon={<FolderOffIcon />}>
          清除
        </Button>
      </div>
    </div>
  );
}

function InstallKindChip({ kind, chunkCount }: { kind: InstallKind; chunkCount: number }) {
  if (kind === "slim") {
    return <Chip color="success" label={`Slim install · ${chunkCount} 个 bundle`} size="small" />;
  }
  if (kind === "legacy") {
    return <Chip color="info" label="Legacy install" size="small" />;
  }
  return <Chip color="warning" label="未检测到游戏文件" size="small" />;
}

async function activate(handle: FileSystemDirectoryHandle): Promise<GameDirSelection> {
  const granted = await ensureReadPermission(handle);
  if (!granted) {
    throw new Error("读取权限被拒绝。");
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
  if (permission === "granted") {
    try {
      const status = await inspectGameDirectory(handle);
      const ready: GameDirSelection = { handle, status };
      setState({ kind: "ready", selection: ready });
      onChange(ready);
      return;
    } catch {
      setState({ kind: "empty" });
      return;
    }
  }
  setState({ kind: "needs-permission", handle });
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
