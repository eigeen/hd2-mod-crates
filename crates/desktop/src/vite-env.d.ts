/// <reference types="vite/client" />

declare const __GIT_HASH__: string;

interface DataTransferItem {
  getAsFileSystemHandle?(): Promise<FileSystemHandle | null>;
}
