interface DataTransferItem {
  getAsFileSystemHandle?(): Promise<FileSystemHandle | null>;
}
