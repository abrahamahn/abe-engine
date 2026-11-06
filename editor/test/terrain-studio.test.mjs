import assert from 'node:assert/strict';
import test from 'node:test';
import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  TERRAIN_BRUSH_MODES,
  TERRAIN_SKY_PRESETS,
  DEFAULT_SPLAT_LAYERS,
  createDefaultTerrainDoc,
  applyTerrainBrush,
  evaluateCatmullRomSpline,
  applySplineWalkways,
  computeSplatmapWeights,
  generateSplineEdgeScatter,
  summarizeTerrainForAi,
  exportTerrainToGbs,
  buildTerrainSceneEntities,
  mountTerrainStudio,
} from '../terrain-studio.js';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

class FakeElement {
  constructor(tagName, ownerDocument) {
    this.tagName = String(tagName || 'div').toUpperCase();
    this.ownerDocument = ownerDocument;
    this.attributes = new Map();
    this.style = {};
    this.children = [];
    this.listeners = new Map();
    this.className = '';
    this.textContent = '';
    this.value = '';
    this.width = 920;
    this.height = 580;
    this.classList = {
      add: (...tokens) => {
        const set = new Set(this.className.split(/\s+/).filter(Boolean));
        for (const t of tokens) set.add(t);
        this.className = [...set].join(' ');
      },
      remove: (...tokens) => {
        const set = new Set(this.className.split(/\s+/).filter(Boolean));
        for (const t of tokens) set.delete(t);
        this.className = [...set].join(' ');
      },
      toggle: (token, force) => {
        const set = new Set(this.className.split(/\s+/).filter(Boolean));
        const shouldAdd = force !== undefined ? Boolean(force) : !set.has(token);
        if (shouldAdd) set.add(token);
        else set.delete(token);
        this.className = [...set].join(' ');
        return shouldAdd;
      },
      contains: (token) => this.className.split(/\s+/).filter(Boolean).includes(token),
    };
  }

  setAttribute(k, v) {
    this.attributes.set(k, String(v));
  }

  getAttribute(k) {
    return this.attributes.get(k) ?? null;
  }

  addEventListener(type, fn) {
    const arr = this.listeners.get(type) || [];
    arr.push(fn);
    this.listeners.set(type, arr);
  }

  append(...nodes) {
    for (const n of nodes) {
      if (n == null) continue;
      if (typeof n === 'object') {
        n.parentNode = this;
        this.children.push(n);
      }
    }
  }

  replaceChildren(...nodes) {
    this.children = [];
    this.append(...nodes);
  }

  getBoundingClientRect() {
    return { left: 0, top: 0, width: this.width || 920, height: this.height || 580 };
  }

  getContext(kind) {
    if (kind !== '2d') return null;
    return {
      createLinearGradient: () => ({ addColorStop: () => {} }),
      fillRect: () => {},
      beginPath: () => {},
      moveTo: () => {},
      lineTo: () => {},
      closePath: () => {},
      fill: () => {},
      stroke: () => {},
      arc: () => {},
    };
  }
}

function createFakeContainer() {
  const doc = {
    createElement(tag) {
      return new FakeElement(tag, doc);
    },
    createTextNode(text) {
      return { nodeType: 3, textContent: String(text) };
    },
  };
  return new FakeElement('div', doc);
}

test('terrain-studio: createDefaultTerrainDoc produces 1m-grid heightmap, 4 splat layers, water, sky, splines, and scatter', () => {
  const doc = createDefaultTerrainDoc('Alpine Test', 'alpine_valley', { cols: 24, rows: 24, cellSize: 1.0 });
  assert.equal(doc.ganbate, 'terrain');
  assert.equal(doc.version, 1);
  assert.equal(doc.grid.cols, 24);
  assert.equal(doc.grid.rows, 24);
  assert.equal(doc.grid.cellSize, 1.0);
  assert.equal(doc.grid.heights.length, 24 * 24);
  assert.equal(doc.splatLayers.length, 4);
  assert.equal(doc.sky.preset, 'midday');
  assert.equal(doc.sky.timeHours, 13.0);
  assert.ok(Array.isArray(doc.splines) && doc.splines.length >= 1);
  assert.ok(Array.isArray(doc.scatter) && doc.scatter.length >= 1);
});

test('terrain-studio: applyTerrainBrush supports raise, lower, smooth, flatten, and terrace deterministically', () => {
  const doc = createDefaultTerrainDoc('Brush Test', 'sanctuary_garden', { cols: 16, rows: 16, cellSize: 1.0 });
  const centerIdx = 8 * 16 + 8;
  const beforeCenter = doc.grid.heights[centerIdx];

  const raised = applyTerrainBrush(doc.grid, { mode: 'raise', x: 0, z: 0, radius: 4, strength: 0.8 });
  assert.equal(raised.changed, true);
  assert.ok(raised.affectedVertices > 0);
  assert.ok(raised.nextHeights[centerIdx] > beforeCenter);

  const lowered = applyTerrainBrush(
    { ...doc.grid, heights: raised.nextHeights },
    { mode: 'lower', x: 0, z: 0, radius: 4, strength: 0.8 },
  );
  assert.equal(lowered.changed, true);
  assert.ok(lowered.nextHeights[centerIdx] < raised.nextHeights[centerIdx]);

  const flattened = applyTerrainBrush(doc.grid, {
    mode: 'flatten',
    x: 0,
    z: 0,
    radius: 5,
    strength: 1.0,
    targetHeight: 2.5,
  });
  assert.equal(flattened.changed, true);
  assert.ok(Math.abs(flattened.nextHeights[centerIdx] - 2.5) < 0.25);

  const terraced = applyTerrainBrush(
    { ...doc.grid, heights: raised.nextHeights },
    { mode: 'terrace', x: 0, z: 0, radius: 6, strength: 1.0, terraceStep: 1.0 },
  );
  assert.equal(terraced.changed, true);

  const smoothed = applyTerrainBrush(
    { ...doc.grid, heights: raised.nextHeights },
    { mode: 'smooth', x: 0, z: 0, radius: 5, strength: 0.9 },
  );
  assert.equal(smoothed.changed, true);
});

test('terrain-studio: Catmull-Rom spline walkways flatten terrain corridor and compute normalized 4-layer splat weights', () => {
  const doc = createDefaultTerrainDoc('Spline Test', 'alpine_valley', { cols: 20, rows: 20, cellSize: 1.0 });
  const pts = [[-6, 1.0, -6], [0, 1.0, 0], [6, 1.0, 6]];
  const sampled = evaluateCatmullRomSpline(pts, 8);
  assert.ok(sampled.length > 10);

  const graded = applySplineWalkways(doc.grid, [{
    id: 'test-road',
    width: 3.0,
    falloff: 1.5,
    flattenStrength: 1.0,
    layerIndex: 1,
    points: pts,
  }]);
  assert.equal(graded.nextHeights.length, 20 * 20);
  const maskHits = graded.pathMask.filter(Boolean);
  assert.ok(maskHits.length > 0);

  const weights = computeSplatmapWeights(
    { ...doc.grid, heights: graded.nextHeights },
    DEFAULT_SPLAT_LAYERS,
    doc.water,
    [{ id: 'test-road', width: 3.0, falloff: 1.5, flattenStrength: 1.0, layerIndex: 1, points: pts }],
  );
  assert.equal(weights.length, 20 * 20);
  for (const w of weights) {
    assert.equal(w.length, 4);
    const sum = w[0] + w[1] + w[2] + w[3];
    assert.ok(Math.abs(sum - 1.0) < 0.02, `expected normalized splat weights, got sum=${sum}`);
  }
});

test('terrain-studio: PCG spline edge scatter avoids underwater cells and summarizeTerrainForAi stays under 120 tokens', () => {
  const doc = createDefaultTerrainDoc('Scatter Test', 'coastal_archipelago', {
    cols: 24,
    rows: 24,
    assets: [{ id: 'asset_lantern_01', kind: 'model', path: 'content/models/stone_lantern.glb' }],
  });
  const scatter = generateSplineEdgeScatter(doc.grid, doc.splines, doc.water, [
    { id: 'asset_lantern_01', kind: 'model', path: 'content/models/stone_lantern.glb' },
  ], 42);
  assert.ok(scatter.length > 0);
  for (const item of scatter) {
    assert.ok(item.translation[1] > doc.water.level, 'scatter item must stay above water level');
    assert.equal(item.assetId, 'asset_lantern_01');
  }

  const digest = summarizeTerrainForAi(doc);
  assert.ok(digest.text.includes('Scatter Test'));
  assert.ok(digest.tokensApprox < 120, `expected <120 tokens, got ${digest.tokensApprox}`);

  const gbs = exportTerrainToGbs(doc);
  assert.ok(gbs.includes('spawn TerrainRoot'));
  assert.ok(gbs.includes('spawn WaterSurface'));

  const entities = buildTerrainSceneEntities(doc);
  assert.ok(entities['terrain0000000001']?.components?.terrain);
  assert.ok(entities['terrain0000000002']?.components?.water);
});

test('terrain-studio: mountTerrainStudio integrates with studioApi.edit, sky presets, and syncToScene', async () => {
  const container = createFakeContainer();
  const edits = [];
  const timeUpdates = [];
  const api = {
    listAssets: () => [{ id: 'tree_01', kind: 'model', path: 'content/models/pine.glb' }],
    getActiveDocument: () => ({ id: 'scene_01', kind: 'scene', path: 'content/scenes/main.scene.json' }),
    edit: async (docId, ops, label, merge) => {
      edits.push({ docId, ops, label, merge });
      return { id: docId };
    },
    getViewport: () => ({
      setTime: (h) => timeUpdates.push(h),
      setWeather: () => {},
    }),
    notify: () => {},
  };

  const instance = mountTerrainStudio(container, api);
  instance.setDocument('terrain_doc_01', createDefaultTerrainDoc('Mounted Valley', 'alpine_valley', { cols: 16, rows: 16 }));

  const strokeRes = await instance.applyBrushStroke({ mode: 'raise', x: 0, z: 0, radius: 4, strength: 0.7 });
  assert.equal(strokeRes.changed, true);
  assert.equal(edits.length, 1);
  assert.equal(edits[0].docId, 'terrain_doc_01');

  await instance.applySkyPreset('midnight');
  assert.deepEqual(timeUpdates, [0.5]);
  assert.equal(instance.getState().doc.sky.preset, 'midnight');

  const syncedEntities = await instance.syncToScene();
  assert.ok(Object.keys(syncedEntities).length >= 2);
  assert.equal(edits.at(-1).docId, 'scene_01');

  assert.equal(TERRAIN_BRUSH_MODES.length, 6);
  assert.ok(TERRAIN_SKY_PRESETS.dawn && TERRAIN_SKY_PRESETS.dusk);
  instance.destroy();
});

test('terrain-studio: engine purity law (zero domain-specific gambling or character strings)', async () => {
  const jsSrc = await fs.readFile(path.resolve(__dirname, '../terrain-studio.js'), 'utf8');
  const cssSrc = await fs.readFile(path.resolve(__dirname, '../terrain-studio.css'), 'utf8');
  const forbidden = /\b(poker|casino|rei|yozakura|holdem|blackjack|roulette)\b/i;
  assert.equal(forbidden.test(jsSrc), false, 'terrain-studio.js must remain 100% genre-neutral');
  assert.equal(forbidden.test(cssSrc), false, 'terrain-studio.css must remain 100% genre-neutral');
});
