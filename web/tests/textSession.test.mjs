import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import ts from 'typescript';

const compile = source => {
  const { outputText } = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ES2022 } });
  return `data:text/javascript;base64,${Buffer.from(outputText).toString('base64')}`;
};
const http = compile(await readFile(new URL('../src/utils/http.ts', import.meta.url), 'utf8'));
const { parseRoomCode, mergeMessages, randomHex, textRequest, TextSessionError } = await import(compile(
  (await readFile(new URL('../src/utils/textSession.ts', import.meta.url), 'utf8')).replace("'./http'", JSON.stringify(http)),
));

test('room entry uses the same five-character alphabet as file transfers and accepts invitation links', () => {
  for (const input of [' A3B7K ', 'a3b7k', 'https://example.com/text?room=A3B7K']) {
    assert.equal(parseRoomCode(input), 'a3b7k');
  }
  for (const input of ['abcd', 'abcdef', 'abc-d', 'https://example.com/text', '<script>']) assert.equal(parseRoomCode(input), '');
  assert.match(randomHex(32), /^[a-f0-9]{64}$/);
  assert.notEqual(randomHex(32), randomHex(32));
});

const message = (seq, text = `message ${seq}`) => ({ seq, id: String(seq), author: seq % 3 + 1, author_name: `匿名成员 ${seq % 3 + 1}`, text, created_at: 0 });

test('polling and send responses can arrive out of order without losing earlier peer messages or duplicating echoes', () => {
  const afterSend = mergeMessages([message(1)], [message(3)]);
  assert.deepEqual(mergeMessages(afterSend, [message(2), message(3)]).map(item => item.seq), [1, 2, 3]);
  assert.deepEqual(mergeMessages(afterSend, [message(1)]).map(item => item.seq), [1, 3]);
});

test('client history retains newest messages within count and UTF-8 byte bounds', () => {
  const many = mergeMessages([], Array.from({ length: 110 }, (_, index) => message(index + 1)));
  assert.equal(many.length, 100);
  assert.equal(many[0].seq, 11);
  const large = mergeMessages([], Array.from({ length: 6 }, (_, index) => message(index + 1, '中'.repeat(20000))));
  assert.deepEqual(large.map(item => item.seq), [3, 4, 5, 6]);
});

test('message requests keep credentials out of URLs and preserve idempotency IDs and original text', async () => {
  const original = globalThis.fetch;
  const payload = { id: 'a'.repeat(32), text: '  中文\n<script>hello</script> 😀  ' };
  globalThis.fetch = async (url, init) => {
    assert.equal(url, '/api/fileflow/text/rooms/a3b7k/messages');
    assert.equal(init.headers.Authorization, `Bearer ${'b'.repeat(64)}`);
    assert.equal(init.method, 'POST');
    assert.equal(init.cache, 'no-store');
    assert.deepEqual(JSON.parse(init.body), payload);
    return Response.json({ success: true, data: message(1, payload.text) });
  };
  try { assert.equal((await textRequest('/a3b7k/messages', { token: 'b'.repeat(64), body: payload })).text, payload.text); }
  finally { globalThis.fetch = original; }
});

test('ended/full rooms surface server status and can be distinguished from transient network failures', async () => {
  const original = globalThis.fetch;
  try {
    for (const status of [403, 404, 409]) {
      globalThis.fetch = async () => Response.json({ success: false, message: '房间不可用' }, { status });
      await assert.rejects(textRequest('/a3b7k'), error => error instanceof TextSessionError && error.status === status && error.message === '房间不可用');
    }
  } finally { globalThis.fetch = original; }
});
