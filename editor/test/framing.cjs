'use strict';

module.exports = async function checkViewportFrames(page, check) {
  const originalSize = page.viewportSize();
  const metrics = () => page.evaluate(() => {
    const rect = document.querySelector('#viewport').getBoundingClientRect();
    const canvas = document.querySelector('#viewport-canvas');
    const guide = document.querySelector('#frame-guide').getBoundingClientRect();
    return {
      rect: { left: rect.left, top: rect.top, width: rect.width, height: rect.height },
      canvasRatio: canvas ? canvas.width / canvas.height : 0,
      guide: { left: guide.left, top: guide.top },
      fits: document.documentElement.scrollWidth <= window.innerWidth + 1,
      rev: window.__studio.doc.rev,
    };
  });
  const rev = (await metrics()).rev;
  await page.locator('.frame-options summary').click();
  for (const [shape, ratio, size] of [
    ['portrait', 9 / 16, {width: 1040, height: 650}],
    ['landscape', 16 / 9, {width: 390, height: 820}],
    ['classic', 4 / 3, originalSize],
  ]) {
    await page.setViewportSize(size);
    await page.locator('#frame-shape').selectOption(shape);
    await page.evaluate(() => new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r))));
    const m = await metrics();
    check(`the ${shape} preview renders at the selected aspect without clipping the editor`,
      m.rect.height > 40 && Math.abs(m.rect.width / m.rect.height - ratio) < 0.01 &&
      Math.abs(m.canvasRatio - ratio) < 0.02 &&
      m.rect.left >= 0 && m.rect.left + m.rect.width <= size.width + 1 && m.fits, m);
  }
  await page.locator('#show-frame-guide').check();
  await page.locator('#frame-margin').fill('12');
  const m = await metrics();
  check('the guide is measured from the rendered frame and leaves the document alone',
    Math.abs(m.guide.left - m.rect.left - 0.12*m.rect.width) <= 1 &&
    Math.abs(m.guide.top - m.rect.top - 0.12*m.rect.height) <= 1 && m.rev === rev, m);
  await page.locator('#show-frame-guide').uncheck();
  await page.locator('#frame-shape').selectOption('free');
  await page.locator('.frame-options summary').click();
};
