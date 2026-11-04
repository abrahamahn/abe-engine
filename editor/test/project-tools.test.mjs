import assert from 'node:assert/strict';
import test from 'node:test';
import { resolveProjectTools, withEmbedStudioParam } from '../project-tools.js';

const host = 'http://localhost:7480';
const manifest = { version: 1, origin: 'http://127.0.0.1:5173', tools: [{ label: 'Level rules', path: '/tools/levels?view=all' }] };
test('project routes stay at the game origin including on a different local development port', () => {
  const resolved = resolveProjectTools(manifest, host);
  assert.equal(resolved[0].href, 'http://127.0.0.1:5173/tools/levels?view=all');
  assert.equal(resolved[0].embedHref, 'http://127.0.0.1:5173/tools/levels?view=all&embed=studio');
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

test('a workspace needing a top-level game session can declare a link', () => {
  const tools = [{label:'Playtest control',path:'/playtest',open:'tab'}];
  assert.equal(resolveProjectTools({...manifest, tools}, host)[0].open, 'tab');
  assert.throws(() => resolveProjectTools({...manifest, tools:[{...tools[0],open:'javascript'}]}, host));
});

test('CloudStudioHost allows explicit same-origin docked tools with ?embed=studio', () => {
  const cloudManifest = {
    version: 1,
    origin: 'same-origin',
    tools: [
      { id: 'motion-lab', label: 'Motion Lab', path: '/ganbate-editor/motion' },
      { id: 'ops', label: 'Operations', path: '/ganbate-editor/operations', open: 'tab' },
    ],
  };
  const resolved = resolveProjectTools(cloudManifest, 'https://ganbate.example', null, { allowSameOrigin: true });
  assert.equal(resolved[0].embedHref, 'https://ganbate.example/ganbate-editor/motion?embed=studio');
  assert.equal(resolved[1].href, 'https://ganbate.example/ganbate-editor/operations');
  assert.equal(withEmbedStudioParam('https://ganbate.example/tool?embed=custom'), 'https://ganbate.example/tool?embed=custom');
});
