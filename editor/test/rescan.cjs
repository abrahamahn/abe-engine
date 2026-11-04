// The artist's external writes use a disposable sample project. Both viewers use the normal UI.
'use strict';
const fs = require('node:fs');
const path = require('node:path');

module.exports = async function verifyRescan(page, project, check) {
  const original = 'content/looks/day.look.json', moved = 'content/looks/moved/day.look.json';
  const copy = 'content/models/rescan-copy.glb', renamed = 'content/models/rescan-renamed.glb';
  const imported = 'content/scenes/rescan-import.scene.json', duplicate = 'content/looks/copy.look.json';
  const abs = p => path.join(project, p);
  const originalLook = fs.readFileSync(abs(original)), originalMeta = fs.readFileSync(abs(original + '.meta.json'));
  const id = JSON.parse(originalMeta).id;
  const oldModel = await page.evaluate(() => window.__studio.assets.find(a => a.kind === 'model').path);
  const wait = async (tab, predicate, argument) => {
    try { await tab.waitForFunction(predicate, argument, {polling: 100, timeout: 90_000}); }
    catch (error) {
      const state = await tab.evaluate(() => ({
        url: location.pathname + location.search, connected: window.__studio?.host?.open,
        doc: window.__studio?.doc?.path, engine: window.__studio?.engine?.api,
        watchedLooks: [...(window.__studio?.looks.keys() || [])], report: window.__studio?.scanReport,
        console: document.querySelector('#console-lines')?.innerText?.slice(-1500),
      })).catch(() => ({}));
      throw Error(error.message + '\n' + JSON.stringify(state));
    }
  };
  const observer = await page.context().browser().newContext({
    viewport: {width: 1000, height: 680}, storageState: await page.context().storageState(),
  });
  const other = await observer.newPage();
  const errors = [];
  other.on('pageerror', e => errors.push(String(e)));
  const scan = async tab => {
    const old = await tab.evaluateHandle(() => window.__studio.scanReport);
    try {
      await tab.locator('#rescan-assets').click();
      await wait(tab, previous => window.__studio.scanReport && window.__studio.scanReport !== previous, old);
    } finally { await old.dispose(); }
  };
  const cleanup = p => fs.rmSync(abs(p), {force: true});
  try {
    await other.goto(page.url());
    await wait(other, id => window.__studio?.host.open && window.__studio.looks.get(id)?.root, id);
    await page.locator(`#assets-list [data-id="${id}"] button`).click();
    await page.locator('#look-section').selectOption('post');
    const exposure = page.locator('#look-field-post-exposure');
    await exposure.fill('0.65');
    await exposure.press('Tab');
    await wait(other, id => window.__studio.looks.get(id)?.root.post?.exposure === 0.65, id);
    fs.mkdirSync(path.dirname(abs(moved)), {recursive: true});
    for (const suffix of ['', '.meta.json']) fs.renameSync(abs(original + suffix), abs(moved + suffix));
    fs.copyFileSync(abs(oldModel), abs(copy));
    await scan(other);
    for (const tab of [page, other]) {
      await wait(tab, ([id, p, copy]) => window.__studio.looks.get(id)?.path === p &&
        window.__studio.looks.get(id).dirty && window.__studio.assets.some(a => a.path === copy), [id, moved, copy]);
    }
    const first = await page.evaluate(() => window.__studio.scanReport);
    check('rescanning one viewer publishes moves and external additions to both without losing the Look draft',
      first.ok && first.moved.some(a => a.id === id && a.from === original && a.to === moved) && first.created.includes(copy));
    await page.locator('#save').click();
    await wait(page, id => !window.__studio.looks.get(id).dirty, id);
    check('a renamed Look saves its draft only at the new path and retains its existing id',
      !fs.existsSync(abs(original)) && JSON.parse(fs.readFileSync(abs(moved))).post.exposure === 0.65 &&
      JSON.parse(fs.readFileSync(abs(moved + '.meta.json'))).id === id);
    const model = JSON.parse(fs.readFileSync(abs(copy + '.meta.json'))).id;
    other.once('dialog', dialog => dialog.accept(imported));
    await other.locator(`#assets-list [data-id="${model}"] button`).click();
    await wait(other, p => window.__studio.doc?.path === p && window.__studio.cooked?.doc === window.__studio.doc.id &&
      window.__studio.inst.length > 0, imported);
    check('the new model imports as a renderable scene without restarting the host',
      JSON.stringify(JSON.parse(fs.readFileSync(abs(imported))).entities).includes(model));
    // Moving the source alone cannot preserve its id. The imported scene now has a broken mesh reference.
    fs.renameSync(abs(copy), abs(renamed));
    await scan(page);
    const broken = await page.evaluate(() => window.__studio.scanReport);
    const newId = JSON.parse(fs.readFileSync(abs(renamed + '.meta.json'))).id;
    check('a model without its sidecar gets a new id and both the orphaned sidecar and unresolved scene reference are explained',
      newId !== model && broken.problems.some(p => p.includes(copy + '.meta.json') && p.includes('missing')) &&
      broken.problems.some(p => p.includes(imported) && p.includes(model)));
    const before = await other.evaluate(() => JSON.stringify(window.__studio.assets));
    for (const suffix of ['', '.meta.json']) fs.copyFileSync(abs(moved + suffix), abs(duplicate + suffix));
    await scan(other);
    await wait(page, () => window.__studio.scanReport?.ok === false);
    check('duplicate identities keep the previous catalog in both sessions',
      await page.evaluate(list => JSON.stringify(window.__studio.assets) === list, before) &&
      await other.evaluate(list => JSON.stringify(window.__studio.assets) === list, before) &&
      (await other.locator('#asset-scan-report').innerText()).includes('same id'));
    for (const suffix of ['', '.meta.json']) cleanup(duplicate + suffix);
    await exposure.fill('1.25');
    await exposure.press('Tab');
    await wait(page, id => window.__studio.looks.get(id).root.post.exposure === 1.25, id);
    cleanup(moved); cleanup(moved + '.meta.json');
    await scan(other);
    await wait(page, id => window.__studio.looks.get(id)?.unavailable && window.__studio.looks.get(id).dirty, id);
    const download = page.waitForEvent('download');
    await page.locator('#download-draft').click();
    const file = await download;
    const recovered = JSON.parse(fs.readFileSync(await file.path()));
    check('deleting an open source preserves its editable draft, blocks Save, and allows local recovery',
      await page.locator('#save').isDisabled() && recovered.post.exposure === 1.25 && !fs.existsSync(abs(moved)));
    check('neither viewer raised a page error while rescanning', !errors.length, errors);
  } finally {
    await observer.close();
    for (const file of [moved, moved + '.meta.json', duplicate, duplicate + '.meta.json', copy, copy + '.meta.json',
      renamed, renamed + '.meta.json', imported]) cleanup(file);
    fs.writeFileSync(abs(original), originalLook);
    fs.writeFileSync(abs(original + '.meta.json'), originalMeta);
  }
  // The same id returns at its original path, with the draft and its undo history intact.
  await scan(page);
  await wait(page, ([id,p]) => !window.__studio.looks.get(id).unavailable && window.__studio.looks.get(id).path === p, [id, original]);
  await page.locator('#undo').click(); // 1.25 → the saved 0.65 (the source on disk is the original again)
  await wait(page, id => !window.__studio.looks.get(id).dirty, id);
  await scan(page);
  await wait(page, id => !window.__studio.looks.get(id).root.post, id);
  await page.getByRole('button', {name: 'Back to scene', exact: true}).click();
};
