// Exercises the Asset Library Content Browser: folder navigation, smart collections (Favorites,
// Recent, Unused), live 3D GLB / Look / Waveform previews, direct file import, non-destructive
// rename & drag-to-folder move, "Where Used" reference jump, context menu, sort/scale controls,
// model placement (+ Place and viewport drag), and prefab creation + placement.
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const zlib = require('node:zlib');

function crc32(buf) {
  let c = 0xffffffff;
  for (let i = 0; i < buf.length; i++) {
    c ^= buf[i];
    for (let k = 0; k < 8; k++) c = (c >>> 1) ^ (c & 1 ? 0xedb88320 : 0);
  }
  return (c ^ 0xffffffff) >>> 0;
}

function pngChunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length, 0);
  const tag = Buffer.from(type, 'ascii');
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(Buffer.concat([tag, data])), 0);
  return Buffer.concat([len, tag, data, crc]);
}

function syntheticPng2x2() {
  const sig = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(2, 0);
  ihdr.writeUInt32BE(2, 4);
  ihdr[8] = 8; // 8-bit
  ihdr[9] = 6; // RGBA
  const raw = Buffer.from([
    0, 232, 180, 74, 255, 42, 45, 52, 255,
    0, 42, 45, 52, 255, 232, 180, 74, 255,
  ]);
  return Buffer.concat([
    sig,
    pngChunk('IHDR', ihdr),
    pngChunk('IDAT', zlib.deflateSync(raw)),
    pngChunk('IEND', Buffer.alloc(0)),
  ]);
}

function syntheticWavTone() {
  const samples = 128;
  const dataSize = samples * 2;
  const buf = Buffer.alloc(44 + dataSize);
  buf.write('RIFF', 0, 'ascii');
  buf.writeUInt32LE(36 + dataSize, 4);
  buf.write('WAVEfmt ', 8, 'ascii');
  buf.writeUInt32LE(16, 16);
  buf.writeUInt16LE(1, 20); // PCM
  buf.writeUInt16LE(1, 22); // mono
  buf.writeUInt32LE(8000, 24);
  buf.writeUInt32LE(16000, 28);
  buf.writeUInt16LE(2, 32);
  buf.writeUInt16LE(16, 34);
  buf.write('data', 36, 'ascii');
  buf.writeUInt32LE(dataSize, 40);
  for (let i = 0; i < samples; i++) {
    buf.writeInt16LE(Math.round(Math.sin((i / 16) * Math.PI * 2) * 18000), 44 + i * 2);
  }
  return buf;
}

module.exports = async function verifyAssetLibrary(page, project, check, outDir = null) {
  const texRel = 'content/textures/checker.png';
  const wavRel = 'content/audio/tone.wav';
  fs.mkdirSync(path.join(project, 'content', 'textures'), { recursive: true });
  fs.mkdirSync(path.join(project, 'content', 'audio'), { recursive: true });
  fs.writeFileSync(path.join(project, texRel), syntheticPng2x2());
  fs.writeFileSync(path.join(project, wavRel), syntheticWavTone());

  const prevReport = await page.evaluateHandle(() => window.__studio.scanReport);
  await page.locator('#rescan-assets').click();
  await page.waitForFunction(
    (old) => window.__studio.scanReport && window.__studio.scanReport !== old,
    prevReport,
  );
  await prevReport.dispose();

  // 1. Live previews: Texture <img>, 3D GLB <canvas>, Look sky <canvas>, and WAV waveform <canvas>.
  await page.waitForFunction(() => {
    const img = document.querySelector('#assets-list [data-kind="texture"] .asset-thumb img');
    const glbCanvas = document.querySelector('#assets-list [data-kind="model"] .asset-thumb canvas.asset-preview-canvas');
    const wavCanvas = document.querySelector('#assets-list [data-kind="sound"] .asset-thumb canvas.asset-preview-canvas');
    const lookCanvas = document.querySelector('#assets-list [data-kind="look"] .asset-thumb canvas.asset-preview-canvas');
    return img && img.complete && img.naturalWidth === 2
      && glbCanvas && glbCanvas.width > 0
      && wavCanvas && wavCanvas.width > 0
      && lookCanvas && lookCanvas.width > 0;
  });
  const wavType = await page.evaluate(async (rel) => {
    const res = await fetch('/project/' + rel);
    return res.headers.get('content-type');
  }, wavRel);
  const thumbHeights = await page.evaluate(() =>
    [...document.querySelectorAll('#assets-list .asset-thumb')].map((el) => Math.round(el.getBoundingClientRect().height)),
  );
  check('live previews render for 3D GLB models, textures, Looks, and WAV waveforms without vertical squishing',
    wavType === 'audio/wav' && thumbHeights.length >= 6 && thumbHeights.every((h) => h >= 60),
    { wavType, thumbHeights });

  // 2. Folder tree navigation, breadcrumbs, search, kind pills, and Grid/List view toggle.
  const totalCount = await page.evaluate(() => window.__studio.assets.length);
  await page.locator('#assets-folders [data-folder="content/models"]').click();
  const modelFolderCount = await page.locator('#assets-list [data-id]').count();
  const crumbsText = await page.locator('#assets-bar .assets-crumbs').innerText();
  check('selecting a folder scopes the asset browser and renders folder breadcrumbs',
    modelFolderCount > 0 && modelFolderCount < totalCount && crumbsText.includes('models'),
    { modelFolderCount, totalCount, crumbsText });
  await page.locator('#assets-bar .crumb').first().click();
  check('clicking the root breadcrumb restores all assets',
    (await page.locator('#assets-list [data-id]').count()) === totalCount);

  await page.locator('#assets-search').fill('checker');
  check('searching filters assets live',
    (await page.locator('#assets-list [data-id]').count()) === 1);
  await page.locator('#assets-search').press('Escape');
  check('Escape in the asset search clears the query',
    (await page.locator('#assets-list [data-id]').count()) === totalCount);

  await page.locator('#assets-kinds [data-kind-filter="texture"]').click();
  check('clicking a kind pill filters to that asset kind',
    (await page.locator('#assets-list [data-id]').count()) === 1);
  await page.locator('#assets-kinds [data-kind-filter="texture"]').click();
  check('clicking an active kind pill returns to all kinds',
    (await page.locator('#assets-list [data-id]').count()) === totalCount);

  await page.locator('#assets-view-list').click();
  const isList = await page.evaluate(() => document.getElementById('assets-list').classList.contains('view-list'));
  await page.locator('#assets-view-grid').click();
  const isGrid = await page.evaluate(() => document.getElementById('assets-list').classList.contains('view-grid'));
  check('Grid and List view buttons switch Content Browser layouts', isList && isGrid);

  // 3. Sort selector & Card Scale slider.
  await page.locator('#assets-sort').selectOption('size');
  const sortedSizes = await page.evaluate(() => {
    const byId = new Map(window.__studio.assets.map((a) => [a.id, a.bytes || 0]));
    return [...document.querySelectorAll('#assets-list [data-id]')].map((n) => byId.get(n.dataset.id) || 0);
  });
  const monotonicDesc = sortedSizes.every((s, i) => i === 0 || sortedSizes[i - 1] >= s);
  await page.locator('#assets-sort').selectOption('kind');
  await page.locator('#assets-scale').fill('184');
  await page.locator('#assets-scale').dispatchEvent('input');
  const cardScaleVar = await page.evaluate(() => document.documentElement.style.getPropertyValue('--asset-card-min'));
  await page.locator('#assets-scale').fill('152');
  await page.locator('#assets-scale').dispatchEvent('input');
  check('Sort by Size orders cards by byte length and Card Scale updates thumbnail grid sizing',
    monotonicDesc && cardScaleVar === '184px', { first: sortedSizes[0], last: sortedSizes.at(-1), cardScaleVar });

  // 4. Asset Details Inspector + "Where Used" Reference Explorer & Jump-to-Entity + Unused filter.
  const referencedModelId = await page.evaluate(() => {
    const ents = Object.values(window.__studio.doc.root.entities);
    for (const e of ents) {
      if (e.components?.mesh?.model) return e.components.mesh.model;
    }
    return window.__studio.assets.find((a) => a.kind === 'model').id;
  });
  await page.evaluate(() => window.__studioView.select(null));
  await page.locator(`#assets-list [data-id="${referencedModelId}"]`).click();
  await page.waitForFunction(() => document.querySelectorAll('#asset-where-used .where-used-item').length > 0);
  const detailsInfo = await page.evaluate(() => ({
    statsText: document.querySelector('#assets-inspector .asset-inspector-stats')?.innerText || '',
    refCount: document.querySelectorAll('#asset-where-used .where-used-item').length,
    firstRefEntity: document.querySelector('#asset-where-used .where-used-item')?.dataset.refEntity || '',
  }));
  await page.locator('#asset-where-used .where-used-item').first().click();
  await page.waitForFunction((eid) => window.__studio.selection === eid, detailsInfo.firstRefEntity);
  if (outDir) {
    await page.screenshot({ path: path.join(outDir, 'studio-assets.png') });
  }
  check('Asset Details displays 3D geometry stats and Where Used jumps directly to the referencing entity',
    detailsInfo.statsText.includes('tris') && detailsInfo.refCount >= 1 && !!detailsInfo.firstRefEntity,
    detailsInfo);

  await page.waitForFunction(() => window.__studio.assetLibrary.state.usageReady === true);
  await page.locator('#assets-folders [data-collection="unused"]').click();
  const unusedPaths = await page.evaluate(() => {
    const byId = new Map(window.__studio.assets.map((a) => [a.id, a.path]));
    return [...document.querySelectorAll('#assets-list [data-id]')].map((n) => byId.get(n.dataset.id));
  });
  check('Unused smart collection surfaces unreferenced assets while excluding active scene assets',
    unusedPaths.includes(texRel) && unusedPaths.includes(wavRel) &&
    !unusedPaths.includes('content/scenes/blocks.scene.json'),
    unusedPaths);
  await page.locator('#assets-folders [data-collection="all"]').click();

  // 5. Direct File Import (`#import-assets-input`) + Non-destructive Rename & Drag-to-Folder Move + Context Menu / Favorites.
  await page.locator('#import-assets-input').setInputFiles([
    { name: 'imported_tile.png', mimeType: 'image/png', buffer: syntheticPng2x2() },
  ]);
  await page.waitForFunction(() => window.__studio.assets.some((a) => a.path.endsWith('/imported_tile.png')));
  const importedAsset = await page.evaluate(() => window.__studio.assets.find((a) => a.path.endsWith('/imported_tile.png')));
  check('importing a file via + Import writes the asset and .meta.json sidecar live',
    !!importedAsset && fs.existsSync(path.join(project, importedAsset.path + '.meta.json')),
    importedAsset);

  page.once('dialog', (d) => d.accept('stone_tile'));
  await page.locator('#asset-detail-rename').click();
  await page.waitForFunction((id) => {
    const a = window.__studio.assets.find((x) => x.id === id);
    return a && a.path.endsWith('/stone_tile.png');
  }, importedAsset.id);

  await page.locator(`#assets-list [data-id="${importedAsset.id}"]`).dragTo(
    page.locator('#assets-folders [data-folder="content"]'),
  );
  await page.waitForFunction((id) => {
    const a = window.__studio.assets.find((x) => x.id === id);
    return a && a.path === 'content/stone_tile.png';
  }, importedAsset.id);
  check('renaming and dragging an asset to another folder preserves its asset ID and moves its sidecar',
    fs.existsSync(path.join(project, 'content', 'stone_tile.png')) &&
    fs.existsSync(path.join(project, 'content', 'stone_tile.png.meta.json')),
    importedAsset.id);

  await page.locator(`#assets-list [data-id="${importedAsset.id}"]`).click({ button: 'right' });
  const menuVisible = await page.evaluate(() => !document.getElementById('asset-context-menu').hidden);
  await page.locator('#asset-context-menu button', { hasText: 'Favorite' }).click();
  await page.locator('#assets-folders [data-collection="favorites"]').click();
  const favCount = await page.locator('#assets-list [data-id]').count();
  await page.locator('#assets-folders [data-collection="all"]').click();
  check('right-click context menu toggles Favorites and scopes the Favorites collection',
    menuVisible && favCount === 1);

  // 6. Place a model via `+ Place`, verify cook + instance draw, then undo.
  await page.locator('#frame').click();
  const sceneId = await page.evaluate(() => window.__studio.doc.id);
  const modelId = referencedModelId;
  const instBefore = await page.evaluate(() => window.__studio.inst.length);
  let rev = await page.evaluate(() => window.__studio.doc.rev);

  await page.locator(`#assets-list [data-id="${modelId}"] .asset-place`).click();
  await page.waitForFunction(
    (r) => window.__studio.doc.rev > r && window.__studio.cooked?.rev >= window.__studio.doc.rev && window.__studio.engine,
    rev,
  );
  const placedClick = await page.evaluate((mId) => {
    const s = window.__studio;
    const ent = s.doc.root.entities[s.selection];
    return {
      model: ent?.components?.mesh?.model,
      instAfter: s.inst.length,
    };
  }, modelId);
  check('clicking + Place adds a model entity and cooks its mesh into the viewport',
    placedClick.model === modelId && placedClick.instAfter > instBefore,
    { instBefore, ...placedClick });

  rev = await page.evaluate(() => window.__studio.doc.rev);
  await page.locator('#undo').click();
  await page.waitForFunction(
    (r) => window.__studio.doc.rev > r && window.__studio.cooked?.rev >= window.__studio.doc.rev && window.__studio.engine,
    rev,
  );

  // 7. Drag a model card into the viewport at an offset position, then undo.
  rev = await page.evaluate(() => window.__studio.doc.rev);
  const vpBox = await page.locator('#viewport').boundingBox();
  await page.locator(`#assets-list [data-id="${modelId}"]`).dragTo(page.locator('#viewport'), {
    targetPosition: { x: Math.round(vpBox.width * 0.72), y: Math.round(vpBox.height * 0.55) },
  });
  await page.waitForFunction((r) => window.__studio.doc.rev > r, rev);
  const dropped = await page.evaluate(() => {
    const s = window.__studio;
    const ent = s.doc.root.entities[s.selection];
    return { model: ent?.components?.mesh?.model, tr: ent?.components?.transform?.translation };
  });
  check('dragging a model into the viewport places it at the projected drop location',
    dropped.model === modelId && Array.isArray(dropped.tr) && dropped.tr.length === 3,
    dropped);

  rev = await page.evaluate(() => window.__studio.doc.rev);
  await page.locator('#undo').click();
  await page.waitForFunction((r) => window.__studio.doc.rev > r, rev);

  // 8. Create a prefab (+ Prefab), verify self-placement guard & single-root parenting, save, and place in scene.
  const prefabRel = 'content/prefabs/pedestal.prefab.json';
  page.once('dialog', (d) => d.accept(prefabRel));
  await page.locator('#new-prefab').click();
  await page.waitForFunction((p) => window.__studio.doc?.path === p, prefabRel);
  const prefabId = await page.evaluate(() => window.__studio.doc.id);

  const selfDisabled = await page.evaluate(
    (id) => document.querySelector(`#assets-list [data-id="${id}"] .asset-place`)?.classList.contains('disabled'),
    prefabId,
  );
  check('an open prefab disables placing itself inside itself', selfDisabled === true);

  rev = await page.evaluate(() => window.__studio.doc.rev);
  await page.locator(`#assets-list [data-id="${modelId}"] .asset-place`).click();
  await page.waitForFunction((r) => window.__studio.doc.rev > r, rev);
  const prefabRoots = await page.evaluate(() =>
    Object.values(window.__studio.doc.root.entities).filter((e) => !e.parent).length);
  check('placing a model inside a prefab parents it under the prefab single root', prefabRoots === 1, prefabRoots);

  await page.locator('#save').click();
  await page.waitForFunction(() => !window.__studio.doc.dirty);

  // Reopen the scene and place the new prefab into it.
  await page.locator(`#assets-list [data-id="${sceneId}"] button`).click();
  await page.waitForFunction(
    (id) => window.__studio.doc?.id === id && window.__studio.cooked?.doc === id && window.__studio.engine,
    sceneId,
  );
  const sceneInstBefore = await page.evaluate(() => window.__studio.inst.length);
  rev = await page.evaluate(() => window.__studio.doc.rev);
  await page.locator(`#assets-list [data-id="${prefabId}"] .asset-place`).click();
  await page.waitForFunction(
    (r) => window.__studio.doc.rev > r && window.__studio.cooked?.rev >= window.__studio.doc.rev && window.__studio.engine,
    rev,
  );
  const placedPrefab = await page.evaluate(() => {
    const s = window.__studio;
    const ent = s.doc.root.entities[s.selection];
    return { prefab: ent?.prefab?.asset, instAfter: s.inst.length };
  });
  check('placing a saved prefab into a scene instantiates and renders its hierarchy',
    placedPrefab.prefab === prefabId && placedPrefab.instAfter > sceneInstBefore,
    { sceneInstBefore, ...placedPrefab });

  // Undo the temporary prefab placement so the open scene returns clean for subsequent checks.
  rev = await page.evaluate(() => window.__studio.doc.rev);
  await page.locator('#undo').click();
  await page.waitForFunction(
    (r) => window.__studio.doc.rev > r && !window.__studio.doc.dirty && window.__studio.cooked?.rev >= window.__studio.doc.rev && window.__studio.engine,
    rev,
  );
};
