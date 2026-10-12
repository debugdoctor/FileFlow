import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import ts from 'typescript';
const compile = source => {
  const { outputText } = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ES2022 } });
  return `data:text/javascript;base64,${Buffer.from(outputText).toString('base64')}`;
};
const http = compile(await readFile(new URL('../src/utils/http.ts', import.meta.url), 'utf8'));
const { watchTransfer, cancelTransfer, createSenderToken } = await import(compile(
  (await readFile(new URL('../src/utils/transferControl.ts', import.meta.url), 'utf8')).replace("'./http'", JSON.stringify(http)),
));
const json = data => new Response(JSON.stringify(data));

test('peer cancellation notifies exactly once and stops polling', { timeout: 2000 }, async () => {
  const original = globalThis.fetch;
  let calls = 0;
  let notifications = 0;
  globalThis.fetch = async () => { calls++; return json({ data: { cancelled_by: 'receiver', done: false } }); };
  let stop;
  try {
    await new Promise(resolve => { stop = watchTransfer('id', 'sender', () => { notifications++; resolve(); }); });
    await new Promise(resolve => setTimeout(resolve, 750));
    assert.equal(calls, 1);
    assert.equal(notifications, 1);
  } finally { stop?.(); globalThis.fetch = original; }
});

test('completion and own cancellation do not report peer cancellation', { timeout: 1000 }, async () => {
  const original = globalThis.fetch;
  try {
    for (const data of [{ done: true }, { cancelled_by: 'sender' }]) {
      globalThis.fetch = async () => json({ data });
      const stop = watchTransfer('id', 'sender', () => assert.fail('Incorrect peer notification'));
      await new Promise(resolve => setTimeout(resolve, 20));
      stop();
    }
  } finally { globalThis.fetch = original; }
});

test('cancel notification uses separate credentials and survives local abort', async () => {
  const original = globalThis.fetch;
  const local = new AbortController();
  local.abort();
  globalThis.fetch = async (url, init) => {
    assert.equal(url, '/api/fileflow/id/cancel');
    assert.notEqual(init.signal, local.signal);
    assert.equal(init.signal.aborted, false);
    assert.deepEqual(JSON.parse(init.body), { role: 'sender', sender_token: 'secret' });
    return json({ success: true });
  };
  try { await cancelTransfer('id', 'sender', 'secret'); } finally { globalThis.fetch = original; }
});

test('sender cancellation token is a fresh 256-bit random credential', () => {
  const first = createSenderToken();
  assert.match(first, /^[a-f0-9]{64}$/);
  assert.notEqual(first, createSenderToken());
});
