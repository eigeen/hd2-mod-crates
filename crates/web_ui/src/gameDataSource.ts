// 实现 wasm `migrate_cross_archive` 所需的 DataSource 接口，基于 FileSystemDirectoryHandle。
// 方法对应 Rust 端 JsDataSource 中通过 Reflect::get 提取的四个 JS 函数。

const BUNDLE_CHUNK_PATTERN = /^bundles\.\d\d\.nxa$/;

export class GameDataSource {
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
    const result: string[] = [];
    for await (const [name, handle] of this.dir.entries()) {
      if (handle.kind === "file" && BUNDLE_CHUNK_PATTERN.test(name)) {
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

function isNotFound(error: unknown): boolean {
  return error instanceof DOMException && error.name === "NotFoundError";
}
