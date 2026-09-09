import { expect, test } from "bun:test";
import { GameDataSource } from "../src/gameDataSource";

test("range reads reuse a task's file snapshot without reading the full file", async () => {
  let opens = 0;
  const file = new File([new Uint8Array([1, 2, 3, 4])], "bundles.00.nxa");
  file.arrayBuffer = () => { throw new Error("Unexpected full read"); };
  const dir = {
    async getFileHandle() {
      opens += 1;
      return { getFile: async () => file };
    },
  } as unknown as FileSystemDirectoryHandle;
  const source = new GameDataSource(dir);
  const ranges = await Promise.all([source.readRange(file.name, 0, 2), source.readRange(file.name, 2, 2)]);
  expect(ranges.map((bytes) => [...bytes])).toEqual([[1, 2], [3, 4]]);
  expect(opens).toBe(1);
  await new GameDataSource(dir).readRange(file.name, 0, 1);
  expect(opens).toBe(2);
});

test("a failed file open is retried instead of poisoning later reads", async () => {
  let opens = 0;
  const dir = {
    async getFileHandle() {
      opens += 1;
      if (opens === 1) throw new Error("permission denied");
      return { getFile: async () => new File([new Uint8Array([7])], "example") };
    },
  } as unknown as FileSystemDirectoryHandle;
  const source = new GameDataSource(dir);
  await expect(source.readRange("example", 0, 1)).rejects.toThrow("permission denied");
  expect([...await source.readRange("example", 0, 1)]).toEqual([7]);
  expect(opens).toBe(2);
});
