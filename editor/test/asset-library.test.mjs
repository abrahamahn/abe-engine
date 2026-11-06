import assert from 'node:assert/strict';
import test from 'node:test';
import {
  assetBasename,
  assetExtension,
  assetFolder,
  assetStem,
  buildFolderTree,
  buildRenamedPath,
  buildSemanticCatalogFromAssets,
  canPlaceAsset,
  computeRelationalDropPoint,
  extractSceneSemanticNodes,
  filterAssets,
  filterIntakeItems,
  formatBytes,
  inferAssetSemanticAffordance,
  inFolder,
  kindCounts,
  liftPositionToSemanticEdges,
  placementEntity,
  projectAssetDropPosition,
  projectAssetUrl,
  raycastDropPoint,
  resolveSemanticAssetDrop,
} from '../asset-library.js';
import { createPreviewManager, extractWavPeaks, parseGlbPreview } from '../asset-previews.js';

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
  assert.equal(parsed.bones, 0);
  assert.ok(parsed.estimatedVramBytes >= 3 * 48 + 12);
  assert.equal(parsed.fullMesh.positions.length, 9);
  assert.equal(parsed.fullMesh.normals.length, 9);
  assert.equal(parsed.fullMesh.indices.length, 3);
  assert.equal(parsed.nodeHierarchy.length, 1);
});

test('filterIntakeItems filters external art queue items by kind and query tokens', () => {
  const intake = [
    { intakeIndex: 0, rootLabel: 'ganbate-art', intakePath: 'props/column.glb', name: 'column.glb', kind: 'model', bytes: 4096 },
    { intakeIndex: 0, rootLabel: 'ganbate-art', intakePath: 'surfaces/granite.png', name: 'granite.png', kind: 'texture', bytes: 8192 },
    { intakeIndex: 1, rootLabel: 'tripo3d-queue', intakePath: 'statue_raw.glb', name: 'statue_raw.glb', kind: 'model', bytes: 16384 },
  ];
  assert.equal(filterIntakeItems(intake, { kind: 'model' }).length, 2);
  assert.deepEqual(
    filterIntakeItems(intake, { kind: 'model', query: 'tripo statue' }).map((i) => i.name),
    ['statue_raw.glb'],
  );
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

test('parseGlbPreview resolves EXT_texture_webp, alphaCutoff, skips zero-scale nodes, and defers >20MB auto-loads', async () => {
  const gltf = {
    asset: { version: '2.0' },
    extensionsUsed: ['EXT_texture_webp'],
    scene: 0,
    scenes: [{ nodes: [0, 1] }],
    nodes: [
      { name: 'visible_body', mesh: 0, scale: [1, 1, 1] },
      { name: 'hidden_attachment', mesh: 0, scale: [0, 0, 0], translation: [100, 100, 100] },
    ],
    images: [{ bufferView: 2, mimeType: 'image/webp' }],
    textures: [{ extensions: { EXT_texture_webp: { source: 0 } } }],
    materials: [
      {
        alphaMode: 'MASK',
        alphaCutoff: 0.42,
        pbrMetallicRoughness: {
          baseColorFactor: [1, 1, 1, 1],
          baseColorTexture: { index: 0 },
        },
      },
    ],
    meshes: [{ primitives: [{ attributes: { POSITION: 0 }, indices: 1, material: 0 }] }],
    bufferViews: [
      { buffer: 0, byteOffset: 0, byteLength: 36 },
      { buffer: 0, byteOffset: 36, byteLength: 6 },
      { buffer: 0, byteOffset: 44, byteLength: 16 },
    ],
    accessors: [
      { bufferView: 0, componentType: 5126, count: 3, type: 'VEC3' },
      { bufferView: 1, componentType: 5123, count: 3, type: 'SCALAR' },
    ],
    buffers: [{ byteLength: 60 }],
  };
  const jsonRaw = Buffer.from(JSON.stringify(gltf), 'utf8');
  const jsonPad = (4 - (jsonRaw.length % 4)) % 4;
  const jsonChunk = Buffer.concat([jsonRaw, Buffer.alloc(jsonPad, 0x20)]);
  const binChunk = Buffer.alloc(60);
  [0, 0, 0, 1, 0, 0, 0, 2, 0].forEach((v, i) => binChunk.writeFloatLE(v, i * 4));
  binChunk.writeUInt16LE(0, 36);
  binChunk.writeUInt16LE(1, 38);
  binChunk.writeUInt16LE(2, 40);
  binChunk.write('RIFF\x08\x00\x00\x00WEBP', 44, 'binary');

  const totalLen = 12 + 8 + jsonChunk.length + 8 + binChunk.length;
  const hdr = Buffer.alloc(12);
  hdr.writeUInt32LE(0x46546c67, 0);
  hdr.writeUInt32LE(2, 4);
  hdr.writeUInt32LE(totalLen, 8);
  const jh = Buffer.alloc(8);
  jh.writeUInt32LE(jsonChunk.length, 0);
  jh.writeUInt32LE(0x4e4f534a, 4);
  const bh = Buffer.alloc(8);
  bh.writeUInt32LE(binChunk.length, 0);
  bh.writeUInt32LE(0x004e4942, 4);

  const parsed = parseGlbPreview(Buffer.concat([hdr, jh, jsonChunk, bh, binChunk]));
  assert.ok(parsed);
  assert.equal(parsed.triangles, 1, 'zero-scale hidden node is skipped');
  assert.deepEqual(parsed.bounds, [[0, 0, 0], [1, 2, 0]], 'hidden node at [100,100,100] does not pollute bounds');
  assert.equal(parsed.submeshes[0].imageIndex, 0, 'EXT_texture_webp source resolves to image 0');
  assert.ok(Math.abs(parsed.submeshes[0].alphaCutoff - 0.42) < 1e-6);

  let fetchCalls = 0;
  const mgr = createPreviewManager({
    urlForAsset: (p) => `/project/${p}`,
  });
  const deferred = await mgr.loadEntry({
    id: 'huge_env',
    kind: 'model',
    path: 'content/world/district.glb',
    bytes: 50 * 1024 * 1024,
  });
  assert.equal(deferred.deferredLargeModel, true);
  assert.equal(fetchCalls, 0);
});

test('inferAssetSemanticAffordance and buildSemanticCatalogFromAssets synthesize ganbate.semantic.node.v2 descriptors and resolve EN/KO queries', () => {
  const testAssets = [
    { id: 'mdl_table', kind: 'model', path: 'content/models/props/table.glb' },
    { id: 'mdl_vessel', kind: 'model', path: 'content/models/props/vessel.glb' },
    { id: 'mdl_chair', kind: 'model', path: 'content/models/props/chair.glb' },
    { id: 'mdl_lantern', kind: 'model', path: 'content/models/props/lantern.glb' },
    { id: 'chr_rei', kind: 'character', path: 'content/characters/rei-d30-rigged.character.json' },
    { id: 'mdl_pavilion', kind: 'model', path: 'content/models/world/yozakura-pavilion.glb' },
  ];

  const tableDesc = inferAssetSemanticAffordance(testAssets[0]);
  assert.equal(tableDesc.schema, 'ganbate.semantic.node.v2');
  assert.equal(tableDesc.role, 'support_surface');
  assert.equal(tableDesc.affordances.supportsOnTopOf, true);
  assert.equal(tableDesc.affordances.domain, 'floor');
  assert.equal(tableDesc.bounds.surfaceY, 0.9);

  const vesselDesc = inferAssetSemanticAffordance(testAssets[1], {
    bounds: [[-0.12, 0, -0.12], [0.12, 0.28, 0.12]],
  });
  assert.equal(vesselDesc.role, 'surface_prop');
  assert.equal(vesselDesc.affordances.domain, 'table_surface');
  assert.equal(vesselDesc.affordances.defaultRelation, 'on_top_of');
  assert.deepEqual(vesselDesc.bounds.halfExtents, [0.12, 0.14, 0.12]);
  assert.equal(vesselDesc.bounds.surfaceY, 0.28);

  const catalog = buildSemanticCatalogFromAssets(testAssets);
  assert.equal(catalog.schema, 'ganbate.semantic.catalog.v2');
  assert.equal(catalog.entries.length, 6);
  assert.equal(catalog.resolveQuery('테이블')?.assetId, 'mdl_table');
  assert.equal(catalog.resolveQuery('의자')?.assetId, 'mdl_chair');
  assert.equal(catalog.resolveQuery('등불')?.assetId, 'mdl_lantern');
  assert.equal(catalog.resolveQuery('캐릭터')?.assetId, 'chr_rei');
  assert.equal(catalog.resolveQuery('yozakura-pavilion')?.role, 'architectural_prop');
});

test('resolveSemanticAssetDrop snaps surface props onto support_surface topY and lifts on_top_of anchor edges', () => {
  const tableAsset = { id: 'mdl_table', kind: 'model', path: 'content/models/props/table.glb' };
  const vesselAsset = { id: 'mdl_vessel', kind: 'model', path: 'content/models/props/vessel.glb' };

  const tableNode = inferAssetSemanticAffordance(tableAsset, {
    id: 'table_1',
    position: [0, 0, 0],
  });

  // Drop vessel near the east side of the table
  const dropResult = resolveSemanticAssetDrop(
    vesselAsset,
    [0.75, 0.0, 0.05],
    { nodes: [tableNode] },
  );

  assert.equal(dropResult.projection.mode, 'surface_snap');
  assert.equal(dropResult.projection.supportTargetId, 'table_1');
  assert.equal(dropResult.projection.anchor, 'east');
  assert.equal(dropResult.node.position[1], 0.9, 'snaps Y to table surfaceY (0.9m)');
  assert.equal(dropResult.node.relation, 'on_top_of');
  assert.deepEqual(dropResult.edges, [
    { from: 'vessel', rel: 'on_top_of', to: 'table_1', anchor: 'east' },
  ]);
  assert.deepEqual(dropResult.entity.components.transform.translation, [0.75, 0.9, 0.05]);
});

test('resolveSemanticAssetDrop solves Minimal-Perturbation Constraint Projection (x* = argmin ||x - xHat||^2) and lifts symbolic + facing edges', () => {
  const tableAsset = { id: 'mdl_table', kind: 'model', path: 'content/models/props/table.glb' };
  const chairAsset = { id: 'mdl_chair', kind: 'model', path: 'content/models/props/chair.glb' };

  const tableNode = inferAssetSemanticAffordance(tableAsset, {
    id: 'table_main',
    position: [0, 0, 0],
  });

  // Drop chair clipping inside the left (-X) side of the table at [-0.95, 0, 0.1].
  // Table halfExtents = [1.3, 0.45, 0.85], chair halfExtents = [0.32, 0.45, 0.32], clearance = 0.08 -> expandX = 1.70
  const drop = resolveSemanticAssetDrop(
    chairAsset,
    [-0.95, 0, 0.1],
    { nodes: [tableNode] },
  );

  assert.equal(drop.projection.projected, true);
  assert.equal(drop.projection.mode, 'aabb_minimal_perturbation');
  assert.equal(drop.projection.violatedTargetId, 'table_main');
  assert.deepEqual(drop.projection.proposedPosition, [-0.95, 0, 0.1]);
  assert.deepEqual(drop.projection.position, [-1.7, 0, 0.1]);
  assert.ok(drop.projection.ghost, 'emits hallucination/conflict ghost vector from xHat to xStar');
  assert.deepEqual(drop.projection.ghost.from, [-0.95, 0, 0.1]);
  assert.deepEqual(drop.projection.ghost.to, [-1.7, 0, 0.1]);

  // Inverse Semantic Lifting: chair at [-1.7, 0, 0.1] is left_of table_main and oriented facing table_main
  assert.equal(drop.node.relation, 'left_of');
  assert.equal(drop.edges.some((e) => e.rel === 'left_of' && e.to === 'table_main'), true);
  assert.equal(drop.edges.some((e) => e.rel === 'facing' && e.to === 'table_main'), true);
  assert.ok(Array.isArray(drop.entity.components.transform.rotation), 'chair entity receives facing quaternion rotation');

  // Verify ECS doc compatibility via placementEntity(..., { sceneContext: doc })
  const ecsDoc = {
    root: {
      entities: {
        table_main: {
          name: 'table',
          components: {
            transform: { translation: [0, 0, 0] },
            mesh: { model: 'mdl_table' },
          },
        },
      },
    },
  };
  const entFromDoc = placementEntity(chairAsset, {
    localTranslation: [0.1, 0, 0.65],
    sceneContext: ecsDoc,
  });
  // Closest boundary from [0.1, 0, 0.65] is front (+Z): expandZ = 0.85 + 0.32 + 0.08 = 1.25
  assert.deepEqual(entFromDoc.components.transform.translation, [0.1, 0, 1.25]);
  assert.ok(Array.isArray(entFromDoc.components.transform.rotation));
});

test('filterAssets supports academic:* Tri-Modal collections, bilingual search tokens, and computeRelationalDropPoint', () => {
  const benchmarkAssets = [
    { id: 'mdl_table', kind: 'model', path: 'content/models/props/table.glb' },
    { id: 'mdl_chair', kind: 'model', path: 'content/models/props/chair.glb' },
    { id: 'mdl_lantern', kind: 'model', path: 'content/models/props/lantern.glb' },
    { id: 'chr_rei', kind: 'character', path: 'content/characters/rei-d30-rigged.character.json' },
    { id: 'mdl_pavilion', kind: 'model', path: 'content/models/world/yozakura-pavilion.glb' },
  ];

  assert.deepEqual(
    filterAssets(benchmarkAssets, { collection: 'academic:tabletop' }).map((a) => a.id),
    ['mdl_chair', 'mdl_table'],
  );
  assert.deepEqual(
    filterAssets(benchmarkAssets, { collection: 'academic:characters' }).map((a) => a.id),
    ['chr_rei'],
  );
  assert.deepEqual(
    filterAssets(benchmarkAssets, { collection: 'academic:lighting' }).map((a) => a.id),
    ['mdl_lantern'],
  );
  assert.deepEqual(
    filterAssets(benchmarkAssets, { collection: 'academic:architecture' }).map((a) => a.id),
    ['mdl_pavilion'],
  );
  assert.deepEqual(
    filterAssets(benchmarkAssets, { query: '의자' }).map((a) => a.id),
    ['mdl_chair'],
  );

  const tableNode = inferAssetSemanticAffordance(benchmarkAssets[0], {
    id: 'table_1',
    position: [0, 0, 0],
  });
  const chairDesc = inferAssetSemanticAffordance(benchmarkAssets[1]);
  const behindPt = computeRelationalDropPoint('behind', chairDesc, { nodes: [tableNode] });
  assert.equal(behindPt[0], 0);
  assert.ok(behindPt[2] < -1.2, `behind drop point should be at -Z outside table, got ${behindPt}`);
});
