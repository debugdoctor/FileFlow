import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import ts from 'typescript';

const source = await readFile(new URL('../src/utils/asyncPool.ts', import.meta.url), 'utf8');
const { outputText } = ts.transpileModule(source, {
  compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ES2022 },
});
const { processUploadWithConcurrencyLimit, processDownloadWithConcurrencyLimit } =
  await import(`data:text/javascript;base64,${Buffer.from(outputText).toString('base64')}`);
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));

test('empty queues resolve and invalid limits reject', { timeout: 1000 }, async () => {
  await processUploadWithConcurrencyLimit([], 4);
  await assert.rejects(processUploadWithConcurrencyLimit([], 0), /positive integer/);
});

test('a failed request rejects after draining active workers', { timeout: 1000 }, async () => {
  let drained = false;
  let extraStarted = false;
  const error = new Error('network failure');
  await assert.rejects(processUploadWithConcurrencyLimit([
    async () => { throw error; },
    async () => { await delay(20); drained = true; return 1; },
    async () => { extraStarted = true; return 1; },
  ], 2), err => err === error);
  assert.equal(drained, true);
  assert.equal(extraStarted, false);
});

test('response-body processing obeys the concurrency limit', { timeout: 1000 }, async () => {
  let active = 0;
  let peak = 0;
  let processed = 0;
  await processDownloadWithConcurrencyLimit(Array.from({ length: 8 }, () => async () => {
    active++;
    peak = Math.max(peak, active);
    return [null, new Response('chunk')];
  }), 2, async (_, response) => {
    await delay(5);
    assert.equal(await response.text(), 'chunk');
    active--;
    processed++;
  });
  assert.equal(peak, 2);
  assert.equal(processed, 8);
});

test('response-body failure propagates to the caller', { timeout: 1000 }, async () => {
  await assert.rejects(processDownloadWithConcurrencyLimit([
    async () => [null, new Response('chunk')],
  ], 1, async () => { throw new Error('truncated response'); }), /truncated response/);
});
