export interface PatchFileSelection {
  files: File[];
  originalName?: string;
}

/** Preserve a dropped Mod directory's name while flattening its files for Patch loading. */
export async function patchSelectionFromDrop(
  dataTransfer: DataTransfer,
): Promise<PatchFileSelection> {
  // Drag data is only readable while the drop event is active. Snapshot files and
  // start every handle request before the first await yields back to the browser.
  const files = Array.from(dataTransfer.files);
  const handleRequests = droppedHandleRequests(dataTransfer.items);
  const directory = await firstDroppedDirectory(handleRequests);
  if (directory) {
    return { files: await filesInDirectory(directory), originalName: directory.name };
  }
  return { files };
}

function droppedHandleRequests(
  items: DataTransferItemList,
): Promise<FileSystemHandle | null | undefined>[] {
  return Array.from(items, (item) => Promise.resolve(item.getAsFileSystemHandle?.()));
}

async function firstDroppedDirectory(
  requests: Promise<FileSystemHandle | null | undefined>[],
): Promise<FileSystemDirectoryHandle | null> {
  for (const request of requests) {
    const handle = await request;
    if (handle?.kind === "directory") return handle as FileSystemDirectoryHandle;
  }
  return null;
}

async function filesInDirectory(directory: FileSystemDirectoryHandle): Promise<File[]> {
  const files: File[] = [];
  for await (const [, handle] of directory.entries()) {
    files.push(...await filesFromHandle(handle));
  }
  return files;
}

async function filesFromHandle(handle: FileSystemHandle): Promise<File[]> {
  if (handle.kind === "file") {
    return [(await (handle as FileSystemFileHandle).getFile())];
  }
  return filesInDirectory(handle as FileSystemDirectoryHandle);
}
