// Exercises the same native Look inspector opened from a scene and from an asset deep link.
'use strict';
const fs = require('node:fs');
const path = require('node:path');

async function meanLight(page) {
  const png = await page.locator('#viewport-canvas').screenshot();
  return page.evaluate(async (image) => {
    const bitmap = await createImageBitmap(await (await fetch('data:image/png;base64,' + image)).blob());
    const canvas = document.createElement('canvas');
    canvas.width = bitmap.width;
    canvas.height = bitmap.height;
    const ctx = canvas.getContext('2d');
    ctx.drawImage(bitmap, 0, 0);
    const data = ctx.getImageData(0, 0, canvas.width, canvas.height).data;
    let sum = 0;
    for (let i = 0; i < data.length; i += 4) sum += (data[i] + data[i + 1] + data[i + 2]) / 3;
    bitmap.close();
    return sum / (data.length / 4);
  }, png.toString('base64'));
}

module.exports = async function verifyLook(page, project, check) {
  const rel = 'content/looks/day.look.json';
  const file = path.join(project, rel);
  const original = JSON.parse(fs.readFileSync(file));
  const originalSidecar = fs.readFileSync(file + '.meta.json', 'utf8');
  const id = JSON.parse(originalSidecar).id;
  const records = await page.evaluate(() => {
    const s = window.__studio;
    window.testLookEngine = s.engine;
    return {scene: s.doc.id, selection: s.selection, env: Object.entries(s.doc.root.entities).find(([, e]) => e.components?.environment)[0]};
  });
  await page.locator(`#outliner-tree .node[data-id="${records.env}"]`).click();
  await page.locator('[data-key="edit-look"]').click();
  await page.locator('#look-section').selectOption('post');
  const set = async (n) => {
    const input = page.locator('#look-field-post-exposure');
    await input.fill(String(n));
    await input.press('Tab');
    await page.waitForFunction(([id, v]) => window.__studio.looks.get(id)?.root.post?.exposure === v, [id, n]);
  };
  await set(0.2);
  const dim = await meanLight(page);
  await set(3);
  const bright = await meanLight(page);
  check('Look exposure changes the rendered scene without replacing its engine', bright > dim + 4 &&
    await page.evaluate(() => window.__studio.engine === window.testLookEngine), {dim, bright});
  check('Look preview uses the open scene and does not write before Save',
    await page.evaluate(id => window.__studio.doc.id === id, records.scene) && !JSON.parse(fs.readFileSync(file)).post);
  await page.locator('#undo').click();
  await page.waitForFunction(id => window.__studio.looks.get(id).root.post.exposure === 0.2, id);
  await page.locator('#redo').click();
  await page.waitForFunction(id => window.__studio.looks.get(id).root.post.exposure === 3, id);
  await page.locator('#save').click();
  await page.waitForFunction(id => !window.__studio.looks.get(id).dirty, id);
  const stored = JSON.parse(fs.readFileSync(file));
  check('Look Save writes only the intended value and preserves its sidecar identity',
    JSON.stringify(stored.post) === '{"exposure":3}' && stored.about === original.about &&
    fs.readFileSync(file + '.meta.json', 'utf8') === originalSidecar && !('id' in stored));
  const url = new URL(page.url());
  url.searchParams.set('asset', rel);
  await page.goto(url.href);
  await page.waitForFunction(id => window.__studio.inspectedLook === id && window.__studio.looks.get(id)?.root.post?.exposure === 3 &&
      window.__studio.engine, id);
  check('a Look asset deep link reopens the saved asset with the scene still visible',
    await page.locator('#outliner-tree .node').count() > 0);
  // Return the sample to its original style, including absence of the optional post object.
  await page.locator('#undo').click();
  await page.waitForFunction(id => window.__studio.looks.get(id).root.post.exposure === 0.2, id);
  await page.locator('#undo').click();
  await page.waitForFunction(id => !window.__studio.looks.get(id).root.post, id);
  await page.locator('#save').click();
  await page.waitForFunction(id => !window.__studio.looks.get(id).dirty, id);
  await page.getByRole('button', {name: 'Back to scene', exact: true}).click();
  check('undo and save restore the original Look, then the scene inspector',
    !JSON.parse(fs.readFileSync(file)).post && await page.evaluate(() => window.__studio.inspectedLook === null));
  // Reopening a clean Look reads another tool's edits. Save must never force an
  // overwrite just because the action was triggered by a button click.
  await page.locator(`#assets-list [data-id="${id}"] button`).click();
  await page.locator('#look-section').selectOption('post');
  await set(0.75);
  const outside = {...original, about: 'Changed outside Studio'};
  fs.writeFileSync(file, JSON.stringify(outside));
  await Promise.all([
    page.waitForEvent('dialog').then(dialog => dialog.dismiss()),
    page.locator('#save').click(),
  ]);
  check('declining the conflict keeps the outside file and the local unsaved Look',
    JSON.parse(fs.readFileSync(file)).about === outside.about && await page.evaluate(id => window.__studio.looks.get(id).dirty, id));
  await page.locator('#undo').click();
  await page.waitForFunction(id => !window.__studio.looks.get(id).dirty, id);
  await page.locator(`#assets-list [data-id="${id}"] button`).click();
  await page.waitForFunction(([id, text]) => window.__studio.looks.get(id).root.about === text, [id, outside.about]);
  check('reopening the same Look reads the outside edit into the inspector session', true);
  fs.writeFileSync(file, JSON.stringify(original));
  await page.locator(`#assets-list [data-id="${id}"] button`).click();
  await page.waitForFunction(([id, text]) => window.__studio.looks.get(id).root.about === text, [id, original.about]);
  const newPath = 'content/looks/evening.look.json';
  page.once('dialog', dialog => dialog.accept(newPath));
  await page.locator('#new-look').click();
  await page.waitForFunction(p => window.__studio.looks.get(window.__studio.inspectedLook)?.path === p, newPath);
  const created = JSON.parse(fs.readFileSync(path.join(project, newPath + '.meta.json')));
  check('New Look creates a registered editable style and preserves the selected scene',
    created.kind === 'look' && await page.evaluate(([scene, id]) => window.__studio.doc.id === scene && window.__studio.inspectedLook === id, [records.scene, created.id]));
  await page.getByRole('button', {name: 'Back to scene', exact: true}).click();
  await page.locator(`#outliner-tree .node[data-id="${records.env}"]`).click();
  await page.locator('[data-key="environment.look"] select').selectOption(created.id);
  await page.waitForFunction(([env, look]) => window.__studio.doc.root.entities[env].components.environment.look === look, [records.env, created.id]);
  await page.locator('#save').click();
  await page.waitForFunction(() => !window.__studio.doc.dirty);
  const scenePath = await page.evaluate(() => window.__studio.doc.path);
  check('the new Look can be assigned and saved by its stable id in a real scene',
    JSON.parse(fs.readFileSync(path.join(project, scenePath))).entities[records.env].components.environment.look === created.id);
  await page.locator('#undo').click();
  await page.waitForFunction(([env, look]) => window.__studio.doc.root.entities[env].components.environment.look === look, [records.env, id]);
  await page.locator('#save').click();
  await page.waitForFunction(() => !window.__studio.doc.dirty);
  // The following scene reloads should open the default scene, as a normal project launch would.
  url.searchParams.delete('asset');
  await page.evaluate(path => history.replaceState(null, '', path), url.pathname + url.search + url.hash);
  await page.evaluate(id => window.__studioView.select(id), records.selection);
};
