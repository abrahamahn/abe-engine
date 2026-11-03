import assert from 'node:assert/strict';
import test from 'node:test';
import { resolveProjectTools } from '../project-tools.js';

const host = 'http://localhost:7480';
const manifest = { version: 1, origin: 'http://127.0.0.1:5173', tools: [{ label: 'Level rules', path: '/tools/levels?view=all' }] };
test('project routes stay at the game origin including on a different local development port', () => {
  assert.equal(resolveProjectTools(manifest, host)[0].href, 'http://127.0.0.1:5173/tools/levels?view=all');
  assert.equal(resolveProjectTools(manifest, host, 'http://127.0.0.1:4111')[0].href, 'http://127.0.0.1:4111/tools/levels?view=all');
});
test('rejects script and cross-origin paths, host self-embedding and nonlocal overrides', () => {
  for (const path of ['javascript:alert(1)', '//example.test/page', '/\\example.test/page']) {
    assert.throws(() => resolveProjectTools({ ...manifest, tools: [{label:'Level rules', path}] }, host));
  }
  assert.throws(() => resolveProjectTools(manifest, host, host));
  assert.throws(() => resolveProjectTools(manifest, host, 'http://localhost:5173'));
  assert.throws(() => resolveProjectTools(manifest, host, 'https://example.test'));
});
