// End-to-end verification of Ganbate Studio's Unified Docked Workbenches, PIE, Live Release & AI IDE Bar:
// - Kind::Data (*.data.json) creation, schema inspector, asset-usage ("Where Used"), and canonical save
// - studio-game.json Play-in-Editor (PIE) viewport + 2x2 Multi-Device Matrix + strict cross-origin cookie/referer isolation
// - Interactive Responsive Anchor Layout Rig (#layout-rig-overlay) with Mirror X + merged single-step Undo
// - Bidirectional PIE UI Picker (game:select-data & game:edit-data)
// - Docked Workbench Deck (#workspace-tabs + window.__ganbateStudioApi) authoring custom *.seq.json / *.soundbank.json
//   documents + streaming studio:clock & studio:cue to PIE + docked ?embed=studio project workbench tab
// - AI Agentic IDE Bar (#ai-ide-bar): live @selection sync, .gbs scene editing + Undo, <100ms Verify + 4-angle contactSheet,
//   and Brick Crystallize
// - Native Skip-Deploy Live Release round-trip (Stage Draft -> Publish Live -> Pull Live to Project -> Save)
'use strict';

const fs = require('node:fs');
const http = require('node:http');
const path = require('node:path');

const OBSERVATORY_SCHEMAS = {
  ganbate: 'schema',
  version: 1,
  components: {
    'hud.anchors': {
      label: 'Observatory HUD',
      editor: 'layout-rig',
      doc: 'Normalized viewport anchors for telemetry widgets.',
      fields: {
        opacity: { type: 'float', min: 0, max: 1, default: 0.92 },
        compass_left: { type: 'vec2', unit: 'norm', min: 0, max: 1, default: [0.2, 0.18] },
        compass_right: { type: 'vec2', unit: 'norm', min: 0, max: 1, default: [0.8, 0.18] },
        banner: {
          type: 'object',
          fields: {
            pos: { type: 'vec2', unit: 'norm', min: 0, max: 1, default: [0.5, 0.85] },
            title: { type: 'string', default: 'Sector Horizon' },
          },
        },
      },
    },
    'fx.sequence': {
      label: 'Telemetry Sequence',
      editor: 'sequencer',
      doc: 'Timed telemetry motion and audio cues.',
      fields: {
        stiffness: { type: 'float', min: 20, max: 600, default: 220 },
        damping: { type: 'float', min: 1, max: 80, default: 20 },
        cues: {
          type: 'array',
          max: 8,
          default: [
            { id: 'pulse_in', name: 'Pulse In', delay: 0.2, duration: 0.45, sound: null },
            { id: 'beacon_lock', name: 'Beacon Lock', delay: 0.75, duration: 0.35, sound: null },
          ],
          items: {
            type: 'object',
            fields: {
              id: { type: 'string', default: 'cue' },
              name: { type: 'string', default: 'Cue' },
              delay: { type: 'float', unit: 's', min: 0, max: 10, default: 0 },
              duration: { type: 'float', unit: 's', min: 0.05, max: 5, default: 0.3 },
              sound: { type: 'asset', kind: 'sound' },
            },
          },
        },
      },
    },
    'ui.theme': {
      label: 'Observatory Theme',
      editor: 'style-tokens',
      doc: 'Surface and accent tokens for Observatory panels.',
      fields: {
        accent: { type: 'rgb', default: [0.2, 0.65, 0.95] },
        panel_bg: { type: 'rgba', default: [0.06, 0.08, 0.12, 0.88] },
        corner_radius: { type: 'int', min: 0, max: 32, default: 10 },
      },
    },
  },
};

function buildMockPieHtml() {
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Observatory Runtime</title>
<style>
  html, body { margin: 0; width: 100%; height: 100%; background: #0c1220; color: #e2ecf8; font: 13px/1.4 system-ui, sans-serif; overflow: hidden; position: relative; }
  .hud-widget { position: absolute; transform: translate(-50%, -50%); padding: 6px 10px; border-radius: 6px; background: rgba(24, 42, 68, 0.88); border: 1px solid #5cc9f5; cursor: pointer; user-select: none; }
  #pie-controls { position: absolute; bottom: 10px; left: 10px; display: flex; gap: 8px; }
  button { background: #1e2d44; color: #fff; border: 1px solid #5cc9f5; border-radius: 4px; padding: 4px 8px; cursor: pointer; }
</style>
</head>
<body>
  <div id="hud-banner" class="hud-widget" data-studio-component="hud.anchors" style="left:50%;top:85%">Sector Horizon</div>
  <div id="hud-left" class="hud-widget" style="left:20%;top:18%">W</div>
  <div id="hud-right" class="hud-widget" style="left:80%;top:18%">E</div>
  <div id="pie-controls">
    <button id="pie-nudge-opacity" type="button">Set Opacity 0.64</button>
  </div>
<script>
  window.__pieLoads = (window.__pieLoads || 0) + 1;
  window.__pieState = {
    syncCount: 0,
    docUpdates: [],
    clocks: [],
    cues: [],
    uiPick: false,
    schemas: {},
  };

  function applyHud(hud) {
    if (!hud) return;
    const banner = document.getElementById('hud-banner');
    const left = document.getElementById('hud-left');
    const right = document.getElementById('hud-right');
    if (hud.banner && Array.isArray(hud.banner.pos)) {
      banner.style.left = (hud.banner.pos[0] * 100) + '%';
      banner.style.top = (hud.banner.pos[1] * 100) + '%';
    }
    if (hud.banner && typeof hud.banner.title === 'string') {
      banner.textContent = hud.banner.title;
    }
    if (Array.isArray(hud.compass_left)) {
      left.style.left = (hud.compass_left[0] * 100) + '%';
      left.style.top = (hud.compass_left[1] * 100) + '%';
    }
    if (Array.isArray(hud.compass_right)) {
      right.style.left = (hud.compass_right[0] * 100) + '%';
      right.style.top = (hud.compass_right[1] * 100) + '%';
    }
    if (typeof hud.opacity === 'number') {
      document.body.style.opacity = String(hud.opacity);
    }
  }

  window.addEventListener('message', (ev) => {
    const msg = ev.data;
    if (!msg || msg.ganbateStudio !== 1) return;
    if (msg.type === 'studio:sync') {
      window.__pieState.syncCount += 1;
      window.__pieState.uiPick = !!msg.uiPick;
      window.__pieState.schemas = msg.schemas || {};
      applyHud(window.__pieState.schemas['hud.anchors']);
    } else if (msg.type === 'studio:doc-update') {
      window.__pieState.docUpdates.push(msg);
      if (msg.schema && msg.values) {
        window.__pieState.schemas[msg.schema] = msg.values;
        if (msg.schema === 'hud.anchors') applyHud(msg.values);
      }
    } else if (msg.type === 'studio:clock') {
      window.__pieState.clocks.push(msg);
    } else if (msg.type === 'studio:cue') {
      window.__pieState.cues.push(msg);
    } else if (msg.type === 'studio:ui-pick') {
      window.__pieState.uiPick = !!msg.active;
    }
  });

  document.getElementById('hud-banner').addEventListener('click', () => {
    window.parent.postMessage({
      ganbateGame: 1,
      type: 'game:select-data',
      schema: 'hud.anchors',
      path: ['banner', 'title'],
    }, '*');
  });

  document.getElementById('pie-nudge-opacity').addEventListener('click', () => {
    window.parent.postMessage({
      ganbateGame: 1,
      type: 'game:edit-data',
      schema: 'hud.anchors',
      label: 'PIE Opacity',
      ops: [{ set: ['opacity'], value: 0.64 }],
    }, '*');
  });

  window.parent.postMessage({ ganbateGame: 1, type: 'game:ready' }, '*');
</script>
</body>
</html>`;
}

function startMockGameServer() {
  const state = {
    requests: [],
    leakedCookies: [],
    leakedReferers: [],
    draftBundle: null,
    liveBundle: { documents: {}, schemas: {} },
    revision: 0,
    note: '',
  };

  const server = http.createServer((req, res) => {
    const cookieHeader = req.headers.cookie || '';
    const refererHeader = req.headers.referer || '';
    state.requests.push({ method: req.method, url: req.url, origin: req.headers.origin || null });
    if (cookieHeader.includes('ganbate_studio_pair')) {
      state.leakedCookies.push({ url: req.url, cookie: cookieHeader });
    }
    if (refererHeader) {
      state.leakedReferers.push({ url: req.url, referer: refererHeader });
    }

    const reqOrigin = req.headers.origin;
    if (reqOrigin) {
      res.setHeader('Access-Control-Allow-Origin', reqOrigin);
      res.setHeader('Access-Control-Allow-Credentials', 'true');
      res.setHeader('Access-Control-Allow-Methods', 'GET, PUT, POST, OPTIONS');
      res.setHeader('Access-Control-Allow-Headers', 'Content-Type, Accept');
      res.setHeader('Vary', 'Origin');
    }
    if (req.method === 'OPTIONS') {
      res.writeHead(204);
      res.end();
      return;
    }

    const u = new URL(req.url, 'http://127.0.0.1');
    if (u.pathname === '/favicon.ico') {
      res.writeHead(204);
      res.end();
      return;
    }

    const readBody = () => new Promise((resolve) => {
      const chunks = [];
      req.on('data', (c) => chunks.push(c));
      req.on('end', () => {
        const txt = Buffer.concat(chunks).toString('utf8');
        resolve(txt ? JSON.parse(txt) : {});
      });
    });

    if (req.method === 'GET' && u.pathname === '/api/dev-studio/live-release') {
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({
        revision: state.revision,
        hasDraft: !!state.draftBundle,
        note: state.note,
      }));
      return;
    }

    if (req.method === 'PUT' && u.pathname === '/api/dev-studio/live-bundle') {
      readBody().then((body) => {
        state.draftBundle = body;
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({
          ok: true,
          revision: state.revision,
          hasDraft: true,
        }));
      });
      return;
    }

    if (req.method === 'POST' && u.pathname === '/api/dev-studio/live-release/publish') {
      readBody().then((body) => {
        if (state.draftBundle) {
          state.liveBundle = JSON.parse(JSON.stringify(state.draftBundle));
          state.draftBundle = null;
        }
        state.revision += 1;
        state.note = body.note || '';
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({
          ok: true,
          revision: state.revision,
          hasDraft: false,
          note: state.note,
        }));
      });
      return;
    }

    if (req.method === 'POST' && u.pathname === '/api/dev-studio/live-release/discard') {
      state.draftBundle = null;
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({
        ok: true,
        revision: state.revision,
        hasDraft: false,
      }));
      return;
    }

    if (req.method === 'GET' && u.pathname === '/api/studio/live-bundle') {
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify(state.liveBundle));
      return;
    }

    res.writeHead(200, {
      'Content-Type': 'text/html; charset=utf-8',
      'Set-Cookie': 'observatory_pie=1; Path=/; HttpOnly',
    });
    res.end(buildMockPieHtml());
  });

  return new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => {
      const { port } = server.address();
      resolve({ server, port, state });
    });
  });
}

async function waitForAsync(predicate, what, timeoutMs = 20000) {
  const t0 = Date.now();
  while (Date.now() - t0 < timeoutMs) {
    try {
      const val = await predicate();
      if (val) return val;
    } catch {
      /* frame may still be navigating */
    }
    await new Promise((r) => setTimeout(r, 60));
  }
  throw new Error(`timed out waiting for ${what}`);
}

async function getSinglePieContentFrame(page) {
  const handle = await page.locator('#pie-stage iframe.pie-frame').elementHandle();
  if (!handle) return null;
  const frame = await handle.contentFrame();
  await handle.dispose();
  return frame;
}

async function getMatrixPieContentFrames(page) {
  const handles = await page.locator('#pie-matrix iframe.pie-frame').elementHandles();
  const frames = [];
  for (const h of handles) {
    const f = await h.contentFrame();
    await h.dispose();
    if (f) frames.push(f);
  }
  return frames;
}

async function createDataAssetViaDialog(page, schema, relPath, docName) {
  await page.locator('#new-data').click();
  await page.waitForFunction(() => document.getElementById('new-data-dialog')?.open === true);
  await page.locator('#new-data-schema').selectOption(schema);
  await page.locator('#new-data-name').fill(docName);
  await page.locator('#new-data-path').fill(relPath);
  await page.locator('#new-data-submit').click();
  await page.waitForFunction((expectedPath) => {
    const s = window.__studio;
    const d = s.inspectedData && s.datas.get(s.inspectedData);
    return d?.root && d.path === expectedPath;
  }, relPath);
  return page.evaluate(() => window.__studio.inspectedData);
}

module.exports = async function verifyGameEditor(page, project, check) {
  // Set an HttpOnly cookie scoped to Studio's `localhost` host so we verify zero leakage to `127.0.0.1`.
  await page.context().addCookies([
    { name: 'ganbate_studio_pair', value: 'studio-secret-token', domain: 'localhost', path: '/', httpOnly: true },
  ]);

  const mock = await startMockGameServer();
  try {
    // 1. Write project schemas (`schemas/observatory.schema.json`) and `editor/studio-game.json`, then rescan.
    fs.mkdirSync(path.join(project, 'schemas'), { recursive: true });
    fs.mkdirSync(path.join(project, 'editor'), { recursive: true });
    fs.writeFileSync(
      path.join(project, 'schemas', 'observatory.schema.json'),
      JSON.stringify(OBSERVATORY_SCHEMAS, null, 2) + '\n',
    );
    fs.writeFileSync(
      path.join(project, 'editor', 'studio-game.json'),
      JSON.stringify({
        version: 1,
        origin: `http://127.0.0.1:${mock.port}`,
        entry: '/?studioPie=1',
        presets: [{ id: 'default', label: 'Observatory Deck', path: '/?studioPie=1' }],
        devices: [
          { id: 'phone-portrait', label: 'Phone · 390×844', width: 390, height: 844 },
          { id: 'phone-compact', label: 'Compact · 375×667', width: 375, height: 667 },
          { id: 'phone-landscape', label: 'Landscape · 844×390', width: 844, height: 390 },
          { id: 'tablet', label: 'Tablet · 1024×768', width: 1024, height: 768 },
        ],
        liveRelease: {
          statusPath: '/api/dev-studio/live-release',
          bundlePath: '/api/studio/live-bundle',
          stagePath: '/api/dev-studio/live-bundle',
          publishPath: '/api/dev-studio/live-release/publish',
          discardPath: '/api/dev-studio/live-release/discard',
        },
      }, null, 2) + '\n',
    );

    const prevReport = await page.evaluateHandle(() => window.__studio.scanReport);
    await page.locator('#rescan-assets').click();
    await page.waitForFunction(
      (old) => window.__studio.scanReport && window.__studio.scanReport !== old
        && window.__studio.pieViewport.getWorkspace() !== null
        && !!window.__studio.kinds['hud.anchors'],
      prevReport,
    );
    await prevReport.dispose();

    const toolbarVisible = await page.evaluate(() => ({
      pieToolbarHidden: document.getElementById('pie-toolbar').hidden,
      liveMenuHidden: document.getElementById('live-release-menu').hidden,
      schemas: Object.keys(window.__studio.kinds).filter((k) => k.includes('.')).sort(),
      workspaceTabs: [...document.querySelectorAll('#workspace-tabs .workspace-tab')].map((b) => b.dataset.workbench),
    }));
    check(
      'rescan discovers project schemas, studio-game.json workspace controls, and docked workbench tabs',
      !toolbarVisible.pieToolbarHidden &&
        !toolbarVisible.liveMenuHidden &&
        toolbarVisible.workspaceTabs.includes('world') &&
        toolbarVisible.workspaceTabs.includes('sequencer') &&
        toolbarVisible.workspaceTabs.includes('audio') &&
        toolbarVisible.workspaceTabs.includes('project:level-rules') &&
        JSON.stringify(toolbarVisible.schemas) === JSON.stringify(['fx.sequence', 'hud.anchors', 'ui.theme']),
      toolbarVisible,
    );

    // 2. Create and edit `.data.json` documents (`ui.theme`, `fx.sequence`, `hud.anchors`).
    const themePath = 'content/data/ui-theme.data.json';
    const seqPath = 'content/data/fx-sequence.data.json';
    const hudPath = 'content/data/hud-anchors.data.json';

    const themeId = await createDataAssetViaDialog(page, 'ui.theme', themePath, 'Observatory Theme');
    const styleTokensActive = await page.evaluate(() =>
      !!document.querySelector('#inspector-body .data-fields-wrap.style-tokens-mode'),
    );
    const radiusInput = page.locator('#inspector-body [data-data-path="corner_radius"] input[type="number"]');
    let rev = await page.evaluate((id) => window.__studio.datas.get(id).rev, themeId);
    await radiusInput.fill('16');
    await radiusInput.press('Enter');
    await page.waitForFunction(({ id, r }) => window.__studio.datas.get(id).rev > r, { id: themeId, r: rev });
    await page.locator('#save').click();
    await page.waitForFunction((id) => !window.__studio.datas.get(id).dirty, themeId);
    const savedTheme = JSON.parse(fs.readFileSync(path.join(project, themePath), 'utf8'));
    check(
      'ui.theme .data.json renders style-tokens inspector and writes canonical JSON on Save',
      styleTokensActive &&
        savedTheme.ganbate === 'data' &&
        savedTheme.version === 1 &&
        savedTheme.schema === 'ui.theme' &&
        savedTheme.values.corner_radius === 16,
      savedTheme,
    );

    // Create `fx.sequence`, assign `tone.wav` sound asset to `cues[0].sound`, save, and verify Where Used!
    const seqId = await createDataAssetViaDialog(page, 'fx.sequence', seqPath, 'Telemetry Sequence');
    const soundId = await page.evaluate(() => window.__studio.assets.find((a) => a.kind === 'sound')?.id || null);
    rev = await page.evaluate((id) => window.__studio.datas.get(id).rev, seqId);
    await page.locator('#inspector-body [data-data-path="cues.0.sound"] select').selectOption(soundId);
    await page.waitForFunction(({ id, r }) => window.__studio.datas.get(id).rev > r, { id: seqId, r: rev });
    await page.locator('#save').click();
    await page.waitForFunction((id) => !window.__studio.datas.get(id).dirty, seqId);

    // Select `tone.wav` in Content Browser to verify `asset-usage` ("Where Used") includes `fx-sequence.data.json`.
    await page.evaluate((sId) => {
      window.__studio.assetLibrary.revealAsset(sId);
    }, soundId);
    await page.locator(`#assets-list [data-id="${soundId}"]`).click();
    await page.waitForFunction(() => {
      const items = [...document.querySelectorAll('#assets-inspector .where-used-item')];
      return items.some((el) => el.textContent.includes('Telemetry Sequence') || el.textContent.includes('fx-sequence.data.json'));
    });
    check('asset-usage ("Where Used") tracks asset references inside .data.json array items', true);

    // Create `hud.anchors` (newly created `.data.json` is already written cleanly on disk).
    const hudId = await createDataAssetViaDialog(page, 'hud.anchors', hudPath, 'Observatory HUD');
    await page.waitForFunction((id) => !window.__studio.datas.get(id).dirty, hudId);

    // 3. Switch to Game (PIE), verify `studio:sync`, drag `#layout-rig-overlay` handle with Mirror X, and single-step Undo.
    await page.locator('#view-mode-pie').click();
    await page.locator('#pie-device').selectOption('phone-portrait');
    const pieFrame = page.frameLocator('#pie-stage iframe.pie-frame');
    await pieFrame.locator('#hud-banner').waitFor();
    await waitForAsync(async () => {
      const cf = await getSinglePieContentFrame(page);
      if (!cf) return false;
      return cf.evaluate(() => !!window.__pieState && window.__pieState.syncCount >= 1 && !!window.__pieState.schemas['hud.anchors']);
    }, 'PIE initial studio:sync');

    const rigBefore = await page.evaluate(() => ({
      sceneHidden: document.getElementById('viewport').hidden,
      pieHidden: document.getElementById('pie-stage').hidden,
      rigHidden: document.getElementById('layout-rig-overlay').hidden,
      handles: [...document.querySelectorAll('#layout-rig-overlay .layout-rig-handle')].map((h) => h.dataset.anchorPath).sort(),
    }));
    check(
      'Game (PIE) mounts framed device viewport and activates Layout Rig handles',
      rigBefore.sceneHidden && !rigBefore.pieHidden && !rigBefore.rigHidden &&
        JSON.stringify(rigBefore.handles) === JSON.stringify(['banner.pos', 'compass_left', 'compass_right']),
      rigBefore,
    );

    // Enable Mirror X and drag `compass_left` across 6 steps so both `compass_left` and `compass_right` stream live to PIE.
    await page.locator('#layout-rig-mirror').click();
    const leftHandle = page.locator('#layout-rig-overlay [data-anchor-path="compass_left"]');
    const hBox = await leftHandle.boundingBox();
    const startX = hBox.x + hBox.width / 2;
    const startY = hBox.y + hBox.height / 2;
    rev = await page.evaluate((id) => window.__studio.datas.get(id).rev, hudId);
    await page.mouse.move(startX, startY);
    await page.mouse.down();
    for (let step = 1; step <= 6; step++) {
      await page.mouse.move(startX + step * 8, startY + step * 6);
    }
    await page.mouse.up();
    await page.waitForFunction(({ id, r }) => window.__studio.datas.get(id).rev > r
      && !window.__studio.host.waiting && !window.__studio.host.queue.length, { id: hudId, r: rev });

    const studioAnchors = await page.evaluate((id) => {
      const doc = window.__studio.datas.get(id);
      return {
        left: doc.root.values.compass_left,
        right: doc.root.values.compass_right,
      };
    }, hudId);
    const pieAfterDrag = await waitForAsync(async () => {
      const cf = await getSinglePieContentFrame(page);
      if (!cf) return null;
      const info = await cf.evaluate(() => ({
        pieLeft: window.__pieState.schemas['hud.anchors']?.compass_left,
        pieLoads: window.__pieLoads,
        pieUpdates: window.__pieState.docUpdates.length,
      }));
      return JSON.stringify(info.pieLeft) === JSON.stringify(studioAnchors.left) ? info : null;
    }, 'PIE live anchor update');
    const afterRigDrag = { ...studioAnchors, ...pieAfterDrag };
    check(
      'dragging a Layout Rig handle with Mirror X streams live studio:doc-update to PIE without reload',
      afterRigDrag.left[0] > 0.25 &&
        Math.abs(afterRigDrag.left[0] + afterRigDrag.right[0] - 1) < 1e-3 &&
        Math.abs(afterRigDrag.left[1] - afterRigDrag.right[1]) < 1e-3 &&
        JSON.stringify(afterRigDrag.pieLeft) === JSON.stringify(afterRigDrag.left) &&
        afterRigDrag.pieLoads === 1 &&
        afterRigDrag.pieUpdates >= 1,
      afterRigDrag,
    );

    // Single-step Undo undoes the entire merged multi-step drag!
    rev = await page.evaluate((id) => window.__studio.datas.get(id).rev, hudId);
    await page.locator('#undo').click();
    await page.waitForFunction(({ id, r }) => window.__studio.datas.get(id).rev > r, { id: hudId, r: rev });
    const afterRigUndo = await page.evaluate((id) => {
      const doc = window.__studio.datas.get(id);
      return { dirty: doc.dirty, values: doc.root.values };
    }, hudId);
    check(
      'merged Layout Rig drag undoes completely in a single Undo step',
      !afterRigUndo.dirty && afterRigUndo.values.compass_left === undefined && afterRigUndo.values.compass_right === undefined,
      afterRigUndo,
    );

    // Redo and save so the updated anchors persist.
    rev = await page.evaluate((id) => window.__studio.datas.get(id).rev, hudId);
    await page.locator('#redo').click();
    await page.waitForFunction(({ id, r }) => window.__studio.datas.get(id).rev > r, { id: hudId, r: rev });
    await page.locator('#save').click();
    await page.waitForFunction((id) => !window.__studio.datas.get(id).dirty, hudId);

    // 4. Multi-Device Matrix: verify 4 responsive device iframes render scaled and stay synchronized on live edits!
    await page.locator('#view-mode-matrix').click();
    await waitForAsync(async () => {
      const cfs = await getMatrixPieContentFrames(page);
      if (cfs.length !== 4) return false;
      const synced = await Promise.all(cfs.map((cf) => cf.evaluate(() => window.__pieState?.syncCount >= 1)));
      return synced.every(Boolean);
    }, '4 Device Matrix iframes synced');

    const titleInput = page.locator('#inspector-body [data-data-path="banner.title"] input[type="text"]');
    rev = await page.evaluate((id) => window.__studio.datas.get(id).rev, hudId);
    await titleInput.fill('Orbital Array');
    await titleInput.press('Enter');
    await page.waitForFunction(({ id, r }) => window.__studio.datas.get(id).rev > r, { id: hudId, r: rev });
    await waitForAsync(async () => {
      const cfs = await getMatrixPieContentFrames(page);
      if (cfs.length !== 4) return false;
      const texts = await Promise.all(
        cfs.map((cf) => cf.evaluate(() => document.getElementById('hud-banner')?.textContent)),
      );
      return texts.every((txt) => txt === 'Orbital Array');
    }, 'Device Matrix synchronized banner text');

    const matrixInfo = await page.evaluate(() => {
      const shells = [...document.querySelectorAll('#pie-matrix .pie-device-shell')];
      return {
        count: shells.length,
        scaled: shells.every((s) => s.style.transform.startsWith('scale(')),
      };
    });
    check(
      'Device Matrix renders 4 CSS-scaled viewports and synchronizes live edits across all devices',
      matrixInfo.count === 4 && matrixInfo.scaled,
      matrixInfo,
    );
    await page.locator('#save').click();
    await page.waitForFunction((id) => !window.__studio.datas.get(id).dirty, hudId);

    // 5. Bidirectional PIE UI Picker (`game:select-data` & `game:edit-data`).
    await page.locator('#view-mode-pie').click();
    await waitForAsync(async () => {
      const cf = await getSinglePieContentFrame(page);
      return cf ? cf.evaluate(() => window.__pieState?.syncCount >= 1) : false;
    }, 'return to single PIE');
    await page.locator('#pie-inspect-ui').click();
    await waitForAsync(async () => {
      const cf = await getSinglePieContentFrame(page);
      return cf ? cf.evaluate(() => window.__pieState?.uiPick === true) : false;
    }, 'PIE studio:ui-pick true');

    // Switch Studio Inspector away to `ui.theme` first, then click `[data-studio-component="hud.anchors"]` inside PIE!
    await page.evaluate((id) => window.__studio.inspectData(id), themeId);
    await pieFrame.locator('[data-studio-component="hud.anchors"]').click();
    await page.waitForFunction((expectedId) => {
      const flash = document.querySelector('#inspector-body .data-field-flash');
      return window.__studio.inspectedData === expectedId && flash?.dataset.dataPath === 'banner.title';
    }, hudId);
    check('clicking [data-studio-component] in PIE focuses the matching .data.json document and field', true);

    // Trigger `game:edit-data` from inside PIE (`#pie-nudge-opacity`) and verify Undo works!
    rev = await page.evaluate((id) => window.__studio.datas.get(id).rev, hudId);
    await pieFrame.locator('#pie-nudge-opacity').click();
    await page.waitForFunction(({ id, r }) => window.__studio.datas.get(id).rev > r
      && window.__studio.datas.get(id).root.values.opacity === 0.64, { id: hudId, r: rev });
    check('game:edit-data applies schema-validated undoable ops through the Studio host', true);

    rev = await page.evaluate((id) => window.__studio.datas.get(id).rev, hudId);
    await page.locator('#undo').click();
    await page.waitForFunction(({ id, r }) => window.__studio.datas.get(id).rev > r
      && !window.__studio.datas.get(id).dirty, { id: hudId, r: rev });

    // 6. Unified Workbench Deck (`window.__ganbateStudioApi`): custom `.seq.json` authoring + PIE clock/cue + docked `?embed=studio` tool!
    const wbResult = await page.evaluate(async () => {
      const api = window.__ganbateStudioApi;
      let mounted = false;
      api.registerWorkbench({
        id: 'sequencer',
        label: 'Sequencer',
        mount: (container, studioApi) => {
          mounted = true;
          container.innerHTML = '<div id="test-seq-workbench">Sequencer Ready</div>';
          studioApi.sendPieClock({ playing: true, time: 0.55, speed: 1 });
          studioApi.sendPieCue({ cueId: 'pulse_in', track: 'cues', payload: { gain: 0.9 } });
          return {};
        },
      });
      await api.activateWorkbench('sequencer');
      const created = await api.createDocument({
        path: 'content/sequences/intro.seq.json',
        kind: 'seq',
        name: 'Intro Sequence',
      });
      const edited = await api.edit(created.id, [{ set: ['duration'], value: 4.5 }], 'Set Sequence Duration');
      const undone = await api.undo(created.id);
      const redone = await api.redo(created.id);
      const savedOk = await api.save(created.id);
      return {
        mounted,
        bodyActive: document.body.classList.contains('workbench-active'),
        createdKind: created?.kind,
        editedDuration: edited?.root?.duration,
        undoneDirty: undone?.dirty,
        redoneDuration: redone?.root?.duration,
        savedOk,
      };
    });
    const savedSeqOnDisk = JSON.parse(
      fs.readFileSync(path.join(project, 'content/sequences/intro.seq.json'), 'utf8'),
    );
    check(
      'docked workbench mounts via studioApi, authors/undoes/redoes/saves custom .seq.json, and writes canonical JSON',
      wbResult.mounted &&
        wbResult.bodyActive &&
        wbResult.createdKind === 'seq' &&
        wbResult.editedDuration === 4.5 &&
        !wbResult.undoneDirty &&
        wbResult.redoneDuration === 4.5 &&
        wbResult.savedOk &&
        savedSeqOnDisk.duration === 4.5,
      wbResult,
    );

    await waitForAsync(async () => {
      const cf = await getSinglePieContentFrame(page);
      if (!cf) return false;
      return cf.evaluate(() =>
        window.__pieState?.clocks.some((c) => c.time > 0.5) &&
        window.__pieState?.cues.some((c) => c.cueId === 'pulse_in'),
      );
    }, 'studioApi sendPieClock and sendPieCue in PIE');
    check('studioApi.sendPieClock and sendPieCue stream studio:clock and studio:cue to PIE', true);

    // Docked `?embed=studio` project workbench tab
    let dockedEmbedHref = null;
    await page.route('http://127.0.0.1:5173/tools/levels?embed=studio', async (route) => {
      dockedEmbedHref = route.request().url();
      await route.fulfill({ contentType: 'text/html', body: '<h2 id="docked-tool-ready">Docked Level Rules</h2>' });
    });
    await page.locator('#workspace-tabs [data-workbench="project:level-rules"]').click();
    await page
      .frameLocator('#workbench-deck [data-workbench-pane="project:level-rules"] iframe.project-workbench-frame')
      .locator('#docked-tool-ready')
      .waitFor();
    check(
      'clicking a docked project workbench tab embeds the tool chrome-less with ?embed=studio',
      dockedEmbedHref === 'http://127.0.0.1:5173/tools/levels?embed=studio',
      { dockedEmbedHref },
    );

    // Return to World workspace and 3D Scene mode for AI Agentic Bar (`M-AI.1..4`) checks!
    await page.locator('#workspace-tabs [data-workbench="world"]').click();
    await page.locator('#view-mode-scene').click();
    if (await page.locator('#data-back-to-scene').count()) {
      await page.locator('#data-back-to-scene').click();
    }

    // 7. AI Agentic IDE Bar (`#ai-ide-bar`): `.gbs` apply + single-step Undo, `<100ms` Verify + contact sheet, and Crystallize!
    const sceneRevBeforeGbs = await page.evaluate(() => window.__studio.doc.rev);
    await page.locator('#ai-gbs-input').fill('spawn Beacon empty; place $Beacon at [1.5, 2, -1]');
    await page.locator('#ai-gbs-apply').click();
    await page.waitForFunction((r) => window.__studio.doc.rev > r, sceneRevBeforeGbs);
    const beaconId = await page.evaluate(() => {
      const ents = window.__studio.doc.root.entities || {};
      return Object.keys(ents).find((id) => ents[id].name === 'Beacon') || null;
    });
    check('AI IDE bar applies .gbs spawn statement as an atomic scene edit', !!beaconId, { beaconId });

    // Select Beacon -> verify `@selection` pill updates -> Crystallize Beacon into a reusable brick!
    await page.evaluate((id) => window.__studioView.select(id), beaconId);
    const pillText = await page.locator('#ai-selection-pill').innerText();
    const crystallized = await page.evaluate(() => window.__studio.runAiCrystallize('beacon_brick'));
    const brickFilePath = crystallized?.path ? path.join(project, crystallized.path) : '';
    check(
      'AI IDE bar tracks live @selection and crystallizes the selected entity into a reusable brick',
      pillText.includes('Beacon') &&
        !!crystallized?.path &&
        crystallized.path.endsWith('beacon_brick.brick.json') &&
        fs.existsSync(brickFilePath),
      { pillText, crystallized },
    );

    // Run `<100ms` Scene Verify (`#ai-verify-run`) and verify `worldHash` + 4-angle `contactSheet` PNG!
    const verifyReport = await page.evaluate(() => window.__studio.runAiSceneVerify());
    const hasContactSheetImg = await page.evaluate(
      () => !!document.querySelector('#ai-verify-results img[src^="data:image/png;base64,"]'),
    );
    check(
      'AI IDE bar Scene Verify computes deterministic worldHash, screenBoxes, and 4-angle PNG contactSheet',
      Boolean(verifyReport?.worldHash) &&
        Array.isArray(verifyReport?.screenBoxes) &&
        verifyReport.screenBoxes.length >= 1 &&
        hasContactSheetImg,
      { elapsedMs: verifyReport?.elapsedMs, worldHash: verifyReport?.worldHash },
    );

    // Single-step Undo removes the `.gbs` spawned Beacon cleanly so scene stays clean!
    const revBeforeGbsUndo = await page.evaluate(() => window.__studio.doc.rev);
    await page.locator('#undo').click();
    await page.waitForFunction((r) => window.__studio.doc.rev > r, revBeforeGbsUndo);
    const beaconAfterUndo = await page.evaluate((id) => !!window.__studio.doc.root.entities[id], beaconId);
    check('.gbs transaction undoes cleanly in a single Studio Undo step', !beaconAfterUndo);

    // 8. Native Skip-Deploy Live Release round-trip: Stage Draft -> Publish Live -> Pull Live to Project -> Save!
    await page.locator('#live-release-menu > summary').click();
    await page.locator('#live-stage').click();
    await page.waitForFunction(() => document.getElementById('live-release-badge')?.dataset.state === 'draft');
    check('Stage Draft uploads open and disk project documents to the game workspace', !!mock.state.draftBundle);

    await page.locator('#live-release-note').fill('Observatory tuning v1');
    await page.locator('#live-publish').click();
    await page.waitForFunction(() => document.getElementById('live-release-badge')?.dataset.state === 'live');
    const badgeText = await page.locator('#live-release-badge').innerText();
    check(
      'Publish Live promotes the staged bundle immediately without redeploy',
      mock.state.revision === 1 && badgeText.includes('r1'),
      { revision: mock.state.revision, badgeText },
    );

    // Simulate a live server tweak (`opacity = 0.77`) on `127.0.0.1` and Pull Live to Project!
    mock.state.liveBundle.documents[hudPath].values.opacity = 0.77;
    mock.state.liveBundle.schemas['hud.anchors'].opacity = 0.77;

    rev = await page.evaluate((id) => window.__studio.datas.get(id).rev, hudId);
    await page.locator('#live-pull').click();
    await page.waitForFunction(({ id, r }) => window.__studio.datas.get(id).rev > r
      && window.__studio.datas.get(id).root.values.opacity === 0.77
      && window.__studio.inspectedData === id
      && window.__studio.datas.get(id).dirty, { id: hudId, r: rev });
    await page.locator('#live-release-menu > summary').click();
    await page.locator('#save').click();
    await page.waitForFunction((id) => !window.__studio.datas.get(id).dirty, hudId);

    const diskAfterPull = JSON.parse(fs.readFileSync(path.join(project, hudPath), 'utf8'));
    check(
      'Pull Live to Project applies remote bundle diffs as undoable Studio edits and saves canonical JSON',
      diskAfterPull.values.opacity === 0.77,
      diskAfterPull.values,
    );

    check(
      'cross-origin PIE and Live Release requests leak neither Studio pairing cookies nor Referer headers',
      mock.state.requests.length >= 4 &&
        mock.state.leakedCookies.length === 0 &&
        mock.state.leakedReferers.length === 0,
      {
        requests: mock.state.requests.length,
        leakedCookies: mock.state.leakedCookies,
        leakedReferers: mock.state.leakedReferers,
      },
    );

    // Return cleanly to 3D Scene mode before finishing.
    await page.locator('#view-mode-scene').click();
    if (await page.locator('#data-back-to-scene').count()) {
      await page.locator('#data-back-to-scene').click();
    }
  } finally {
    await new Promise((resolve) => mock.server.close(resolve));
  }
};
