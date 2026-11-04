// Exercises the Asset Library Content Browser: folder navigation, search, kind pills,
// Grid/List views, texture/sound previews, model placement (+ Place and viewport/outliner drag),
// and prefab creation + placement.
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
  const samples = 64;
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
  return buf;
}

module.exports = async function verifyAssetLibrary(page, project, check) {
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

  // 1. Texture thumbnail decodes in Grid view and WAV is served with audio/wav.
  await page.waitForFunction(() => {
    const img = document.querySelector('#assets-list [data-kind="texture"] .asset-thumb img');
    return img && img.complete && img.naturalWidth === 2;
  });
  const wavType = await page.evaluate(async (rel) => {
    const res = await fetch('/project/' + rel);
    return res.headers.get('content-type');
  }, wavRel);
  check('texture thumbnails render in the Content Browser and sound files carry audio MIME', wavType === 'audio/wav', wavType);

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

  // 3. Place a model via `+ Place`, verify cook + instance draw, then undo.
  // Exit followed camera view first so orbit target is deterministic.
  await page.locator('#frame').click();
  const sceneId = await page.evaluate(() => window.__studio.doc.id);
  const modelId = await page.evaluate(() => window.__studio.assets.find((a) => a.kind === 'model').id);
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

  // 4. Drag a model card into the viewport at an offset position, then undo.
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

  // 5. Create a prefab (+ Prefab), verify self-placement guard & single-root parenting, save, and place in scene.
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
