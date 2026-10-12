import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import ts from 'typescript';

const source = await readFile(new URL('../src/utils/http.ts', import.meta.url), 'utf8');
const { outputText } = ts.transpileModule(source, {
  compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ES2022 },
});
const { fetchWithTimeout } = await import(`data:text/javascript;base64,${Buffer.from(outputText).toString('base64')}`);

test('timeout covers a stalled body after headers arrived', { timeout: 1000 }, async () => {
  const original = globalThis.fetch;
  let receivedSignal;
  globalThis.fetch = async (_, { signal, cache }) => {
    receivedSignal = signal;
    assert.equal(cache, 'no-store');
    return new Response(new ReadableStream({ start(controller) {
      signal.addEventListener('abort', () => controller.error(new DOMException('timeout', 'AbortError')));
    } }));
  };
  try {
    await assert.rejects(fetchWithTimeout('/chunk', {}, 20, response => response.arrayBuffer()), { name: 'AbortError' });
    assert.equal(receivedSignal.aborted, true);
  } finally { globalThis.fetch = original; }
});

test('a completed body clears the timeout', { timeout: 1000 }, async () => {
  const original = globalThis.fetch;
  let receivedSignal;
  globalThis.fetch = async (_, { signal }) => {
    receivedSignal = signal;
    return new Response('data');
  };
  try {
    assert.equal(await fetchWithTimeout('/chunk', {}, 20, response => response.text()), 'data');
    await new Promise(resolve => setTimeout(resolve, 40));
    assert.equal(receivedSignal.aborted, false);
  } finally { globalThis.fetch = original; }
});
