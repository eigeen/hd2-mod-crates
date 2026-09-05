/// <reference types="vite/client" />

declare const __GIT_HASH__: string;
declare const __APP_TARGET__: "web" | "desktop";

declare module "@xyflow/react/dist/style.css";

interface DataTransferItem {
  getAsFileSystemHandle?(): Promise<FileSystemHandle | null>;
}
