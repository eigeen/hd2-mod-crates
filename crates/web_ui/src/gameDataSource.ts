// 实现 WASM 迁移与识别所需的 DataSource 接口，基于 FileSystemDirectoryHandle。
// 方法对应 Rust 端 JsDataSource 中通过 Reflect::get 提取的四个 JS 函数。

const BUNDLE_CHUNK_PATTERN = /^bundles\.\d\d\.nxa$/;
const PACKAGE_PATTERN = /^[0-9a-f]{16}$/i;

export interface GameDataSourcePort {
  readFull(path: string): Promise<Uint8Array>;
  readRange(path: string, offset: number, len: number): Promise<Uint8Array>;
  exists(path: string): Promise<boolean>;
  listBundleChunks(): Promise<string[]>;
  listPackages(): Promise<string[]>;
}

export type GameDataRpcMethod = keyof GameDataSourcePort;

export class GameDataSource implements GameDataSourcePort {
  constructor(private readonly dir: FileSystemDirectoryHandle) {}

  async readFull(path: string): Promise<Uint8Array> {
    const file = await this.openFile(path);
    return new Uint8Array(await file.arrayBuffer());
  }

  async readRange(path: string, offset: number, len: number): Promise<Uint8Array> {
    const file = await this.openFile(path);
    const end = offset + len;
    if (end > file.size) {
      throw new RangeError(`${path}: range ${offset}..${end} exceeds file size ${file.size}`);
    }
    const slice = file.slice(offset, end);
    return new Uint8Array(await slice.arrayBuffer());
  }

  async exists(path: string): Promise<boolean> {
    try {
      await this.dir.getFileHandle(path);
      return true;
    } catch (error) {
      if (isNotFound(error)) {
        return false;
      }
      throw error;
    }
  }

  async listBundleChunks(): Promise<string[]> {
    return this.listMatchingFiles(BUNDLE_CHUNK_PATTERN);
  }

  async listPackages(): Promise<string[]> {
    return this.listMatchingFiles(PACKAGE_PATTERN);
  }

  private async listMatchingFiles(pattern: RegExp): Promise<string[]> {
    const result: string[] = [];
    for await (const [name, handle] of this.dir.entries()) {
      if (handle.kind === "file" && pattern.test(name)) {
        result.push(name);
      }
    }
    result.sort();
    return result;
  }

  private async openFile(path: string): Promise<File> {
    const handle = await this.dir.getFileHandle(path);
    return handle.getFile();
  }
}

/** Execute only the file operations exposed to a task Worker. */
export function executeGameDataRequest(
  source: GameDataSourcePort,
  method: GameDataRpcMethod,
  args: unknown[],
): Promise<Uint8Array | boolean | string[]> {
  if (method === "readFull") return source.readFull(stringArg(args, 0));
  if (method === "readRange") {
    return source.readRange(stringArg(args, 0), numberArg(args, 1), numberArg(args, 2));
  }
  if (method === "exists") return source.exists(stringArg(args, 0));
  if (method === "listBundleChunks") return source.listBundleChunks();
  if (method === "listPackages") return source.listPackages();
  return Promise.reject(new Error(`Unsupported game data operation: ${String(method)}`));
}

function stringArg(args: unknown[], index: number): string {
  const value = args[index];
  if (typeof value !== "string") throw new TypeError(`Argument ${index} must be a string`);
  return value;
}

function numberArg(args: unknown[], index: number): number {
  const value = args[index];
  if (!Number.isSafeInteger(value) || (value as number) < 0) {
    throw new TypeError(`Argument ${index} must be a non-negative integer`);
  }
  return value as number;
}

function isNotFound(error: unknown): boolean {
  return error instanceof DOMException && error.name === "NotFoundError";
}
