import { expect, test } from "bun:test";
import { droppedGameDirectory, isFileSystemAbort } from "../src/directoryAccess";

test("extracts a dragged directory after ignoring non-directory items", async () => {
  const directory = handle("directory", "data") as FileSystemDirectoryHandle;
  const items = transferItems([
    transferItem("string", null),
    transferItem("file", handle("file", "bundles.nxa")),
    transferItem("file", directory),
  ]);

  expect(await droppedGameDirectory(items)).toBe(directory);
});

test("returns null when directory handle drag and drop is unavailable", async () => {
  const unsupportedItem = { kind: "file" } as DataTransferItem;

  expect(await droppedGameDirectory(transferItems([unsupportedItem]))).toBeNull();
});

test("recognizes native and message-wrapped abort errors", () => {
  const namedAbort = new Error("The operation was aborted");
  namedAbort.name = "AbortError";

  expect(isFileSystemAbort(namedAbort)).toBeTrue();
  expect(isFileSystemAbort(new Error("The user aborted a request."))).toBeTrue();
  expect(isFileSystemAbort(new Error("Read permission was denied."))).toBeFalse();
});

function handle(kind: FileSystemHandleKind, name: string): FileSystemHandle {
  return { kind, name } as FileSystemHandle;
}

function transferItem(kind: DataTransferItemKind, value: FileSystemHandle | null): DataTransferItem {
  return {
    kind,
    getAsFileSystemHandle: async () => value,
  } as DataTransferItem;
}

function transferItems(items: DataTransferItem[]): DataTransferItemList {
  return items as unknown as DataTransferItemList;
}
