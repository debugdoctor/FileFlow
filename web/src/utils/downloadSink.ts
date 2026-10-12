export interface FileWriter {
  write(data: { type: 'write'; position: number; data: ArrayBuffer }): Promise<void>;
  truncate(size: number): Promise<void>;
  close(): Promise<void>;
  abort(): Promise<void>;
}

export class FileSaveError extends Error {}

// Only offsets are retained when writing to disk; payload memory stays bounded
// by the download workers or the P2P receive window.
export class DownloadSink {
  private saved = new Map<number, number>();
  private buffers = new Map<number, ArrayBuffer>();
  private queue: Promise<void> = Promise.resolve();
  private failure: FileSaveError | null = null;
  private closed = false;
  bytes = 0;

  constructor(private writer?: FileWriter) {}
  get streaming() { return !!this.writer; }
  has(start: number) { return this.saved.has(start); }

  private async write(operation: () => Promise<void>) {
    if (this.failure) throw this.failure;
    if (this.closed) throw new FileSaveError('文件已关闭');
    const next = this.queue.then(() => {
      if (this.failure || this.closed) throw this.failure || new FileSaveError('文件已关闭');
      return operation();
    });
    this.queue = next.catch(() => {});
    try { await next; } catch (error) {
      this.failure = new FileSaveError(`文件写入失败：${error instanceof Error ? error.message : '请检查保存位置和剩余空间'}`);
      throw this.failure;
    }
  }

  async save(start: number, data: ArrayBuffer) {
    if (this.failure) throw this.failure;
    if (!Number.isSafeInteger(start) || start < 0 || data.byteLength === 0) throw new Error('无效的文件分块');
    if (this.saved.has(start)) return;
    if (this.writer) await this.write(() => this.writer!.write({ type: 'write', position: start, data }));
    else this.buffers.set(start, data);
    this.saved.set(start, data.byteLength);
    this.bytes += data.byteLength;
  }

  async reset() {
    if (this.failure) throw this.failure;
    if (this.writer) await this.write(() => this.writer!.truncate(0));
    this.saved.clear();
    this.buffers.clear();
    this.bytes = 0;
  }

  async finish(total: number): Promise<Blob | null> {
    let offset = 0;
    const entries = Array.from(this.saved.entries()).sort((a, b) => a[0] - b[0]);
    for (const [start, length] of entries) {
      if (start !== offset) throw new Error('文件分块不完整，请重试');
      offset += length;
    }
    if (offset !== total) throw new Error('文件大小不完整，请重试');
    if (this.writer) {
      await this.write(() => this.writer!.close());
      this.closed = true;
      return null;
    }
    const blob = new Blob(entries.map(([start]) => this.buffers.get(start)!), { type: 'application/octet-stream' });
    this.buffers.clear();
    this.closed = true;
    return blob;
  }

  async abort() {
    if (this.closed) return;
    this.closed = true;
    this.failure ??= new FileSaveError('文件保存已取消');
    this.buffers.clear();
    try { await this.writer?.abort(); } catch { /* An errored stream may already be closed. */ }
  }
}
