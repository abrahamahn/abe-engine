import assert from 'node:assert/strict';
import test from 'node:test';
import {
  assetBasename,
  assetExtension,
  assetFolder,
  assetStem,
  buildFolderTree,
  buildRenamedPath,
  canPlaceAsset,
  filterAssets,
  formatBytes,
  inFolder,
  kindCounts,
  placementEntity,
  projectAssetUrl,
  raycastDropPoint,
} from '../asset-library.js';
import { extractWavPeaks, parseGlbPreview } from '../asset-previews.js';

const SAMPLE_ASSETS = [
  { id: 'scn_main', kind: 'scene', path: 'content/scenes/courtyard.scene.json', bytes: 1840 },
  { id: 'pfb_arch', kind: 'prefab', path: 'content/prefabs/arch.prefab.json', bytes: 920 },
  { id: 'mdl_pillar', kind: 'model', path: 'content/models/props/pillar.glb', bytes: 24576 },
  { id: 'mdl_crate', kind: 'model', path: 'content/models/props/crate.glb', bytes: 8192 },
  { id: 'lok_day', kind: 'look', path: 'content/looks/day.look.json', bytes: 410 },
  { id: 'mat_stone', kind: 'material', path: 'content/materials/stone.material.json', bytes: 256 },
  { id: 'tex_stone', kind: 'texture', path: 'content/textures/stone albedo.png', bytes: 65536 },
  { id: 'snd_wind', kind: 'sound', path: 'content/audio/wind.ogg', bytes: 131072 },
];

function buildSyntheticGlb() {
  const gltf = {
    asset: { version: '2.0' },
    scene: 0,
    scenes: [{ nodes: [0] }],
    nodes: [{ mesh: 0, translation: [2, 1, -3] }],
    materials: [{ pbrMetallicRoughness: { baseColorFactor: [0.9, 0.4, 0.2, 1] } }],
    meshes: [{ primitives: [{ attributes: { POSITION: 0 }, indices: 1, material: 0 }] }],
    bufferViews: [
      { buffer: 0, byteOffset: 0, byteLength: 36 },
      { buffer: 0, byteOffset: 36, byteLength: 6 },
    ],
    accessors: [
      { bufferView: 0, componentType: 5126, count: 3, type: 'VEC3' },
      { bufferView: 1, componentType: 5123, count: 3, type: 'SCALAR' },
    ],
    buffers: [{ byteLength: 44 }],
  };
  const jsonRaw = Buffer.from(JSON.stringify(gltf), 'utf8');
  const jsonPad = (4 - (jsonRaw.length % 4)) % 4;
  const jsonChunk = Buffer.concat([jsonRaw, Buffer.alloc(jsonPad, 0x20)]);

  const binChunk = Buffer.alloc(44);
  const coords = [0, 0, 0, 2, 0, 0, 0, 3, 0];
  coords.forEach((v, i) => binChunk.writeFloatLE(v, i * 4));
  binChunk.writeUInt16LE(0, 36);
  binChunk.writeUInt16LE(1, 38);
  binChunk.writeUInt16LE(2, 40);

  const totalLen = 12 + 8 + jsonChunk.length + 8 + binChunk.length;
  const header = Buffer.alloc(12);
  header.writeUInt32LE(0x46546c67, 0);
  header.writeUInt32LE(2, 4);
  header.writeUInt32LE(totalLen, 8);

  const jsonHdr = Buffer.alloc(8);
  jsonHdr.writeUInt32LE(jsonChunk.length, 0);
  jsonHdr.writeUInt32LE(0x4e4f534a, 4);

  const binHdr = Buffer.alloc(8);
  binHdr.writeUInt32LE(binChunk.length, 0);
  binHdr.writeUInt32LE(0x004e4942, 4);

  return Buffer.concat([header, jsonHdr, jsonChunk, binHdr, binChunk]);
}

function buildSyntheticWav() {
  const sampleRate = 8000;
  const samples = 800; // 0.10 s
  const dataLen = samples * 2;
  const buf = Buffer.alloc(44 + dataLen);
  buf.write('RIFF', 0, 'ascii');
  buf.writeUInt32LE(36 + dataLen, 4);
  buf.write('WAVEfmt ', 8, 'ascii');
  buf.writeUInt32LE(16, 16);
  buf.writeUInt16LE(1, 20);
  buf.writeUInt16LE(1, 22);
  buf.writeUInt32LE(sampleRate, 24);
  buf.writeUInt32LE(sampleRate * 2, 28);
  buf.writeUInt16LE(2, 32);
  buf.writeUInt16LE(16, 34);
  buf.write('data', 36, 'ascii');
  buf.writeUInt32LE(dataLen, 40);
  for (let i = 0; i < samples; i++) {
    const amp = Math.round(Math.sin((i / 20) * Math.PI * 2) * 16384);
    buf.writeInt16LE(amp, 44 + i * 2);
  }
  return buf;
}

test('path helpers extract folders, basenames, compound extensions, rename targets, and sizes', () => {
  assert.equal(assetBasename('content/scenes/courtyard.scene.json'), 'courtyard.scene.json');
  assert.equal(assetFolder('content/scenes/courtyard.scene.json'), 'content/scenes');
  assert.equal(assetFolder('root.glb'), '');
  assert.equal(assetExtension('content/scenes/courtyard.scene.json'), '.scene.json');
  assert.equal(assetExtension('content/looks/day.look.json'), '.look.json');
  assert.equal(assetExtension('content/models/props/pillar.glb'), '.glb');
  assert.equal(assetStem('content/scenes/courtyard.scene.json'), 'courtyard');
  assert.equal(assetStem('content/prefabs/arch.prefab.json'), 'arch');
  assert.equal(assetStem('content/looks/day.look.json'), 'day');
  assert.equal(assetStem('content/models/props/pillar.glb'), 'pillar');
  assert.equal(projectAssetUrl('content/textures/stone albedo.png'), '/project/content/textures/stone%20albedo.png');
  assert.equal(inFolder('content/models/props', 'content/models'), true);
  assert.equal(inFolder('content/models_extra', 'content/models'), false);

  assert.equal(buildRenamedPath('content/scenes/courtyard.scene.json', 'plaza'), 'content/scenes/plaza.scene.json');
  assert.equal(buildRenamedPath('content/looks/day.look.json', 'sunset.look.json'), 'content/looks/sunset.look.json');
  assert.equal(buildRenamedPath('content/models/props/pillar.glb', 'content/props/column'), 'content/props/column.glb');
  assert.equal(buildRenamedPath('content/models/props/pillar.glb', 'content/archive/'), 'content/archive/pillar.glb');
  assert.equal(buildRenamedPath('content/models/props/pillar.glb', '   '), null);

  assert.equal(formatBytes(0), '0 B');
  assert.equal(formatBytes(512), '512 B');
  assert.equal(formatBytes(2048), '2.0 KB');
  assert.equal(formatBytes(2 * 1024 * 1024), '2.0 MB');
});

test('buildFolderTree compacts empty single-child wrapper folders and counts nested assets', () => {
  const tree = buildFolderTree(SAMPLE_ASSETS, ['content', 'packages/shared/kit']);
  assert.equal(tree.length, 2);
  const content = tree.find((n) => n.path === 'content');
  assert.equal(content.count, SAMPLE_ASSETS.length);
  const modelsNode = content.children.find((n) => n.path === 'content/models/props');
  assert.ok(modelsNode, 'empty content/models wrapper compacts into content/models/props');
  assert.equal(modelsNode.name, 'models/props');
  assert.equal(modelsNode.count, 2);

  const shared = tree.find((n) => n.path === 'packages/shared/kit');
  assert.ok(shared, 'multi-segment empty content root compacts cleanly');
  assert.equal(shared.name, 'packages/shared/kit');
  assert.equal(shared.count, 0);

  const preserved = buildFolderTree(SAMPLE_ASSETS, ['content'], 'content/models');
  const uncompacted = preserved[0].children.find((n) => n.path === 'content/models');
  assert.ok(uncompacted, 'selected intermediate folder stays addressable');
  assert.equal(uncompacted.children[0].path, 'content/models/props');
});

test('filterAssets and kindCounts support smart collections and sort modes', () => {
  const all = filterAssets(SAMPLE_ASSETS);
  assert.deepEqual(
    all.map((a) => a.id),
    ['scn_main', 'pfb_arch', 'mdl_crate', 'mdl_pillar', 'lok_day', 'mat_stone', 'tex_stone', 'snd_wind'],
  );

  const bySize = filterAssets(SAMPLE_ASSETS, { sort: 'size' });
  assert.equal(bySize[0].id, 'snd_wind');
  assert.equal(bySize[1].id, 'tex_stone');
  assert.equal(bySize.at(-1).id, 'mat_stone');

  const byName = filterAssets(SAMPLE_ASSETS, { sort: 'name' });
  assert.equal(byName[0].id, 'pfb_arch'); // arch.prefab.json

  const favs = filterAssets(SAMPLE_ASSETS, {
    collection: 'favorites',
    favorites: new Set(['tex_stone', 'mdl_pillar']),
  });
  assert.deepEqual(favs.map((a) => a.id), ['mdl_pillar', 'tex_stone']);

  const mru = filterAssets(SAMPLE_ASSETS, {
    collection: 'recent',
    recent: ['snd_wind', 'scn_main'],
  });
  assert.deepEqual(mru.map((a) => a.id), ['snd_wind', 'scn_main']);

  const unused = filterAssets(SAMPLE_ASSETS, {
    collection: 'unused',
    usageCounts: {
      scn_main: 1,
      pfb_arch: 2,
      mdl_pillar: 3,
      mdl_crate: 1,
      lok_day: 1,
      mat_stone: 0,
      tex_stone: 1,
    },
  });
  assert.deepEqual(unused.map((a) => a.id), ['mat_stone', 'snd_wind']);

  const countsInProps = kindCounts(SAMPLE_ASSETS, { folder: 'content/models' });
  assert.equal(countsInProps.all, 2);
  assert.equal(countsInProps.model, 2);
  assert.equal(countsInProps.texture, 0);
});

test('canPlaceAsset allows models and prefabs in editable scenes and blocks self-placement in prefabs', () => {
  const model = SAMPLE_ASSETS.find((a) => a.kind === 'model');
  const prefab = SAMPLE_ASSETS.find((a) => a.kind === 'prefab');
  const look = SAMPLE_ASSETS.find((a) => a.kind === 'look');

  assert.equal(canPlaceAsset(look, { id: 'scn_main', kind: 'scene' }).ok, false);
  assert.deepEqual(canPlaceAsset(model, null), { ok: false, reason: 'openSceneToPlace' });
  assert.deepEqual(canPlaceAsset(model, { id: 'scn_main', kind: 'scene', unavailable: true }), {
    ok: false,
    reason: 'openSceneToPlace',
  });
  assert.equal(canPlaceAsset(model, { id: 'scn_main', kind: 'scene' }).ok, true);
  assert.equal(canPlaceAsset(prefab, { id: 'scn_main', kind: 'scene' }).ok, true);
  assert.deepEqual(canPlaceAsset(prefab, { id: 'pfb_arch', kind: 'prefab' }), {
    ok: false,
    reason: 'cannotPlaceSelf',
  });
  assert.equal(canPlaceAsset(prefab, { id: 'pfb_other', kind: 'prefab' }).ok, true);
});

test('placementEntity builds clean model and prefab entities and omits zero translations', () => {
  const modelEnt = placementEntity(
    { id: 'mdl_pillar', kind: 'model', path: 'content/models/props/pillar.glb' },
    { localTranslation: [1.23456, 0, -3.5] },
  );
  assert.deepEqual(modelEnt, {
    name: 'pillar',
    components: {
      transform: { translation: [1.235, 0, -3.5] },
      mesh: { model: 'mdl_pillar' },
    },
  });

  const prefabChild = placementEntity(
    { id: 'pfb_arch', kind: 'prefab', path: 'content/prefabs/arch.prefab.json' },
    { parent: 'ent_root', localTranslation: [0, 0, 0] },
  );
  assert.deepEqual(prefabChild, {
    name: 'arch',
    parent: 'ent_root',
    prefab: { asset: 'pfb_arch' },
    components: { transform: {} },
  });
});

test('raycastDropPoint projects viewport coordinates onto the ground plane and offsets left/right', () => {
  const view = { target: [0, 0, 0], yaw: 0, pitch: 0.55, distance: 16, fov: 50 };
  const rect = { left: 100, top: 50, width: 800, height: 600 };
  const centre = raycastDropPoint(view, 500, 350, rect);
  assert.deepEqual(centre, [0, 0, 0]);

  const rightHit = raycastDropPoint(view, 740, 350, rect);
  const leftHit = raycastDropPoint(view, 260, 350, rect);
  assert.ok(rightHit[0] > 1, `right drop should land at +X, got ${rightHit}`);
  assert.ok(leftHit[0] < -1, `left drop should land at -X, got ${leftHit}`);
  assert.equal(rightHit[1], 0);
});

test('parseGlbPreview extracts world-transformed bounds, counts, and material-tinted triangles', () => {
  const glb = buildSyntheticGlb();
  const parsed = parseGlbPreview(glb);
  assert.ok(parsed);
  assert.equal(parsed.vertices, 3);
  assert.equal(parsed.triangles, 1);
  assert.equal(parsed.meshes, 1);
  assert.equal(parsed.materials, 1);
  assert.deepEqual(parsed.bounds, [[2, 1, -3], [4, 4, -3]]);
  assert.deepEqual(parsed.size, [2, 3, 0]);
  assert.equal(parsed.sampleTris.length, 1);
  assert.ok(Math.abs(parsed.sampleTris[0][9] - 0.9) < 1e-5);
});

test('extractWavPeaks parses PCM RIFF WAVE duration and normalized waveform peaks', () => {
  const wav = buildSyntheticWav();
  const parsed = extractWavPeaks(wav, 16);
  assert.ok(parsed);
  assert.ok(Math.abs(parsed.durationSec - 0.1) < 1e-6);
  assert.equal(parsed.sampleRate, 8000);
  assert.equal(parsed.channels, 1);
  assert.equal(parsed.peaks.length, 16);
  assert.ok(Math.max(...parsed.peaks) > 0.45 && Math.max(...parsed.peaks) <= 0.55);
});
