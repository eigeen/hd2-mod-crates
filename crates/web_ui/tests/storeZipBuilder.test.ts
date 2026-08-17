import { expect, test } from "bun:test";
import { buildRepatchedPatchZip, StoreZipBuilder } from "../src/fileInputs";

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

test("keeps all three repatched files when sidecars are empty", async () => {
  const patchName = "example.patch_0";
  const blob = buildRepatchedPatchZip({
    name: patchName,
    toc: new Uint8Array([1]),
    gpu: new Uint8Array(),
    stream: new Uint8Array(),
  }, new Uint8Array([2]));

  const entries = localEntries(new Uint8Array(await blob.arrayBuffer()));

  expect(entries).toEqual([
    { name: patchName, size: 1 },
    { name: `${patchName}.gpu_resources`, size: 0 },
    { name: `${patchName}.stream`, size: 0 },
  ]);
});

function localEntries(bytes: Uint8Array): Array<{ name: string; size: number }> {
  const entries: Array<{ name: string; size: number }> = [];
  const decoder = new TextDecoder();
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  let offset = 0;
  while (view.getUint32(offset, true) === 0x04034b50) {
    const size = view.getUint32(offset + 18, true);
    const nameLength = view.getUint16(offset + 26, true);
    const extraLength = view.getUint16(offset + 28, true);
    const nameStart = offset + 30;
    const name = decoder.decode(bytes.subarray(nameStart, nameStart + nameLength));
    entries.push({ name, size });
    offset = nameStart + nameLength + extraLength + size;
  }
  return entries;
}
