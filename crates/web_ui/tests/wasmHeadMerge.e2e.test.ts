import { expect, test } from "bun:test";
import { readFile } from "node:fs/promises";
import path from "node:path";
import {
  initSync,
  merge_head_into_armor_stream,
} from "../src/wasm/hd2_migrator_wasm/hd2_migrator_wasm.js";
import { StoreZipBuilder } from "../src/fileInputs";

const fixturePath = process.env.HD2_HEAD_MERGE_E2E_PATCH;

(fixturePath ? test : test.skip)(
  "streams a real large head merge through bounded WASM chunks",
  async () => {
    const runtime = await initializeWasm();
    const patch = await loadPatch(fixturePath!);
    const inputBytes = patch.toc.byteLength + patch.gpu.byteLength + patch.stream.byteLength;
    const first = runHeadMerge(patch, path.basename(fixturePath!), true);
    assertSuccessfulRun(first);
    expect(first.zipBytes).toBeGreaterThan(first.outputBytes);
    const firstWasmBytes = runtime.memory.buffer.byteLength;
    assertSuccessfulRun(runHeadMerge(patch, path.basename(fixturePath!)));
    assertSuccessfulRun(runHeadMerge(patch, path.basename(fixturePath!)));
    const finalWasmBytes = runtime.memory.buffer.byteLength;

    expect(firstWasmBytes).toBeLessThan(inputBytes * 2);
    expect(finalWasmBytes).toBeLessThanOrEqual(firstWasmBytes + 8 * 1024 * 1024);
    console.info({
      finalWasmBytes,
      inputBytes,
      maxChunkBytes: first.maxChunkBytes,
      outputBytes: first.outputBytes,
      firstWasmBytes,
    });
  },
  300_000,
);

interface StreamedFile {
  expectedBytes: number;
  name: string;
  receivedBytes: number;
}

interface MergeRun {
  files: StreamedFile[];
  maxChunkBytes: number;
  outputBytes: number;
  summary: { mergedBodyUnits: number; mergedLods: number };
  zipBytes: number;
}

class StreamCollector {
  private active: StreamedFile | null = null;
  private readonly zip: StoreZipBuilder | null;
  readonly files: StreamedFile[] = [];
  maxChunkBytes = 0;

  constructor(buildZip = false) {
    this.zip = buildZip ? new StoreZipBuilder() : null;
  }

  readonly callbacks = {
    onFileStart: (name: string, expectedBytes: number, crc32: number) =>
      this.start(name, expectedBytes, crc32),
    onFileChunk: (bytes: Uint8Array) => this.add(bytes),
    onFileEnd: () => this.end(),
  };

  finish(summary: MergeRun["summary"]): MergeRun {
    expect(this.active).toBeNull();
    return {
      files: this.files,
      maxChunkBytes: this.maxChunkBytes,
      outputBytes: this.files.reduce((total, file) => total + file.receivedBytes, 0),
      summary,
      zipBytes: this.zip?.finish().size ?? 0,
    };
  }

  private start(name: string, expectedBytes: number, crc32: number) {
    expect(this.active).toBeNull();
    this.active = { name, expectedBytes, receivedBytes: 0 };
    this.zip?.beginFile(name, expectedBytes, crc32);
  }

  private add(bytes: Uint8Array) {
    expect(this.active).not.toBeNull();
    this.maxChunkBytes = Math.max(this.maxChunkBytes, bytes.byteLength);
    this.active!.receivedBytes += bytes.byteLength;
    this.zip?.addFileChunk(bytes);
  }

  private end() {
    expect(this.active).not.toBeNull();
    expect(this.active!.receivedBytes).toBe(this.active!.expectedBytes);
    this.files.push(this.active!);
    this.zip?.endFile();
    this.active = null;
  }
}

function runHeadMerge(
  patch: Awaited<ReturnType<typeof loadPatch>>,
  name: string,
  buildZip = false,
): MergeRun {
  const collector = new StreamCollector(buildZip);
  const result = merge_head_into_armor_stream(
    { name, ...patch },
    null,
    { armorHash: "57cebd7e5f985d45", helmetHash: "e9f6ef51d693fdd5" },
    collector.callbacks,
  ) as { summary: MergeRun["summary"] };
  return collector.finish(result.summary);
}

function assertSuccessfulRun(run: MergeRun) {
  expect(run.files).toHaveLength(3);
  expect(run.summary.mergedBodyUnits).toBe(2);
  expect(run.summary.mergedLods).toBe(8);
  expect(run.maxChunkBytes).toBeLessThanOrEqual(4 * 1024 * 1024);
}

async function initializeWasm() {
  const wasmPath = path.resolve(
    import.meta.dir,
    "../src/wasm/hd2_migrator_wasm/hd2_migrator_wasm_bg.wasm",
  );
  return initSync({ module: await readFile(wasmPath) });
}

async function loadPatch(mainPath: string) {
  return {
    toc: new Uint8Array(await readFile(mainPath)),
    gpu: new Uint8Array(await readFile(`${mainPath}.gpu_resources`)),
    stream: new Uint8Array(await readFile(`${mainPath}.stream`)),
  };
}
