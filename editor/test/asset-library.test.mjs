import assert from 'node:assert/strict';
import test from 'node:test';
import {
  assetBasename,
  assetFolder,
  assetStem,
  buildFolderTree,
  canPlaceAsset,
  filterAssets,
  inFolder,
  kindCounts,
  placementEntity,
  projectAssetUrl,
  raycastDropPoint,
} from '../asset-library.js';

const SAMPLE_ASSETS = [
  { id: 'scn_main', kind: 'scene', path: 'content/scenes/courtyard.scene.json' },
  { id: 'pfb_arch', kind: 'prefab', path: 'content/prefabs/arch.prefab.json' },
  { id: 'mdl_pillar', kind: 'model', path: 'content/models/props/pillar.glb' },
  { id: 'mdl_crate', kind: 'model', path: 'content/models/props/crate.glb' },
  { id: 'lok_day', kind: 'look', path: 'content/looks/day.look.json' },
  { id: 'mat_stone', kind: 'material', path: 'content/materials/stone.material.json' },
  { id: 'tex_stone', kind: 'texture', path: 'content/textures/stone albedo.png' },
  { id: 'snd_wind', kind: 'sound', path: 'content/audio/wind.ogg' },
];

test('path helpers extract folders, basenames, compound stems, and encoded project URLs', () => {
  assert.equal(assetBasename('content/scenes/courtyard.scene.json'), 'courtyard.scene.json');
  assert.equal(assetFolder('content/scenes/courtyard.scene.json'), 'content/scenes');
  assert.equal(assetFolder('root.glb'), '');
  assert.equal(assetStem('content/scenes/courtyard.scene.json'), 'courtyard');
  assert.equal(assetStem('content/prefabs/arch.prefab.json'), 'arch');
  assert.equal(assetStem('content/looks/day.look.json'), 'day');
  assert.equal(assetStem('content/models/props/pillar.glb'), 'pillar');
  assert.equal(projectAssetUrl('content/textures/stone albedo.png'), '/project/content/textures/stone%20albedo.png');
  assert.equal(inFolder('content/models/props', 'content/models'), true);
  assert.equal(inFolder('content/models_extra', 'content/models'), false);
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

test('filterAssets and kindCounts scope by folder, kind, and multi-token search in kind order', () => {
  const all = filterAssets(SAMPLE_ASSETS);
  assert.deepEqual(
    all.map((a) => a.id),
    ['scn_main', 'pfb_arch', 'mdl_crate', 'mdl_pillar', 'lok_day', 'mat_stone', 'tex_stone', 'snd_wind'],
  );

  const propsOnly = filterAssets(SAMPLE_ASSETS, { folder: 'content/models' });
  assert.deepEqual(propsOnly.map((a) => a.id), ['mdl_crate', 'mdl_pillar']);

  const stoneQuery = filterAssets(SAMPLE_ASSETS, { query: 'content stone' });
  assert.deepEqual(stoneQuery.map((a) => a.id), ['mat_stone', 'tex_stone']);

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
