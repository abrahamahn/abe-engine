import assert from 'node:assert/strict';
import test from 'node:test';
import {
  clearFrameRing,
  computeFramePercentiles,
  computeMipChainBytes,
  computeVramCompressionBudget,
  createFrameRing,
  evaluateSceneCulling,
  mountTelemetryProfiler,
  pushFrameSample,
} from '../telemetry-profiler.js';

function createMockContainer() {
  const listeners = new Map();
  return {
    innerHTML: '',
    addEventListener(type, fn) {
      listeners.set(type, fn);
    },
    removeEventListener(type) {
      listeners.delete(type);
    },
    dispatchClick(action) {
      const fn = listeners.get('click');
      if (fn) {
        fn({
          target: {
            closest(sel) {
              return sel === '[data-tp-action]' ? { getAttribute: () => action } : null;
            },
          },
        });
      }
    },
  };
}

test('rolling 120-frame ring buffer computes exact p50, p95, p99 and bottleneck state without overflow', () => {
  const ring = createFrameRing(120);
  for (let i = 1; i <= 150; i += 1) {
    // Last 120 samples are 31..150 -> CPU 3.1..15.0 ms, GPU 6.2..30.0 ms.
    pushFrameSample(ring, {
      frameTimeMs: i * 0.1,
      gpuTimeMs: i * 0.2,
      draws: 48,
      triangles: 120000,
    });
  }
  assert.equal(ring.count, 120);
  const summary = computeFramePercentiles(ring);
  assert.equal(summary.samples, 120);
  assert.equal(summary.draws, 48);
  assert.equal(summary.triangles, 120000);
  // 50th percentile of 31..150 (index 59 -> 90): 9.0 ms CPU, 18.0 ms GPU.
  assert.ok(Math.abs(summary.cpu.p50 - 9.0) < 1e-6);
  assert.ok(Math.abs(summary.gpu.p50 - 18.0) < 1e-6);
  // 95th percentile (index 113 -> 144): 14.4 ms CPU, 28.8 ms GPU (> 16.67 ms -> gpu-bound).
  assert.ok(Math.abs(summary.cpu.p95 - 14.4) < 1e-6);
  assert.ok(Math.abs(summary.gpu.p95 - 28.8) < 1e-6);
  assert.equal(summary.bottleneck, 'gpu-bound');

  clearFrameRing(ring);
  assert.equal(computeFramePercentiles(ring).samples, 0);
});

test('VRAM compression budget matches 4x4 physical mip block alignment for BC1 (8:1), BC7 (4:1) and ASTC 4x4', () => {
  // Single 2048x2048 base level without mips:
  // RGBA8 = 16,777,216 B; BC1 = 2,097,152 B (exact 8x); BC7 / ASTC = 4,194,304 B (exact 4x).
  assert.equal(computeMipChainBytes(2048, 2048, 'rgba8', false), 16777216);
  assert.equal(computeMipChainBytes(2048, 2048, 'bc1', false), 2097152);
  assert.equal(computeMipChainBytes(2048, 2048, 'bc7', false), 4194304);
  assert.equal(computeMipChainBytes(2048, 2048, 'astc4x4', false), 4194304);

  // Sub-block 2x2 mip still occupies one physical 4x4 block (8 B BC1, 16 B BC7).
  assert.equal(computeMipChainBytes(2, 2, 'bc1', false), 8);
  assert.equal(computeMipChainBytes(2, 2, 'bc7', false), 16);

  const budget = computeVramCompressionBudget([{ width: 2048, height: 2048, mips: true }]);
  assert.ok(budget.bc1Ratio > 7.99 && budget.bc1Ratio <= 8.0);
  assert.ok(budget.bc7Ratio > 3.99 && budget.bc7Ratio <= 4.0);
  assert.ok(budget.astcRatio > 3.99 && budget.astcRatio <= 4.0);
});

test('scene culling classifies visible, frustum-culled, distance-culled, subpixel-culled and orders Early-Z front-to-back', () => {
  const entities = {
    nearHero: {
      components: { transform: { translation: [0, 0, -6] }, bounds: { min: [-1, -1, -1], max: [1, 1, 1] } },
    },
    midWall: {
      components: { transform: { translation: [0, 0, -20] }, bounds: { min: [-2, -2, -1], max: [2, 2, 1] } },
    },
    behindCamera: {
      components: { transform: { translation: [0, 0, 12] }, bounds: { min: [-1, -1, -1], max: [1, 1, 1] } },
    },
    beyondMaxDist: {
      components: { transform: { translation: [0, 0, -180] }, bounds: { min: [-2, -2, -2], max: [2, 2, 2] } },
    },
    tinySubpixelPebble: {
      components: { transform: { translation: [0, 0, -90] }, bounds: { min: [-0.005, -0.005, -0.005], max: [0.005, 0.005, 0.005] } },
    },
  };
  const camera = { position: [0, 0, 0], forward: [0, 0, -1], up: [0, 1, 0], fov: 60, near: 0.1, far: 400 };
  const res = evaluateSceneCulling(entities, camera, {
    maxDistance: 120,
    minScreenPx: 2.0,
    viewportHeightPx: 1080,
  });

  assert.equal(res.total, 5);
  assert.equal(res.visible, 2);
  assert.equal(res.frustumCulled, 1);
  assert.equal(res.distanceCulled, 1);
  assert.equal(res.subpixelCulled, 1);
  assert.deepEqual(res.visibleOrder, ['nearHero', 'midWall']);
});

test('mountTelemetryProfiler mounts cleanly, freezes camera frustum across live camera orbits, and unmounts cleanly', () => {
  const container = createMockContainer();
  let subscriber = null;
  const notifications = [];
  let state = {
    locale: 'en',
    assets: [{ id: 'a1', kind: 'texture', path: 'textures/albedo.png', bytes: 3145728 }],
    activeDocument: {
      id: 'scene1',
      path: 'scenes/main.scene.json',
      kind: 'scene',
      rev: 1,
      dirty: false,
      undo: null,
      redo: null,
      root: {
        entities: {
          crateFront: { components: { transform: { translation: [0, 0, -8] } } },
          crateBack: { components: { transform: { translation: [0, 0, 15] } } },
        },
      },
    },
    selection: {
      entityIds: [],
      activeCamera: { position: [0, 0, 0], forward: [0, 0, -1], up: [0, 1, 0], fov: 60, near: 0.1, far: 200 },
    },
    stats: {
      fps: 144,
      frameTimeMs: 2.4,
      gpuTimeMs: 4.1,
      draws: 18,
      triangles: 24500,
      vramBytes: 8388608,
      backend: 'WebGPU',
    },
  };

  const studioApi = {
    getState: () => state,
    getEngine: () => ({
      compressedFormats: { bc: true, astc: false },
      bundleStats: () => ({ cachedPasses: 3, hits: 120, misses: 3 }),
    }),
    subscribe(fn) {
      subscriber = fn;
      return () => { subscriber = null; };
    },
    notify(level, msg) {
      notifications.push({ level, msg });
    },
  };

  const panel = mountTelemetryProfiler(container, studioApi);
  assert.ok(container.innerHTML.includes('Telemetry &amp; Culling Profiler'));
  assert.ok(container.innerHTML.includes('RenderBundles: <strong>3</strong>'));
  assert.equal(panel.isCameraFrozen(), false);

  // Freeze frustum while looking down -Z (1 visible, 1 frustum-culled).
  container.dispatchClick('toggle-freeze');
  assert.equal(panel.isCameraFrozen(), true);
  assert.ok(container.innerHTML.includes('(FROZEN)'));
  assert.equal(notifications.length, 1);

  // Orbit live camera to look down +Z: frozen frustum must keep culling against the original -Z cone!
  state = {
    ...state,
    locale: 'ko',
    selection: {
      ...state.selection,
      activeCamera: { position: [0, 0, 50], forward: [0, 0, 1], up: [0, 1, 0], fov: 60, near: 0.1, far: 200 },
    },
  };
  subscriber(state);
  assert.ok(container.innerHTML.includes('텔레메트리'));
  assert.ok(container.innerHTML.includes('가시 객체: <strong>1</strong> / 2'));

  panel.destroy();
  assert.equal(container.innerHTML, '');
  assert.equal(subscriber, null);
});
