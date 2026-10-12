import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import ts from 'typescript';

const compile = source => {
  const { outputText } = ts.transpileModule(source, {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ES2022 },
  });
  return `data:text/javascript;base64,${Buffer.from(outputText).toString('base64')}`;
};
const http = compile(await readFile(new URL('../src/utils/http.ts', import.meta.url), 'utf8'));
const control = compile((await readFile(new URL('../src/utils/transferControl.ts', import.meta.url), 'utf8')).replace("'./http'", JSON.stringify(http)));
const source = (await readFile(new URL('../src/utils/upload.ts', import.meta.url), 'utf8')).replace("'./http'", JSON.stringify(http)).replace("'./transferControl'", JSON.stringify(control));
const { uploadFile } = await import(compile(source));
const json = (body, status = 200) => new Response(JSON.stringify(body), { status });

test('waiting admission sends only metadata and allocates no multipart body', { timeout: 2000 }, async () => {
  const original = globalThis.fetch;
  let polls = 0;
  let bodies = 0;
  globalThis.fetch = async (url, init) => {
    if (url.endsWith('/admit')) {
      assert.equal(bodies, 0);
      assert.deepEqual(JSON.parse(init.body), { start: 0, length: 4 });
      return json({ success: true, data: ++polls < 3
        ? { status: 'waiting', retry_after_ms: 1 }
        : { status: 'ready', token: 'permit', expires_ms: 30000 } });
    }
    assert.equal(polls, 3);
    assert.equal(init.headers['X-Upload-Permit'], 'permit');
    assert.equal(bodies, 1);
    return json({ code: 200 });
  };
  try {
    assert.equal(await uploadFile(() => { bodies++; return new FormData(); }, 'id', 0, 4, 0), 4);
  } finally { globalThis.fetch = original; }
});

test('a lost upload response is recovered without sending the body twice', { timeout: 2000 }, async () => {
  const original = globalThis.fetch;
  let committed = false;
  let bodies = 0;
  globalThis.fetch = async url => {
    if (url.endsWith('/admit')) return json({ success: true, data: committed
      ? { status: 'complete' } : { status: 'ready', token: 'permit', expires_ms: 30000 } });
    committed = true;
    throw new TypeError('Response lost after commit');
  };
  try {
    assert.equal(await uploadFile(() => { bodies++; return new FormData(); }, 'id', 0, 4, 0), 4);
    assert.equal(bodies, 1);
  } finally { globalThis.fetch = original; }
});

test('expired permits are renewed before retrying an upload', { timeout: 2000 }, async () => {
  const original = globalThis.fetch;
  let permits = 0;
  let uploads = 0;
  globalThis.fetch = async (url, init) => {
    if (url.endsWith('/admit')) return json({ success: true, data: { status: 'ready', token: String(++permits), expires_ms: 30000 } });
    assert.equal(init.headers['X-Upload-Permit'], String(permits));
    return ++uploads === 1 ? json({ message: 'expired' }, 409) : json({ code: 200 });
  };
  try {
    assert.equal(await uploadFile(() => new FormData(), 'id', 0, 4, 0), 4);
    assert.equal(permits, 2);
  } finally { globalThis.fetch = original; }
});

test('cancellation while waiting for admission never sends a body or retries', { timeout: 1000 }, async () => {
  const original = globalThis.fetch;
  const controller = new AbortController();
  let polls = 0;
  globalThis.fetch = async () => {
    polls++;
    setTimeout(() => controller.abort(new DOMException('stopped', 'AbortError')), 10);
    return json({ success: true, data: { status: 'waiting', retry_after_ms: 2000 } });
  };
  try {
    await assert.rejects(uploadFile(() => { assert.fail('Multipart body must not be created'); }, 'id', 0, 4, 0, controller.signal), { name: 'AbortError' });
    assert.equal(polls, 1);
  } finally { globalThis.fetch = original; }
});

test('server cancellation exits admission immediately without retrying', async () => {
  const original = globalThis.fetch;
  let calls = 0;
  globalThis.fetch = async () => { calls++; return json({ success: false, message: 'Transfer cancelled' }, 410); };
  try {
    await assert.rejects(uploadFile(() => { assert.fail('Cancelled transfer must not send data'); }, 'id', 0, 4, 0), { name: 'PeerCancelledError' });
    assert.equal(calls, 1);
  } finally { globalThis.fetch = original; }
});
