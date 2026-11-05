import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import ts from 'typescript';

const source = await readFile(new URL('../src/utils/messageText.ts', import.meta.url), 'utf8');
const { outputText } = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ES2022 } });
const { splitMessageText } = await import(`data:text/javascript;base64,${Buffer.from(outputText).toString('base64')}`);

test('clickable URLs preserve mixed text, whitespace, query parameters and balanced parentheses', () => {
  const text = '  文档：https://example.com/wiki/Test_(page)。\n另一个 (https://other.dev/a?q=1&b=2#part).  ';
  const parts = splitMessageText(text);
  assert.equal(parts.map(part => part.text).join(''), text);
  assert.deepEqual(parts.filter(part => part.href).map(part => part.href), [
    'https://example.com/wiki/Test_(page)', 'https://other.dev/a?q=1&b=2#part',
  ]);
});

test('unsafe schemes, invalid hosts and misleading credentials stay plain text', () => {
  for (const text of ['javascript:alert(1)', 'data:text/html,<script>alert(1)</script>', 'https://', 'https://safe.com@evil.com/']) {
    const parts = splitMessageText(text);
    assert.equal(parts.map(part => part.text).join(''), text);
    assert.equal(parts.filter(part => part.href).length, 0);
  }
  const text = '<img src=x onerror=alert(1)> https://example.com/?q=%3Cscript%3E';
  const parts = splitMessageText(text);
  assert.equal(parts[0].text, '<img src=x onerror=alert(1)> ');
  assert.equal(parts[1].host, 'example.com');
  assert.equal(parts.map(part => part.text).join(''), text);
});
