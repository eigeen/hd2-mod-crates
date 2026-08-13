export interface PatchFileSelection {
  files: File[];
  originalName?: string;
}

/** Preserve a dropped Mod directory's name while flattening its files for Patch loading. */
export async function patchSelectionFromDrop(
  dataTransfer: DataTransfer,
): Promise<PatchFileSelection> {
  const directory = await firstDroppedDirectory(dataTransfer.items);
  if (directory) {
    return { files: await filesInDirectory(directory), originalName: directory.name };
  }
  return { files: Array.from(dataTransfer.files) };
}

async function firstDroppedDirectory(
  items: DataTransferItemList,
): Promise<FileSystemDirectoryHandle | null> {
  for (const item of Array.from(items)) {
    const handle = await item.getAsFileSystemHandle?.();
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
