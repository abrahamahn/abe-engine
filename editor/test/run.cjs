// End-to-end test of ABE Studio: the host, the browser interface and the engine viewport.
//
//   node editor/test/run.cjs --out OUT_DIR [--api webgl2] [--keep]
//
// Needs release builds of the sample writer, the host and the cooker, and the web runtime:
//   cargo build --release --example studio_sample -p abe-engine
//   cargo build --release -p abe-studio --bin abe-studio
//   cargo build --release --bin abe-build --bin abe-render
//   scripts/build-web.sh web/dist
//
// Between the rename and the new component it also checks what the keyboard does after a field
// is committed, undo and save behind edits the host has not answered yet, a scale set to 0 and
// back, two entities added at once, and two cooks that overlap; after reopening the scene it
// stops the host (a move made meanwhile snaps back) and starts it again (the page reconnects).
//
// It writes the synthetic sample project, starts `abe-studio` on it, opens the interface in
// headless Chromium and, through real pointer and key events: selects an entity by clicking
// it, drags it along X with the gizmo, undoes and redoes, moves a parent and checks its child
// follows, renames through the inspector, adds a component (which re-cooks the scene), saves,
// reloads the page and finds the edits, then cooks the saved scene with `abe-build cook-scene`
// and renders it with `abe-render`. OUT gets screenshots, the saved scene and results.json.
'use strict';
const fs = require('fs');
const path = require('path');
const os = require('os');
const { spawn, execFileSync } = require('child_process');
const { chromium } = require('playwright');

const ROOT = path.resolve(__dirname, '..', '..');
const BIN = path.join(ROOT, 'target', 'release');
const CHROMIUM_ARGS = ['--enable-unsafe-webgpu', '--enable-features=Vulkan', '--use-angle=vulkan',
  '--use-vulkan=swiftshader'];

function args() {
  const o = { out: 'editor-test-out', api: '', keep: false };
  const a = process.argv.slice(2);
  for (let i = 0; i < a.length; i++) {
    if (a[i] === '--out') o.out = a[++i];
    else if (a[i] === '--api') o.api = a[++i];
    else if (a[i] === '--keep') o.keep = true;
    else throw new Error(`unknown argument ${a[i]}`);
  }
  return o;
}

const checks = [];
function check(name, ok, detail) {
  checks.push({ name, ok: !!ok, detail });
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${name}${detail !== undefined ? ` ${JSON.stringify(detail)}` : ''}`);
}

/** Starts the host; its output goes on `lines`. Resolves with the process once it listens. */
function startHost(project, port, lines) {
  const host = spawn(path.join(BIN, 'abe-studio'), [project, '--port', String(port),
    '--editor', path.join(ROOT, 'editor'), '--engine', path.join(ROOT, 'web', 'dist')],
  { stdio: ['ignore', 'pipe', 'pipe'] });
  const from = lines.length;
  host.stdout.on('data', (d) => lines.push(String(d)));
  host.stderr.on('data', (d) => lines.push(String(d)));
  return new Promise((resolve, reject) => {
    const t0 = Date.now();
    const poll = setInterval(() => {
      if (lines.slice(from).join('').includes('at http://localhost')) {
        clearInterval(poll);
        resolve(host);
      } else if (Date.now() - t0 > 20000 || host.exitCode !== null) {
        clearInterval(poll);
        reject(new Error(`the host did not start: ${lines.slice(from).join('')}`));
      }
    }, 100);
  });
}

// The engine module as the page imports it, wrapped so that a test can hold a load back and
// see which engines were destroyed: `window.__engines` lists each createEngine call in order.
const ENGINE_WRAPPER = `
import * as real from '/engine/abe.js?real';
export * from '/engine/abe.js?real';
export async function createEngine(options) {
  const rec = { canvas: options.canvas, engine: null, destroyed: false };
  (window.__engines ||= []).push(rec);
  const delay = (window.__engineDelays || []).shift() || 0;
  const engine = await real.createEngine(options);
  await new Promise((r) => setTimeout(r, delay));
  const destroy = engine.destroy;
  engine.destroy = () => {
    rec.destroyed = true;
    destroy();
  };
  rec.engine = engine;
  return engine;
}
`;

async function waitFor(page, fn, arg, what, timeout = 120000) {
  try {
    await page.waitForFunction(fn, arg, { timeout, polling: 100 });
  } catch (e) {
    throw new Error(`timed out waiting for ${what}`);
  }
}

const entityId = (page, name) => page.evaluate((n) => {
  const ents = window.__studio.doc.root.entities;
  return Object.keys(ents).find((id) => ents[id].name === n);
}, name);

const translation = (page, id) => page.evaluate((i) => {
  const t = window.__studio.doc.root.entities[i].components?.transform?.translation;
  return t ? t.slice() : [0, 0, 0];
}, id);

/** The world matrix of an entity's first instance as the engine draws it. */
const instanceMatrix = (page, id) => page.evaluate((i) => {
  const s = window.__studio;
  const r = s.cooked.entities[s.records.get(i)];
  return s.engine.worldInstance(r.instances[0]).matrix;
}, id);

async function centreOf(page, id) {
  return page.evaluate((i) => {
    const b = window.__studioView.boundsOf(i);
    const c = [0, 1, 2].map((k) => (b[0][k] + b[1][k]) / 2);
    return window.__studioView.project(c);
  }, id);
}

async function viewportOrigin(page) {
  return page.evaluate(() => {
    const r = document.querySelector('#viewport').getBoundingClientRect();
    return { x: r.left, y: r.top };
  });
}

async function revision(page) {
  return page.evaluate(() => window.__studio.doc.rev);
}

/** Like waitFor, but resolves with whether `fn` came true instead of throwing. */
async function settle(page, fn, arg, timeout = 15000) {
  try {
    await page.waitForFunction(fn, arg, { timeout, polling: 100 });
    return true;
  } catch {
    return false;
  }
}

/** Holds the host's messages to the page back (`on`), as when the host is busy (it cooks
 * under the lock that edits take), or delivers those held, in order, and lets the rest through. */
const holdHost = (page, on) => page.evaluate((on) => {
  const ws = window.__studio.host.ws;
  ws.deliver ||= ws.onmessage;
  window.__held ||= [];
  if (on) {
    ws.onmessage = (e) => window.__held.push(e);
  } else {
    ws.onmessage = ws.deliver;
    for (const e of window.__held.splice(0)) ws.deliver(e);
  }
}, on);

/** Nothing waiting to be sent or answered. */
const quiet = () => !window.__studio.host.waiting && !window.__studio.host.queue.length;

/** Which control has the keyboard: `row key/control index` inside the inspector, else its id or tag. */
const focusOf = (page) => page.evaluate(() => {
  const a = document.activeElement;
  const row = a.closest('#inspector-body > *');
  return row ? `${row.dataset.key}/${[...row.querySelectorAll('input, select, button')].indexOf(a)}` : a.id || a.tagName;
});

/** An inspector field's inputs, by component and field name. */
const fieldInputs = (page, key) => page.locator(`#inspector-body [data-key="${key}"] input`);

/** Focuses the viewport without selecting anything (a right click starts an orbit). */
const focusViewport = (page) => page.locator('#viewport').click({ position: { x: 5, y: 5 }, button: 'right' });

/** Presses `keys` and waits for the document's revision to move. */
async function pressAndWait(page, keys, what) {
  const r = await revision(page);
  await page.keyboard.press(keys);
  await waitFor(page, (x) => window.__studio.doc.rev > x, r, what);
}

async function main() {
  const o = args();
  fs.mkdirSync(o.out, { recursive: true });
  const project = fs.mkdtempSync(path.join(os.tmpdir(), 'abe-studio-test-'));
  execFileSync(path.join(BIN, 'examples', 'studio_sample'), [project], { stdio: 'inherit' });
  const port = 7400 + Math.floor(Math.random() * 500);
  const lines = [];
  let host = await startHost(project, port, lines);
  const browser = await chromium.launch({ args: CHROMIUM_ARGS });
  console.log(`browser: Chromium ${browser.version()} (headless), api ${o.api || 'auto'}`);
  const page = await browser.newPage({ viewport: { width: 1400, height: 860 }, deviceScaleFactor: 1 });
  const logs = [];
  page.on('console', (m) => logs.push(`[${m.type()}] ${m.text()}`));
  page.on('pageerror', (e) => logs.push(`[pageerror] ${e.message}`));
  await page.route((u) => u.pathname === '/engine/abe.js' && !u.search,
    (route) => route.fulfill({ contentType: 'text/javascript', body: ENGINE_WRAPPER }));
  // While the host is deliberately down, the page's attempts to reconnect fail (log indices).
  const offline = [0, 0];
  const url = `http://localhost:${port}/${o.api ? `?api=${o.api}` : ''}`;
  let failed = null;
  try {
    await page.goto(url);
    await waitFor(page, () => window.__studio?.engine && window.__studio.cooked && window.__studio.doc, null, 'the viewport');
    // Let the first frames draw.
    await page.waitForTimeout(1500);
    await page.screenshot({ path: path.join(o.out, 'studio-open.png') });
    const names = await page.evaluate(() => Object.values(window.__studio.doc.root.entities).map((e) => e.name).sort());
    check('the scene opens with its entities', names.includes('Block A') && names.includes('Tower top'), names);
    check('the outliner lists them', (await page.locator('#outliner-tree .node').count()) === names.length);

    // 0. The interface speaks Korean on request (and back).
    await page.click('#language');
    check('the language button switches the interface to Korean', (await page.locator('#save').innerText()) === '저장'
      && (await page.locator('#inspector-body .hint').innerText()).includes('엔티티'));
    await page.click('#language');
    check('and back to English', (await page.locator('#save').innerText()) === 'Save');

    // 1. Click Block A in the viewport.
    const a = await entityId(page, 'Block A');
    const vp = await viewportOrigin(page);
    const at = await centreOf(page, a);
    await page.mouse.click(vp.x + at.x, vp.y + at.y);
    await waitFor(page, (id) => window.__studio.selection === id, a, 'the click to select Block A', 10000);
    check('a click in the viewport selects the entity under it', true);
    check('the inspector shows it', await page.locator('#inspector-body input[type=text]').first().inputValue() === 'Block A');

    // 2. Drag the X handle of the gizmo.
    const before = await translation(page, a);
    const g = await page.evaluate(() => {
      const z = window.__studioView.gizmo();
      return { o: z.po, x: z.ends[0] };
    });
    const dir = [g.x.x - g.o.x, g.x.y - g.o.y];
    const len = Math.hypot(dir[0], dir[1]);
    const rev0 = await revision(page);
    await page.mouse.move(vp.x + g.x.x, vp.y + g.x.y);
    await page.mouse.down();
    for (let i = 1; i <= 10; i++) {
      await page.mouse.move(vp.x + g.x.x + (dir[0] / len) * 9 * i, vp.y + g.x.y + (dir[1] / len) * 9 * i);
    }
    await page.mouse.up();
    await waitFor(page, (r) => window.__studio.doc.rev > r, rev0, 'the move edit');
    const moved = await translation(page, a);
    check('dragging the X handle moves the entity along X only',
      moved[0] > before[0] + 0.2 && Math.abs(moved[1] - before[1]) < 1e-4 && Math.abs(moved[2] - before[2]) < 1e-4,
      { before, moved });
    const m1 = await instanceMatrix(page, a);
    check('the engine draws it where the document has it', Math.abs(m1[12] - moved[0]) < 1e-3, m1.slice(12, 15));
    check('the document is dirty', await page.evaluate(() => window.__studio.doc.dirty));
    await page.screenshot({ path: path.join(o.out, 'studio-moved.png') });

    // 3. Undo and redo with the keyboard.
    await page.locator('#viewport').click({ position: { x: 5, y: 5 }, button: 'right' });
    let rev = await revision(page);
    await page.keyboard.press('Control+z');
    await waitFor(page, (r) => window.__studio.doc.rev > r, rev, 'undo');
    const undone = await translation(page, a);
    check('Ctrl+Z puts it back', undone.every((v, i) => Math.abs(v - before[i]) < 1e-5), undone);
    check('undo moves the drawing back', Math.abs((await instanceMatrix(page, a))[12] - before[0]) < 1e-3);
    check('after undoing everything it is clean', !(await page.evaluate(() => window.__studio.doc.dirty)));
    rev = await revision(page);
    await page.keyboard.press('Control+Shift+z');
    await waitFor(page, (r) => window.__studio.doc.rev > r, rev, 'redo');
    const redone = await translation(page, a);
    check('Ctrl+Shift+Z moves it again', redone.every((v, i) => Math.abs(v - moved[i]) < 1e-5), redone);

    // 4. A parent carries its child.
    const tower = await entityId(page, 'Tower');
    const top = await entityId(page, 'Tower top');
    await page.evaluate((id) => window.__studioView.select(id), tower);
    const topBefore = await instanceMatrix(page, top);
    const gy = await page.evaluate(() => {
      const z = window.__studioView.gizmo();
      return { o: z.po, y: z.ends[1] };
    });
    rev = await revision(page);
    await page.mouse.move(vp.x + gy.y.x, vp.y + gy.y.y);
    await page.mouse.down();
    for (let i = 1; i <= 8; i++) await page.mouse.move(vp.x + gy.y.x, vp.y + gy.y.y - 8 * i);
    await page.mouse.up();
    await waitFor(page, (r) => window.__studio.doc.rev > r, rev, 'the tower move');
    const topAfter = await instanceMatrix(page, top);
    const towerT = await translation(page, tower);
    check('moving a parent moves its child by the same amount',
      topAfter[13] - topBefore[13] > 0.2 && Math.abs((topAfter[13] - topBefore[13]) - (towerT[1] - 1)) < 1e-3,
      { child: topAfter[13] - topBefore[13], parent: towerT[1] - 1 });
    check('the child keeps its own transform in the file',
      (await translation(page, top)).every((v, i) => Math.abs(v - [0, 0.75, 0][i]) < 1e-6));

    // 5. Rename through the inspector.
    await page.evaluate((id) => window.__studioView.select(id), a);
    rev = await revision(page);
    const nameBox = page.locator('#inspector-body input[type=text]').first();
    await nameBox.fill('Block Alpha');
    await nameBox.press('Enter');
    await waitFor(page, (r) => window.__studio.doc.rev > r, rev, 'the rename');
    check('the inspector renames', await page.evaluate((id) => window.__studio.doc.root.entities[id].name, a) === 'Block Alpha');
    check('the outliner follows', (await page.locator('#outliner-tree .node.selected').innerText()).includes('Block Alpha'));

    // 5a. The keyboard stays in a committed field: Backspace edits it and deletes nothing. Delete
    // deletes only in the viewport or the outliner.
    const entityExists = (id) => page.evaluate((i) => !!window.__studio.doc.root.entities[i], id);
    check('after Enter the renamed field keeps the keyboard', (await focusOf(page)) === '#name/0', await focusOf(page));
    const tx = fieldInputs(page, 'transform.translation').first();
    rev = await revision(page);
    await tx.fill('3');
    await tx.press('Enter');
    await waitFor(page, (r) => window.__studio.doc.rev > r, rev, 'the translation edit');
    check('after Enter the committed number field keeps the keyboard',
      (await focusOf(page)) === 'transform.translation/0', await focusOf(page));
    rev = await revision(page);
    await page.keyboard.press('Backspace');
    await page.waitForTimeout(300);
    const afterBackspace = {
      exists: await entityExists(a),
      undo: await page.evaluate(() => window.__studio.doc.undo),
      field: await tx.inputValue({ timeout: 2000 }).catch(() => null),
    };
    check('Backspace then edits the field and deletes nothing',
      afterBackspace.exists && (await revision(page)) === rev && afterBackspace.field === '', afterBackspace);
    await focusViewport(page);
    await page.waitForTimeout(300);
    check('leaving an emptied number field commits nothing (not 0)',
      (await revision(page)) === rev && (await translation(page, a))[0] === 3 && (await tx.inputValue()) === '3',
      { translation: await translation(page, a), field: await tx.inputValue() });
    await pressAndWait(page, 'Control+z', 'undoing the translation');
    check('Ctrl+Z in the viewport undoes it', (await translation(page, a)).every((v, i) => Math.abs(v - moved[i]) < 1e-6));
    await page.focus('#inspector-body');
    await page.keyboard.press('Delete');
    await page.waitForTimeout(300);
    check('Delete with the inspector focused deletes nothing', await entityExists(a));
    await focusViewport(page);
    await pressAndWait(page, 'Delete', 'the delete');
    check('Delete in the viewport deletes the selection', !(await entityExists(a)));
    await pressAndWait(page, 'Control+z', 'undoing the delete');
    check('and Ctrl+Z brings it back', await entityExists(a));
    await page.evaluate((id) => window.__studioView.select(id), a);

    // 5b. A flattened scale (0) and back: the viewport draws it again (cooked anew).
    const sy = fieldInputs(page, 'transform.scale').nth(1);
    const scaleY = () => instanceMatrix(page, a).then((m) => Math.hypot(m[4], m[5], m[6]));
    // A re-cook of the delete's undo may still be loading.
    await waitFor(page, (r) => window.__studio.engine && window.__studio.cooked.rev >= r, await revision(page), 'the re-cook');
    for (const v of ['0', '1']) {
      rev = await revision(page);
      await sy.fill(v);
      await sy.press('Enter');
      await waitFor(page, (r) => window.__studio.doc.rev > r, rev, `scale y ${v}`);
    }
    const restored = await settle(page, (id) => {
      const s = window.__studio;
      const m = s.engine && s.records.has(id) && s.engine.worldInstance(s.cooked.entities[s.records.get(id)].instances[0]).matrix;
      return m && Math.abs(Math.hypot(m[4], m[5], m[6]) - 1) < 1e-4;
    }, a);
    check('a scale set to 0 and back to 1 is drawn at 1 again', restored, await scaleY().catch((e) => e.message));
    await focusViewport(page);
    await pressAndWait(page, 'Control+z', 'undoing scale y 1');
    await pressAndWait(page, 'Control+z', 'undoing scale y 0');
    await waitFor(page, (r) => window.__studio.engine && window.__studio.cooked.rev >= r, await revision(page), 'the re-cook');
    // Left at 0 on a rotated block (whose cooked matrix differs from the document's in the last
    // digits), a scale does not keep the viewport cooking.
    const b = await entityId(page, 'Block B');
    await page.evaluate((id) => window.__studioView.select(id), b);
    const syB = fieldInputs(page, 'transform.scale').nth(1);
    rev = await revision(page);
    await syB.fill('0');
    await syB.press('Enter');
    await waitFor(page, (r) => window.__studio.doc.rev > r, rev, 'scale y 0 of Block B');
    const gen0 = await page.evaluate(() => window.__studio.loadGen);
    await page.click('#recook');
    await waitFor(page, (g) => window.__studio.loadGen > g, gen0, 'the refresh');
    await page.waitForTimeout(3000);
    const loadsMade = (await page.evaluate(() => window.__studio.loadGen)) - gen0;
    check('a block left flattened does not keep the viewport cooking', loadsMade === 1, loadsMade);
    await focusViewport(page);
    await pressAndWait(page, 'Control+z', 'undoing scale y 0 of Block B');
    await waitFor(page, (r) => window.__studio.engine && window.__studio.cooked.rev >= r, await revision(page), 'the re-cook');

    // 5c. Undo after edits the host has not answered yet (it answers slowly, as while it cooks):
    // the undo waits for them, so it undoes the last one and none is refused.
    await page.evaluate((id) => window.__studioView.select(id), a);
    const nameNow = page.locator('#inspector-body input[type=text]').first();
    const consoleFrom = await page.evaluate(() => document.querySelector('#console-lines').childElementCount);
    await holdHost(page, true);
    await nameNow.fill('Block Beta');
    await nameNow.press('Enter');
    await tx.fill('5');
    await tx.press('Enter');
    await focusViewport(page);
    const unanswered = await page.evaluate(() => !!window.__studio.host.waiting && window.__studio.host.queue.length);
    await page.keyboard.press('Control+z');
    await page.waitForTimeout(300);
    await holdHost(page, false);
    await settle(page, quiet, null);
    await page.waitForTimeout(300);
    const named = await page.evaluate((id) => window.__studio.doc.root.entities[id].name, a);
    const errors = await page.evaluate((n) => [...document.querySelector('#console-lines').children].slice(n)
      .filter((e) => e.classList.contains('error')).map((e) => e.textContent), consoleFrom);
    check('the host had answered neither edit when Ctrl+Z was pressed', unanswered === 1, unanswered);
    check('an undo behind unanswered edits undoes the last of them, after them',
      named === 'Block Beta' && (await translation(page, a)).every((v, i) => Math.abs(v - moved[i]) < 1e-6) && !errors.length,
      { named, translation: await translation(page, a), errors });
    await pressAndWait(page, 'Control+z', 'undoing the rename');

    // 5d. Ctrl+S while typing in a field saves the typed value, behind it.
    const sceneFile = path.join(project, 'content', 'scenes', 'blocks.scene.json');
    await tx.fill('7');
    await page.keyboard.press('Control+s');
    await waitFor(page, () => !window.__studio.doc.dirty && !window.__studio.host.waiting && !window.__studio.host.queue.length, null, 'the save');
    const savedTyping = JSON.parse(fs.readFileSync(sceneFile, 'utf8')).entities[a].components.transform.translation;
    check('Ctrl+S while typing saves what is typed', savedTyping[0] === 7 && (await translation(page, a))[0] === 7, savedTyping);
    check('and the keyboard stays in the field', (await focusOf(page)) === 'transform.translation/0', await focusOf(page));
    await focusViewport(page);
    await pressAndWait(page, 'Control+z', 'undoing the typed translation');

    // 5e. Two quick clicks on + (the host answering slowly) add two entities; a new child's
    // translation, which has no stored value and no default, can be typed.
    const count = () => page.evaluate(() => Object.keys(window.__studio.doc.root.entities).length);
    const count0 = await count();
    await page.evaluate((id) => window.__studioView.select(id), tower);
    await holdHost(page, true);
    await page.click('#add-entity');
    await page.click('#add-entity');
    await holdHost(page, false);
    await settle(page, (n) => Object.keys(window.__studio.doc.root.entities).length >= n + 2, count0, 5000);
    await settle(page, quiet, null);
    check('two quick clicks on + add two entities', (await count()) === count0 + 2, { before: count0, after: await count() });
    const child = await page.evaluate(() => window.__studio.selection);
    const childX = fieldInputs(page, 'transform.translation');
    check('a new child shows a translation to type', (await childX.count()) === 3, await childX.count());
    if ((await childX.count()) !== 3) throw new Error('no translation to type for a new child');
    rev = await revision(page);
    await childX.first().fill('2');
    await childX.first().press('Enter');
    await waitFor(page, (r) => window.__studio.doc.rev > r, rev, 'the child translation');
    check('and typing it sets it', JSON.stringify(await translation(page, child)) === '[2,0,0]', await translation(page, child));
    await focusViewport(page);
    for (let i = 0; i < 3; i++) await pressAndWait(page, 'Control+z', 'undoing the children');
    check('undo takes them away again', (await count()) === count0);

    // 5f. Two cooks overlapping, the first loading slowly: the newest one shows, on the canvas
    // in the page, and every engine it replaced or overtook is destroyed.
    await waitFor(page, (r) => window.__studio.engine && window.__studio.cooked.rev >= r, await revision(page), 'the re-cook');
    const made = await page.evaluate(() => {
      window.__engineDelays = [1500, 0];
      return window.__engines.length;
    });
    await page.click('#recook');
    await page.waitForTimeout(300);
    await page.click('#recook');
    await settle(page, (n) => window.__engines.length >= n + 2 && window.__engines.every((e) => e.engine), made);
    await page.waitForTimeout(500);
    const loads = await page.evaluate(() => {
      const list = window.__engines;
      const last = list[list.length - 1];
      return {
        newestShown: window.__studio.engine === last.engine,
        onThePage: last.canvas.isConnected && last.canvas === window.__studio.canvas,
        othersDestroyed: list.slice(0, -1).every((e) => e.destroyed),
        canvases: document.querySelectorAll('#viewport-canvas').length,
      };
    });
    check('overlapping cooks: the newest load shows and the engines before it are destroyed',
      loads.newestShown && loads.onThePage && loads.othersDestroyed && loads.canvases === 1, loads);
    await page.evaluate((id) => window.__studioView.select(id), a);

    // 6. Add a light component: a structural change re-cooks the scene.
    const lampsBefore = await page.evaluate(() => window.__studio.lamps.length);
    rev = await revision(page);
    await page.locator('#inspector-body select.add').selectOption('light');
    await waitFor(page, (r) => window.__studio.doc.rev > r, rev, 'the new component');
    await page.evaluate((id) => {
      // Make it a lamp that follows the evening, so the cook bakes it into the world.
      const s = window.__studio;
      return s.host.edit(() => ({ type: 'edit', doc: s.doc.id, rev: s.doc.rev, label: 'Lamp', ops: [{ set: ['entities', id, 'components', 'light', 'follows_lamps'], value: true }] }));
    }, a);
    rev = await revision(page);
    // The viewport re-cooks after structural edits; wait for a cook of this revision.
    await waitFor(page, (r) => window.__studio.cooked && window.__studio.cooked.rev >= r && window.__studio.engine
      && window.__studio.lamps.length === (window.__studio.cooked.lamps || []).length, rev, 'the re-cook');
    const lampsAfter = await page.evaluate(() => window.__studio.lamps.length);
    check('adding a lamp re-cooks the scene with one more light', lampsAfter === lampsBefore + 1, { lampsBefore, lampsAfter });

    // 7. Save, reload, find the edits.
    await page.click('#save');
    await waitFor(page, () => window.__studio.doc && !window.__studio.doc.dirty, null, 'the save');
    const text = fs.readFileSync(sceneFile, 'utf8');
    fs.copyFileSync(sceneFile, path.join(o.out, 'blocks.scene.json'));
    const saved = JSON.parse(text);
    check('the saved file has the move', JSON.stringify(saved.entities[a].components.transform.translation) === JSON.stringify(moved),
      saved.entities[a].components.transform.translation);
    check('the saved file is canonical JSON (sorted keys, two-space indent, final newline)',
      text.endsWith('}\n') && text.startsWith('{\n  "abe": "scene",\n  "entities": {'));
    await page.reload();
    await waitFor(page, () => window.__studio?.engine && window.__studio.cooked && window.__studio.doc, null, 'the reload');
    await page.waitForTimeout(800);
    const reopened = await translation(page, a);
    check('reopening shows the saved move', reopened.every((v, i) => Math.abs(v - moved[i]) < 1e-6), reopened);
    check('reopening draws it there too', Math.abs((await instanceMatrix(page, a))[12] - moved[0]) < 1e-3);

    // 7b. The host goes away: a move made meanwhile snaps back to the document and nothing is
    // left waiting. It comes back: the page connects again, reopens the scene and edits.
    await page.evaluate((id) => window.__studioView.select(id), a);
    const loadsBefore = await page.evaluate(() => window.__studio.loadGen);
    offline[0] = logs.length;
    host.kill();
    await waitFor(page, () => document.querySelector('#connection').className === 'disconnected', null, 'the disconnection', 10000);
    const dragX = async () => {
      const g2 = await page.evaluate(() => {
        const z = window.__studioView.gizmo();
        return { o: z.po, x: z.ends[0] };
      });
      const d2 = [g2.x.x - g2.o.x, g2.x.y - g2.o.y];
      const l2 = Math.hypot(d2[0], d2[1]);
      await page.mouse.move(vp.x + g2.x.x, vp.y + g2.x.y);
      await page.mouse.down();
      for (let i = 1; i <= 6; i++) await page.mouse.move(vp.x + g2.x.x + (d2[0] / l2) * 9 * i, vp.y + g2.x.y + (d2[1] / l2) * 9 * i);
      await page.mouse.up();
    };
    await dragX();
    await page.waitForTimeout(300);
    const offlineMove = await page.evaluate((id) => {
      const s = window.__studio;
      const m = s.engine.worldInstance(s.cooked.entities[s.records.get(id)].instances[0]).matrix;
      return { drawnX: m[12], documentX: s.doc.root.entities[id].components.transform.translation[0], queued: s.host.queue.length, waiting: !!s.host.waiting };
    }, a);
    check('a move made without the host snaps back to the document, nothing left waiting',
      Math.abs(offlineMove.drawnX - offlineMove.documentX) < 1e-3 && !offlineMove.queued && !offlineMove.waiting, offlineMove);
    host = await startHost(project, port, lines);
    await waitFor(page, (g) => document.querySelector('#connection').className === 'connected'
      && window.__studio.loadGen > g && window.__studio.engine && window.__studio.doc, loadsBefore, 'the reconnection', 60000);
    offline[1] = logs.length;
    await page.waitForTimeout(500);
    // An edit, taken back, so the scene stays as saved for the steps after this one.
    await page.evaluate((id) => window.__studioView.select(id), a);
    const renameBox = page.locator('#inspector-body input[type=text]').first();
    rev = await revision(page);
    await renameBox.fill('Block Again');
    await renameBox.press('Enter');
    await waitFor(page, (r) => window.__studio.doc.rev > r, rev, 'a rename after reconnecting');
    const renamedAgain = await page.evaluate((id) => window.__studio.doc.root.entities[id].name, a);
    await focusViewport(page);
    await pressAndWait(page, 'Control+z', 'undoing the rename');
    check('the page connects again, reopens the scene and edits it',
      renamedAgain === 'Block Again' && !(await page.evaluate(() => window.__studio.doc.dirty)), renamedAgain);

    await page.screenshot({ path: path.join(o.out, 'studio-reopened.png') });

    // 8. The same scene, cooked by the command-line cooker, for the standalone player.
    const abew = path.join(o.out, 'blocks.abew');
    const cookOut = execFileSync(path.join(BIN, 'abe-build'), ['cook-scene', project, 'content/scenes/blocks.scene.json', abew]).toString();
    const inspect = execFileSync(path.join(BIN, 'abe-build'), ['inspect', abew]).toString();
    check('abe-build cooks the saved scene with its entities', /ABEW v4/.test(inspect) && inspect.includes('"Block Alpha"'), cookOut.trim());
    const look = path.join(project, 'content', 'looks', 'day.look.json');
    execFileSync(path.join(BIN, 'abe-render'), ['--scene', abew, '--look', look, '--view', 'overview',
      '--width', '640', '--height', '360', '--samples', '1', '--out', path.join(o.out, 'player-still.png')], { stdio: 'inherit' });
    check('abe-render draws the cooked scene', fs.existsSync(path.join(o.out, 'player-still.png')));
  } catch (e) {
    failed = e.message;
    console.log(`FAIL ${e.message}`);
    await page.screenshot({ path: path.join(o.out, 'studio-failure.png') }).catch(() => {});
  }
  // The reconnection attempts while the host was down in step 7b are expected to fail.
  const outage = (l, i) => i >= offline[0] && i < offline[1] && /WebSocket connection to|ERR_CONNECTION_REFUSED/.test(l);
  const pageErrors = logs.filter((l, i) => (l.startsWith('[pageerror]') || /\[error\]/.test(l)) && !outage(l, i));
  check('no page errors', pageErrors.length === 0, pageErrors.slice(0, 5));
  fs.writeFileSync(path.join(o.out, 'console.txt'), logs.join('\n'));
  fs.writeFileSync(path.join(o.out, 'host.txt'), lines.join(''));
  await browser.close();
  host.kill();
  if (!o.keep) fs.rmSync(project, { recursive: true, force: true });
  const ok = !failed && checks.every((c) => c.ok);
  fs.writeFileSync(path.join(o.out, 'results.json'), JSON.stringify({ ok, failed, api: o.api || 'auto', checks }, null, 1));
  console.log(ok ? `editor test passed (${checks.length} checks)` : 'editor test FAILED');
  process.exit(ok ? 0 : 1);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
