import { expect, test } from "bun:test";
import { StoreZipBuilder } from "../src/fileInputs";

test("assembles streamed files into a ZIP with a central directory", async () => {
  const builder = new StoreZipBuilder();
  builder.add("first/file.bin", new Uint8Array([1, 2, 3]));
  builder.add("second/file.bin", new Uint8Array([4, 5]));

  const bytes = new Uint8Array(await builder.finish().arrayBuffer());
  const view = new DataView(bytes.buffer);
  const endOffset = bytes.length - 22;

  expect(view.getUint32(0, true)).toBe(0x04034b50);
  expect(view.getUint32(endOffset, true)).toBe(0x06054b50);
  expect(view.getUint16(endOffset + 10, true)).toBe(2);
  expect(view.getUint32(view.getUint32(endOffset + 16, true), true)).toBe(0x02014b50);
});

test("accepts a precomputed CRC without changing ZIP bytes", async () => {
  const bytes = new Uint8Array([1, 2, 3]);
  const computed = new StoreZipBuilder();
  const precomputed = new StoreZipBuilder();
  computed.add("file.bin", bytes);
  precomputed.add("file.bin", bytes, 0x55bc801d);

  expect(await precomputed.finish().arrayBuffer())
    .toEqual(await computed.finish().arrayBuffer());
});
