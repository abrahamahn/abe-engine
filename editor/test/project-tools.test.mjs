import assert from 'node:assert/strict';
import test from 'node:test';
import {
  BUILTIN_GAMEPLAY_TOOLS,
  normalizeLoopbackQueryOverride,
  resolveGameplayTools,
  resolveProjectTools,
  resolveTableLayoutTools,
  withEmbedStudioParam,
} from '../project-tools.js';

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

test('normalizes localhost and 127.0.0.1 loopback origins cleanly while preserving cross-hostname isolation', () => {
  const localhostManifest = {
    version: 1,
    origin: 'http://localhost:5173',
    tools: [{ label: 'Table layouts', path: '/ganbate-editor/layout-tuner' }],
  };
  const onLocalhostStudio = resolveProjectTools(localhostManifest, 'http://localhost:7480');
  assert.equal(onLocalhostStudio[0].href, 'http://127.0.0.1:5173/ganbate-editor/layout-tuner');
  assert.equal(onLocalhostStudio[0].embedHref, 'http://127.0.0.1:5173/ganbate-editor/layout-tuner?embed=studio');

  const onIpv6Studio = resolveProjectTools(localhostManifest, 'http://[::1]:7480');
  assert.equal(onIpv6Studio[0].href, 'http://127.0.0.1:5173/ganbate-editor/layout-tuner');

  const onIpv4Studio = resolveProjectTools(manifest, 'http://127.0.0.1:7480');
  assert.equal(onIpv4Studio[0].href, 'http://localhost:5173/tools/levels?view=all');

  assert.equal(
    normalizeLoopbackQueryOverride('http://127.0.0.1:5173', 'http://127.0.0.1:7480'),
    'http://localhost:5173',
  );
  assert.equal(
    normalizeLoopbackQueryOverride('http://localhost:5173', 'http://localhost:7480'),
    'http://127.0.0.1:5173',
  );
  // Same-port self-embedding must never be rewritten away.
  assert.equal(
    normalizeLoopbackQueryOverride('http://127.0.0.1:7480', 'http://127.0.0.1:7480'),
    'http://127.0.0.1:7480',
  );
});

test('resolveTableLayoutTools discovers layout-tuner and companion graphics-layout-tuner routes', () => {
  const localhostManifest = {
    version: 1,
    origin: 'http://localhost:5173',
    tools: [{ label: 'Table layouts', path: '/ganbate-editor/layout-tuner' }],
  };
  const tools = resolveProjectTools(localhostManifest, 'http://localhost:7480');
  const pair = resolveTableLayoutTools(tools);
  assert.equal(pair.layoutTuner?.embedHref, 'http://127.0.0.1:5173/ganbate-editor/layout-tuner?embed=studio');
  assert.equal(
    pair.graphicsLayoutTuner?.embedHref,
    'http://127.0.0.1:5173/ganbate-editor/graphics-layout-tuner?embed=studio',
  );
});

test('resolveGameplayTools resolves motion-tuner, action-sequencer, god-mode, and audio-mixer with loopback normalization', () => {
  assert.deepEqual(
    BUILTIN_GAMEPLAY_TOOLS.map((t) => [t.id, t.path, t.open]),
    [
      ['motion-tuner', '/__studio-tool/motion-tuner', 'embed'],
      ['action-sequencer', '/__studio-tool/action-sequencer', 'embed'],
      ['god-mode', '/__studio-tool/god-mode', 'embed'],
      ['audio-mixer', '/__studio-tool/audio-mixer', 'embed'],
    ],
  );

  const localhostManifest = {
    version: 1,
    origin: 'http://localhost:5173',
    tools: [
      { label: 'Table layouts', path: '/ganbate-editor/layout-tuner' },
      { label: 'Sound & Foley', path: '/ganbate-editor/audio-mixer' },
    ],
  };
  const tools = resolveProjectTools(localhostManifest, 'http://localhost:7480');
  const gp = resolveGameplayTools(tools);
  assert.equal(gp.motionTuner?.embedHref, 'http://127.0.0.1:5173/__studio-tool/motion-tuner?embed=studio');
  assert.equal(gp.actionSequencer?.embedHref, 'http://127.0.0.1:5173/__studio-tool/action-sequencer?embed=studio');
  assert.equal(gp.godMode?.embedHref, 'http://127.0.0.1:5173/__studio-tool/god-mode?embed=studio');
  assert.equal(gp.audioMixer?.id, 'sound-foley');
  assert.equal(gp.audioMixer?.embedHref, 'http://127.0.0.1:5173/ganbate-editor/audio-mixer?embed=studio');

  // Direct studioOrigin normalization on raw un-normalized tool list when Studio runs on 127.0.0.1:7480.
  const rawTools = [{ id: 'layout-tuner', label: 'Table layouts', href: 'http://127.0.0.1:5173/ganbate-editor/layout-tuner' }];
  const normalizedOnIpv4 = resolveGameplayTools(rawTools, 'http://127.0.0.1:7480');
  assert.equal(normalizedOnIpv4.motionTuner?.embedHref, 'http://localhost:5173/__studio-tool/motion-tuner?embed=studio');
  assert.equal(normalizedOnIpv4.actionSequencer?.embedHref, 'http://localhost:5173/__studio-tool/action-sequencer?embed=studio');
  assert.equal(normalizedOnIpv4.godMode?.embedHref, 'http://localhost:5173/__studio-tool/god-mode?embed=studio');
  assert.equal(normalizedOnIpv4.audioMixer?.embedHref, 'http://localhost:5173/__studio-tool/audio-mixer?embed=studio');
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
      { id: 'layout-tuner', label: 'Layout Tuner', path: '/ganbate-editor/layout-tuner', open: 'embed' },
      { id: 'ops', label: 'Operations', path: '/ganbate-editor/operations', open: 'tab' },
    ],
  };
  const resolved = resolveProjectTools(cloudManifest, 'https://ganbate.example', null, { allowSameOrigin: true });
  assert.equal(resolved[0].embedHref, 'https://ganbate.example/ganbate-editor/motion?embed=studio');
  assert.equal(resolved[1].open, 'embed');
  assert.equal(resolved[1].embedHref, 'https://ganbate.example/ganbate-editor/layout-tuner?embed=studio');
  assert.equal(resolved[2].href, 'https://ganbate.example/ganbate-editor/operations');
  assert.equal(withEmbedStudioParam('https://ganbate.example/tool?embed=custom'), 'https://ganbate.example/tool?embed=custom');
});
