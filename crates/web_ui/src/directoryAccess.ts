// File System Access API 封装：游戏 data 目录的授权、IndexedDB 持久化、安装类型检测。

const DB_NAME = "hd2-migrator-prefs";
const DB_STORE = "handles";
const HANDLE_KEY = "game-data-dir";
const BUNDLE_CHUNK_PATTERN = /^bundles\.\d\d\.nxa$/;

export type InstallKind = "slim" | "legacy" | "empty";

export interface GameDirStatus {
  kind: InstallKind;
  bundleChunkCount: number;
  hasBundleToc: boolean;
}

// 仅在支持 File System Access API 的 Chromium 浏览器中可用。
export function isDirectoryAccessSupported(): boolean {
  return typeof window !== "undefined" && "showDirectoryPicker" in window;
}

// 弹出系统目录选择器，并将句柄持久化到 IndexedDB。
export async function pickGameDirectory(): Promise<FileSystemDirectoryHandle> {
  const handle = await window.showDirectoryPicker({
    id: "hd2-game-data",
    mode: "read",
  });
  await persistDirHandle(handle).catch(() => undefined);
  return handle;
}

// 加载上次记住的目录句柄；权限可能已失效，调用方需在用户手势下调用 ensureReadPermission。
export async function loadRememberedDirectory(): Promise<FileSystemDirectoryHandle | null> {
  return loadPersistedDirHandle();
}

// 查询当前句柄的读取权限状态（不弹窗）。
export async function queryReadPermission(handle: FileSystemDirectoryHandle): Promise<PermissionState> {
  return handle.queryPermission({ mode: "read" });
}

// 确保已获取读权限；必要时弹出权限请求（必须在用户手势内调用）。
export async function ensureReadPermission(handle: FileSystemDirectoryHandle): Promise<boolean> {
  const current = await handle.queryPermission({ mode: "read" });
  if (current === "granted") {
    return true;
  }
  const granted = await handle.requestPermission({ mode: "read" });
  return granted === "granted";
}

// 清除已持久化的目录句柄（例如目录失效后）。
export async function forgetRememberedDirectory(): Promise<void> {
  await deletePersistedDirHandle().catch(() => undefined);
}

// 扫描目录，判断这是 Slim install (有 bundles.nxa) 还是 Legacy。
export async function inspectGameDirectory(dir: FileSystemDirectoryHandle): Promise<GameDirStatus> {
  let hasBundleToc = false;
  let bundleChunkCount = 0;
  let hasAnyArchive = false;
  for await (const [name, handle] of dir.entries()) {
    if (handle.kind !== "file") {
      continue;
    }
    if (name === "bundles.nxa") {
      hasBundleToc = true;
      continue;
    }
    if (BUNDLE_CHUNK_PATTERN.test(name)) {
      bundleChunkCount += 1;
      continue;
    }
    // 16 字符十六进制哈希文件（无后缀或带 .gpu_resources/.stream）也可视为 legacy archive
    if (looksLikeArchiveName(name)) {
      hasAnyArchive = true;
    }
  }
  if (hasBundleToc) {
    return { kind: "slim", bundleChunkCount, hasBundleToc };
  }
  if (hasAnyArchive) {
    return { kind: "legacy", bundleChunkCount, hasBundleToc };
  }
  return { kind: "empty", bundleChunkCount, hasBundleToc };
}

function looksLikeArchiveName(name: string): boolean {
  // 16 char hex base, optionally followed by ".gpu_resources" or ".stream"
  const base = name.replace(/\.(gpu_resources|stream)$/, "");
  return /^[0-9a-f]{16}$/.test(base);
}

function openDb(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(DB_NAME, 1);
    req.onupgradeneeded = () => req.result.createObjectStore(DB_STORE);
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}

async function withStore<T>(
  mode: IDBTransactionMode,
  work: (store: IDBObjectStore) => IDBRequest<T>,
): Promise<T> {
  const db = await openDb();
  try {
    return await new Promise<T>((resolve, reject) => {
      const tx = db.transaction(DB_STORE, mode);
      const req = work(tx.objectStore(DB_STORE));
      req.onsuccess = () => resolve(req.result);
      req.onerror = () => reject(req.error);
    });
  } finally {
    db.close();
  }
}

async function persistDirHandle(handle: FileSystemDirectoryHandle): Promise<void> {
  await withStore("readwrite", (store) => store.put(handle, HANDLE_KEY));
}

async function loadPersistedDirHandle(): Promise<FileSystemDirectoryHandle | null> {
  try {
    const value = await withStore<FileSystemDirectoryHandle | undefined>("readonly", (store) =>
      store.get(HANDLE_KEY),
    );
    return value ?? null;
  } catch {
    return null;
  }
}

async function deletePersistedDirHandle(): Promise<void> {
  await withStore("readwrite", (store) => store.delete(HANDLE_KEY));
}
