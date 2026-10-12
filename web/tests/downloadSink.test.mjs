import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import ts from 'typescript';
const compile = source => {
  const { outputText } = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ES2022 } });
  return `data:text/javascript;base64,${Buffer.from(outputText).toString('base64')}`;
};
const { DownloadSink, FileSaveError } = await import(compile(await readFile(new URL('../src/utils/downloadSink.ts', import.meta.url), 'utf8')));
const { TransferRate } = await import(compile(await readFile(new URL('../src/utils/transferRate.ts', import.meta.url), 'utf8')));
const buffer = text => new TextEncoder().encode(text).buffer;

test('disk writes serialize out-of-order blocks and record success only after write', async () => {
  let active = 0;
  const disk = new Uint8Array(8);
  let closed = false;
  const sink = new DownloadSink({
    async write({ position, data }) {
      assert.equal(active++, 0);
      assert.equal(sink.has(position), false);
      await new Promise(resolve => setTimeout(resolve, 5));
      disk.set(new Uint8Array(data), position);
      active--;
    },
    async close() { closed = true; }, async truncate() {}, async abort() {},
  });
  await Promise.all([sink.save(4, buffer('EFGH')), sink.save(0, buffer('ABCD'))]);
  assert.equal(sink.bytes, 8);
  assert.equal(await sink.finish(8), null);
  assert.equal(closed, true);
  assert.equal(new TextDecoder().decode(disk), 'ABCDEFGH');
});

test('same-page retry skips saved offsets and detects missing blocks', async () => {
  let writes = 0;
  const sink = new DownloadSink({ async write() { writes++; }, async close() {}, async truncate() {}, async abort() {} });
  await sink.save(0, buffer('ABCD'));
  await sink.save(0, buffer('ABCD'));
  await assert.rejects(sink.finish(8), /不完整/);
  await sink.save(4, buffer('EFGH'));
  await sink.finish(8);
  assert.equal(writes, 2);
});

test('failed writes never mark bytes saved or allow subsequent writes', async () => {
  let writes = 0;
  let aborted = false;
  const sink = new DownloadSink({
    async write() { writes++; throw new Error('disk full'); },
    async close() {}, async truncate() {}, async abort() { aborted = true; },
  });
  await assert.rejects(sink.save(0, buffer('data')), FileSaveError);
  await assert.rejects(sink.save(4, buffer('more')), FileSaveError);
  assert.equal(sink.bytes, 0);
  assert.equal(sink.has(0), false);
  assert.equal(writes, 1);
  await sink.abort();
  assert.equal(aborted, true);
});

test('compatible mode assembles a correct ordered Blob', async () => {
  const sink = new DownloadSink();
  await sink.save(4, buffer('EFGH'));
  await sink.save(0, buffer('ABCD'));
  assert.equal(await (await sink.finish(8)).text(), 'ABCDEFGH');
});

test('transfer rate decays to zero after a stall and resets on fallback', () => {
  const meter = new TransferRate();
  meter.reset(0, 0);
  assert.equal(meter.sample(1024, 1000), 1024);
  meter.sample(1024, 2000);
  meter.sample(1024, 3000);
  assert.equal(meter.sample(1024, 4000), 0);
  assert.equal(meter.sample(0, 4500), 0);
  assert.equal(meter.sample(512, 5500), 512);
});
