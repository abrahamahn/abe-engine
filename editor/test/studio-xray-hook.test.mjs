import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

import { setLang, t } from '../i18n.js';

const here = path.dirname(fileURLToPath(import.meta.url));
const editorDir = path.resolve(here, '..');

test('editor/index.html and i18n.js expose the Project X-Ray (code-xray) workbench tab', () => {
  const html = readFileSync(path.join(editorDir, 'index.html'), 'utf8');
  assert.match(html, /data-workbench="code-xray"/);
  assert.match(html, /Project X-Ray/);

  setLang('en');
  assert.equal(t('wb_code_xray'), 'Project X-Ray');
  setLang('ko');
  assert.equal(t('wb_code_xray'), '프로젝트 X-Ray');
  setLang('en');
});

test('editor/studio.js wires code-xray dynamic import, inline-style workbenches, and viewport pose helpers', () => {
  const src = readFileSync(path.join(editorDir, 'studio.js'), 'utf8');
  assert.match(src, /id:\s*'code-xray'/);
  assert.match(src, /modulePath:\s*'\.\/code-xray\.js'/);
  assert.match(src, /exportName:\s*'mountCodeXray'/);
  assert.match(src, /id:\s*'material'[\s\S]*?cssPath:\s*null/);
  assert.match(src, /id:\s*'profiler'[\s\S]*?cssPath:\s*null/);
  assert.match(src, /setCameraPose:\s*\(/);
  assert.match(src, /setLightingPreset:\s*\(/);
});
