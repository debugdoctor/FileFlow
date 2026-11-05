import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import ts from 'typescript';

const source = await readFile(new URL('../src/utils/downloadCapability.ts', import.meta.url), 'utf8');
const { outputText } = ts.transpileModule(source, {
  compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ES2022 },
});
const { isMobileDownloadClient } = await import(`data:text/javascript;base64,${Buffer.from(outputText).toString('base64')}`);

test('mobile browsers use the browser default download flow', () => {
  assert.equal(isMobileDownloadClient('Mozilla/5.0 (Linux; Android 15) AppleWebKit/537.36 Chrome/140 Mobile Safari/537.36'), true);
  assert.equal(isMobileDownloadClient('Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 Mobile/15E148'), true);
});

test('iPad desktop mode is still treated as mobile', () => {
  assert.equal(isMobileDownloadClient('Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15) AppleWebKit/605.1.15 Safari/605.1.15', 5), true);
});

test('user agent client hints take priority and desktop remains eligible', () => {
  assert.equal(isMobileDownloadClient('Mozilla/5.0 (X11; Linux x86_64) Chrome/140 Safari/537.36', 0, true), true);
  assert.equal(isMobileDownloadClient('Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/140 Safari/537.36', 0, false), false);
});
