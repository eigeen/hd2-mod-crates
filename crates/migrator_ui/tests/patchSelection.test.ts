import { expect, test } from "bun:test";
import { patchSelectionFromDrop } from "../src/patchSelection";

test("keeps dropped files after the browser protects drag data", async () => {
  const toc = new File([new Uint8Array([1])], "9ba626afa44a3aa3.patch_0");
  let dragDataReadable = true;
  const selectionPromise = patchSelectionFromDrop(dataTransfer(
    () => dragDataReadable ? [toc] : [],
    [transferItem(null)],
  ));

  dragDataReadable = false;
  const selection = await selectionPromise;

  expect(selection.files).toEqual([toc]);
});

test("starts every dropped handle request before yielding", async () => {
  let dragDataReadable = true;
  const directory = { kind: "directory", name: "DP-8", entries: emptyEntries } as FileSystemDirectoryHandle;
  const selectionPromise = patchSelectionFromDrop(dataTransfer(
    () => [],
    [transferItem(null), transferItem(directory, () => dragDataReadable)],
  ));

  dragDataReadable = false;
  const selection = await selectionPromise;

  expect(selection.originalName).toBe("DP-8");
});

function dataTransfer(
  files: () => File[],
  items: DataTransferItem[],
): DataTransfer {
  return {
    get files() {
      return files() as unknown as FileList;
    },
    items: items as unknown as DataTransferItemList,
  } as DataTransfer;
}

function transferItem(
  handle: FileSystemHandle | null,
  available: () => boolean = () => true,
): DataTransferItem {
  return {
    getAsFileSystemHandle: async () => available() ? handle : null,
  } as DataTransferItem;
}

async function* emptyEntries(): AsyncIterableIterator<[string, FileSystemHandle]> {
  return;
}
