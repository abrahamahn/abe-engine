'use strict';

// The renderer and sample scene are already open from the Studio run.
module.exports = async function checkCameraAuthoring(page, check) {
  const s = () => page.evaluate(() => {
    const studio = window.__studio;
    const camera = studio.doc.root.entities[studio.selection]?.components;
    return { id: studio.selection, pose: camera?.transform, fov: camera?.camera?.fov, rev: studio.doc.rev };
  });
  const waitRev = (rev) => page.waitForFunction(n => window.__studio.doc.rev > n, rev);
  const name = 'Courtyard view';
  const before = await s();
  page.once('dialog', dialog => dialog.accept(name));
  await page.locator('#bookmark-camera').click();
  await waitRev(before.rev);
  await page.waitForFunction(name => window.__studio.doc.root.entities[window.__studio.selection]?.name === name, name);
  const saved = await s();
  check('the viewport adds a named camera entity with its pose and lens', !!saved.pose?.translation && saved.fov > 0);

  const rect = await page.locator('#viewport').boundingBox();
  const x = rect.x + rect.width * 0.45, y = rect.y + rect.height * 0.4;
  await page.mouse.move(x, y);
  await page.mouse.down({ button: 'right' });
  await page.mouse.move(x + 64, y + 17, { steps: 4 });
  await page.mouse.up({ button: 'right' });
  await page.locator('#viewport-fov').fill('67');
  await page.locator('#viewport-fov').press('Tab');
  await page.locator('[data-key="camera#align"]').click();
  await waitRev(saved.rev);
  const aligned = await s();
  check('aligning a camera captures the moved viewport in one edit',
    aligned.fov === 67 && JSON.stringify(aligned.pose) !== JSON.stringify(saved.pose), { saved, aligned });
  await page.locator('#undo').click();
  await waitRev(aligned.rev);
  const undone = await s();
  check('undo restores the original camera transform and lens',
    JSON.stringify(undone.pose) === JSON.stringify(saved.pose) && undone.fov === saved.fov);
  await page.locator('#redo').click();
  await waitRev(undone.rev);
  await page.locator('[data-key="camera#look"]').click();
  const nearInput = page.locator('#inspector-body [data-key="camera.near"] input');
  let curRev = (await s()).rev;
  await nearInput.fill('0.24');
  await nearInput.press('Enter');
  await waitRev(curRev);
  curRev = (await s()).rev;
  const shiftX = page.locator('#inspector-body [data-key="camera.shift"] input').first();
  await shiftX.fill('-0.18');
  await shiftX.press('Enter');
  await waitRev(curRev);
  curRev = (await s()).rev;
  await page.locator('#inspector-body [data-key="camera.focus.enabled"] input').check();
  await waitRev(curRev);
  curRev = (await s()).rev;
  const focusDist = page.locator('#inspector-body [data-key="camera.focus.distance"] input');
  await focusDist.fill('2.7');
  await focusDist.press('Enter');
  await waitRev(curRev);
  const liveOptics = await page.evaluate(() => ({
    near: window.__studio.view.near,
    shift: window.__studio.view.shift,
    focus: window.__studio.view.focus,
  }));
  check('camera optical fields and focus subfields update the followed viewport live',
    liveOptics.near === 0.24 && liveOptics.shift?.[0] === -0.18 &&
    liveOptics.focus?.enabled === true && liveOptics.focus?.distance === 2.7, liveOptics);
  await page.locator('#save').click();
  await page.waitForFunction(() => !window.__studio.doc.dirty && !window.__studio.host.waiting);
  await page.reload();
  await page.waitForFunction(id => window.__studio?.engine && window.__studio?.doc?.root.entities[id], saved.id, { timeout: 120000 });
  await page.evaluate(id => window.__studioView.select(id), saved.id);
  await page.locator('[data-key="camera#look"]').click();
  const reopened = await s();
  const reopenedView = await page.evaluate(() => ({
    fov: window.__studio.view.fov,
    near: window.__studio.view.near,
    shift: window.__studio.view.shift,
    focus: window.__studio.view.focus,
  }));
  check('a saved camera can be looked through after reopening the project',
    JSON.stringify(reopened.pose) === JSON.stringify(aligned.pose) &&
    reopenedView.fov === aligned.fov && reopenedView.near === 0.24 &&
    reopenedView.shift?.[0] === -0.18 && reopenedView.focus?.distance === 2.7);
};
