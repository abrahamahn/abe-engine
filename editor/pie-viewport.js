// Play-in-Editor (PIE) Game Viewport, Multi-Device Matrix & Runtime postMessage Bridge.
// Enforces strict cross-origin isolation (different hostname from Studio) so Studio's HttpOnly
// pairing cookie never leaks to the game origin.
import { projectAssetUrl } from './asset-library.js';
import { buildDataValueOps, materializeDataValues } from './data-inspector.js';
import { normalizeLoopbackOrigin, normalizeLoopbackQueryOverride } from './project-tools.js';

export const DEFAULT_PIE_DEVICES = Object.freeze([
  { id: 'phone-portrait', label: 'Phone · 390×844', width: 390, height: 844 },
  { id: 'phone-compact', label: 'Compact · 375×667', width: 375, height: 667 },
  { id: 'phone-landscape', label: 'Landscape · 844×390', width: 844, height: 390 },
  { id: 'tablet', label: 'Tablet · 1024×768', width: 1024, height: 768 },
]);

export const BUILTIN_GAMEPLAY_PRESETS = Object.freeze([
  { id: 'holdem-play', label: "Hold'em (/play)", path: '/play' },
  { id: 'blackjack-play', label: 'Blackjack (/play/blackjack)', path: '/play/blackjack' },
  { id: 'baccarat-play', label: 'Baccarat (/play/baccarat)', path: '/play/baccarat' },
  {
    id: 'living-stage-review',
    label: '3D Stage Review (/ganbate-stage-review.html)',
    path: '/ganbate-stage-review.html?living=1&room=1&hud=turn',
  },
]);

export function createDefaultGameManifest(origin = 'http://127.0.0.1:5173') {
  return {
    version: 1,
    origin,
    entry: '/play',
    presets: BUILTIN_GAMEPLAY_PRESETS.map((preset) => ({ ...preset })),
  };
}

function mergeBuiltinGameplayPresets(basePresets, gameOrigin) {
  const merged = [...basePresets];
  const seenIds = new Set(merged.map((p) => p.id));
  const seenPaths = new Set(merged.map((p) => p.path));
  for (const builtin of BUILTIN_GAMEPLAY_PRESETS) {
    if (seenIds.has(builtin.id) || seenPaths.has(builtin.path)) continue;
    const safe = safeOriginPath(builtin.path, gameOrigin, 'preset');
    merged.push({
      id: builtin.id,
      label: builtin.label,
      path: safe.path,
      href: safe.href,
    });
    seenIds.add(builtin.id);
    seenPaths.add(builtin.path);
  }
  return merged;
}

function webOrigin(value) {
  const url = new URL(value);
  const local = ['localhost', '127.0.0.1', '[::1]'].includes(url.hostname);
  if (
    url.username ||
    url.password ||
    url.search ||
    url.hash ||
    url.pathname !== '/' ||
    !(url.protocol === 'https:' || (url.protocol === 'http:' && local))
  ) {
    throw new Error('Game workspace requires an HTTPS origin or a local HTTP server.');
  }
  return { origin: url.origin, local };
}

function safeOriginPath(rawPath, origin, what) {
  if (
    typeof rawPath !== 'string' ||
    !rawPath.startsWith('/') ||
    rawPath.startsWith('//') ||
    rawPath.includes('\\')
  ) {
    throw new Error(`Invalid ${what} path.`);
  }
  const url = new URL(rawPath, origin);
  if (url.origin !== origin) {
    throw new Error(`${what} leaves its configured game origin.`);
  }
  return { path: rawPath, href: url.href };
}

export function resolveGameWorkspace(manifest, studioOrigin, override, options = {}) {
  const effectiveManifest = (!manifest && options?.fallback)
    ? createDefaultGameManifest(options.fallbackOrigin || 'http://127.0.0.1:5173')
    : manifest;
  if (!effectiveManifest || typeof effectiveManifest !== 'object' || effectiveManifest.version !== 1) {
    throw new Error('Invalid studio-game.json manifest.');
  }
  const allowSameOrigin = options?.allowSameOrigin === true;
  const rawOrigin = effectiveManifest.origin === 'same-origin' && allowSameOrigin
    ? new URL('/', studioOrigin).origin
    : effectiveManifest.origin;
  const configured = webOrigin(rawOrigin);
  const explicitOverride = Boolean(override);
  const candidate = explicitOverride && configured.local ? webOrigin(override) : configured;
  if (explicitOverride && configured.local && !candidate.local) {
    throw new Error('A local game origin override must stay local.');
  }
  const requested = normalizeLoopbackOrigin(candidate, studioOrigin, { allowSameOrigin, explicitOverride });
  // Cookies are host-scoped, not port-scoped. Studio's HttpOnly pairing cookie belongs only to Studio.
  if (
    !allowSameOrigin &&
    new URL(requested.origin).hostname.replace(/\.$/, '') ===
    new URL(studioOrigin).hostname.replace(/\.$/, '')
  ) {
    throw new Error('Use different hostnames for Studio and the game, e.g. localhost and 127.0.0.1.');
  }

  const entryRaw = effectiveManifest.entry ?? '/?studioPie=1';
  const entry = safeOriginPath(entryRaw, requested.origin, 'entry');

  const basePresets = Array.isArray(effectiveManifest.presets) && effectiveManifest.presets.length
    ? effectiveManifest.presets.map((preset, idx) => {
        if (!preset || typeof preset !== 'object' || typeof preset.label !== 'string' || !preset.label.trim()) {
          throw new Error('Invalid PIE preset label.');
        }
        const p = safeOriginPath(preset.path ?? entry.path, requested.origin, 'preset');
        return {
          id: typeof preset.id === 'string' && preset.id.trim() ? preset.id.trim() : `preset-${idx}`,
          label: preset.label.trim(),
          path: p.path,
          href: p.href,
        };
      })
    : [{ id: 'default', label: 'Main Stage', path: entry.path, href: entry.href }];

  const presets = options?.includeBuiltinPresets === false
    ? basePresets
    : mergeBuiltinGameplayPresets(basePresets, requested.origin);

  const devices = Array.isArray(effectiveManifest.devices) && effectiveManifest.devices.length
    ? effectiveManifest.devices.map((dev, idx) => {
        const width = Math.round(Number(dev?.width));
        const height = Math.round(Number(dev?.height));
        if (!Number.isFinite(width) || !Number.isFinite(height) || width < 120 || height < 120 || width > 7680 || height > 7680) {
          throw new Error('Invalid PIE device dimensions.');
        }
        return {
          id: typeof dev.id === 'string' && dev.id.trim() ? dev.id.trim() : `device-${idx}`,
          label: typeof dev.label === 'string' && dev.label.trim() ? dev.label.trim() : `${width}×${height}`,
          width,
          height,
        };
      })
    : DEFAULT_PIE_DEVICES.map((d) => ({ ...d }));

  let liveRelease = null;
  if (effectiveManifest.liveRelease && typeof effectiveManifest.liveRelease === 'object') {
    const lr = effectiveManifest.liveRelease;
    liveRelease = {
      statusPath: lr.statusPath ? safeOriginPath(lr.statusPath, requested.origin, 'liveRelease.statusPath').href : null,
      bundlePath: lr.bundlePath ? safeOriginPath(lr.bundlePath, requested.origin, 'liveRelease.bundlePath').href : null,
      stagePath: lr.stagePath ? safeOriginPath(lr.stagePath, requested.origin, 'liveRelease.stagePath').href : null,
      publishPath: lr.publishPath ? safeOriginPath(lr.publishPath, requested.origin, 'liveRelease.publishPath').href : null,
      discardPath: lr.discardPath ? safeOriginPath(lr.discardPath, requested.origin, 'liveRelease.discardPath').href : null,
    };
  }

  return {
    origin: requested.origin,
    entry: entry.href,
    entryPath: entry.path,
    presets,
    devices,
    liveRelease,
  };
}

export function createPieViewport({
  el,
  t,
  getContext,
  onSelectData,
  onEditDataFromGame,
  onSelectEntity,
  onModeChanged,
  onWorkspaceLoaded,
  log,
}) {
  let workspace = null;
  let fallbackOrigin = null;
  let mode = 'scene'; // 'scene' | 'pie' | 'matrix'
  let splitPlay = false;
  let activePresetId = 'default';
  let activeDeviceId = 'responsive';
  let uiPick = false;
  let loadGeneration = 0;

  // Preloaded disk snapshots for `.data.json` and `.look.json` so PIE immediately receives all authored assets.
  const preloadedDatas = new Map(); // id -> { id, path, root }
  const preloadedLooks = new Map(); // id -> { id, path, root }
  let preloadPromise = null;

  let singleFrame = null; // { shell, iframe, deviceId }
  let matrixFrames = []; // [{ tile, viewportBox, shell, iframe, device }]
  let resizeObserver = null;

  const pieToolbar = () => document.getElementById('pie-toolbar');
  const stageEl = () => document.getElementById('pie-stage');
  const matrixEl = () => document.getElementById('pie-matrix');
  const sceneViewportEl = () => document.getElementById('viewport');
  const previewFrameEl = () => document.getElementById('preview-frame');
  const studioOriginHref = () => globalThis.location?.origin || 'http://localhost:7480';

  function ensureActiveWorkspace() {
    if (workspace) return workspace;
    if (!fallbackOrigin) return null;
    try {
      const allowSameOrigin = Boolean(getContext?.()?.allowSameOrigin);
      workspace = resolveGameWorkspace(null, studioOriginHref(), null, {
        fallback: true,
        fallbackOrigin,
        allowSameOrigin,
      });
      if (!workspace.presets.some((p) => p.id === activePresetId)) {
        activePresetId = workspace.presets[0]?.id || 'default';
      }
      onWorkspaceLoaded?.(workspace);
      return workspace;
    } catch (err) {
      log?.(2, String(err.message || err));
      return null;
    }
  }

  function activePresetHref() {
    if (!workspace) return '';
    const found = workspace.presets.find((p) => p.id === activePresetId) || workspace.presets[0];
    return found ? found.href : workspace.entry;
  }

  function mountedIframes() {
    const out = [];
    if ((mode === 'pie' || splitPlay) && singleFrame?.iframe?.contentWindow) {
      out.push(singleFrame.iframe);
    } else if (mode === 'matrix') {
      for (const entry of matrixFrames) {
        if (entry.iframe?.contentWindow) out.push(entry.iframe);
      }
    }
    return out;
  }

  function postToWindow(win, payload) {
    if (!workspace || !win) return;
    try {
      win.postMessage(payload, workspace.origin);
    } catch {
      /* ignore detached frame */
    }
  }

  function broadcastToFrames(payload) {
    if (!workspace) return;
    for (const iframe of mountedIframes()) {
      postToWindow(iframe.contentWindow, payload);
    }
  }

  async function preloadProjectDocs() {
    const { assets, hostDocRoot } = getContext();
    const targets = (assets || []).filter((a) => a.kind === 'data' || a.kind === 'look');
    const validIds = new Set(targets.map((a) => a.id));
    for (const k of preloadedDatas.keys()) if (!validIds.has(k)) preloadedDatas.delete(k);
    for (const k of preloadedLooks.keys()) if (!validIds.has(k)) preloadedLooks.delete(k);

    const tasks = targets.map(async (asset) => {
      const store = asset.kind === 'data' ? preloadedDatas : preloadedLooks;
      const existing = store.get(asset.id);
      if (existing && existing.path === asset.path && existing.bytes === asset.bytes) return;
      const inMemory = hostDocRoot?.(asset.id);
      if (inMemory && typeof inMemory === 'object') {
        store.set(asset.id, { id: asset.id, path: asset.path, bytes: asset.bytes, root: structuredClone(inMemory) });
        return;
      }
      try {
        const res = await fetch(projectAssetUrl(asset.path));
        if (!res.ok) return;
        const root = await res.json();
        store.set(asset.id, { id: asset.id, path: asset.path, bytes: asset.bytes, root });
      } catch {
        /* ignore unreadable asset during preload */
      }
    });
    preloadPromise = Promise.all(tasks);
    await preloadPromise;
  }

  function collectDataSnapshot() {
    const { assets, datas, kinds } = getContext();
    const outDatas = {};
    const outSchemas = {};
    for (const asset of assets || []) {
      if (asset.kind !== 'data') continue;
      const openDoc = datas?.get(asset.id);
      const root = openDoc?.root || preloadedDatas.get(asset.id)?.root;
      if (!root || typeof root !== 'object') continue;
      const schemaName = root.schema || '';
      const schemaDef = kinds?.[schemaName];
      const effectiveValues = materializeDataValues(root.values, schemaDef);
      outDatas[asset.id] = {
        id: asset.id,
        path: openDoc?.path || asset.path,
        schema: schemaName,
        name: root.name || '',
        rev: openDoc?.rev ?? 0,
        values: effectiveValues,
        rawValues: structuredClone(root.values || {}),
      };
      if (schemaName && !(schemaName in outSchemas)) {
        outSchemas[schemaName] = effectiveValues;
      }
    }
    return { datas: outDatas, schemas: outSchemas };
  }

  function collectLookSnapshot() {
    const { assets, looks } = getContext();
    const outLooks = {};
    for (const asset of assets || []) {
      if (asset.kind !== 'look') continue;
      const openDoc = looks?.get(asset.id);
      const root = openDoc?.root || preloadedLooks.get(asset.id)?.root;
      if (!root || typeof root !== 'object') continue;
      outLooks[asset.id] = {
        id: asset.id,
        path: openDoc?.path || asset.path,
        rev: openDoc?.rev ?? 0,
        root: structuredClone(root),
      };
    }
    return outLooks;
  }

  /** Collects all project `.data.json` and `.look.json` roots (including open unsaved edits) for Live Stage. */
  async function collectLiveBundlePayload() {
    await preloadProjectDocs();
    const { assets, datas, looks, kinds } = getContext();
    const documents = {};
    const schemas = {};
    for (const asset of assets || []) {
      if (asset.kind === 'data') {
        const openDoc = datas?.get(asset.id);
        const root = openDoc?.root || preloadedDatas.get(asset.id)?.root;
        if (!root) continue;
        const path = openDoc?.path || asset.path;
        documents[path] = structuredClone(root);
        if (root.schema) {
          schemas[root.schema] = materializeDataValues(root.values, kinds?.[root.schema]);
        }
      } else if (asset.kind === 'look') {
        const openDoc = looks?.get(asset.id);
        const root = openDoc?.root || preloadedLooks.get(asset.id)?.root;
        if (!root) continue;
        const path = openDoc?.path || asset.path;
        documents[path] = structuredClone(root);
      }
    }
    return { documents, schemas };
  }

  function buildSyncPayload() {
    const { doc, time, weather, previewLook } = getContext();
    const { datas, schemas } = collectDataSnapshot();
    const looks = collectLookSnapshot();
    return {
      ganbateStudio: 1,
      type: 'studio:sync',
      datas,
      schemas,
      looks,
      scene: doc ? { id: doc.id, path: doc.path, rev: doc.rev, name: doc.root?.name || '' } : null,
      environment: {
        time,
        weather,
        look: typeof previewLook === 'function' ? previewLook() : {},
      },
      uiPick,
    };
  }

  function fitShellIntoBox(box, shell, width, height) {
    if (!box || !shell) return;
    if (!width || !height) {
      shell.style.width = '100%';
      shell.style.height = '100%';
      shell.style.transform = 'none';
      return;
    }
    const rect = box.getBoundingClientRect();
    const pad = 12;
    const availW = Math.max(40, rect.width - pad);
    const availH = Math.max(40, rect.height - pad);
    const scale = Math.min(1, availW / width, availH / height);
    shell.style.width = `${width}px`;
    shell.style.height = `${height}px`;
    shell.style.transform = `scale(${scale.toFixed(5)})`;
  }

  function updateFrameScales() {
    if (!workspace) return;
    if ((mode === 'pie' || splitPlay) && singleFrame) {
      const stage = stageEl();
      if (activeDeviceId === 'responsive') {
        fitShellIntoBox(stage, singleFrame.shell, 0, 0);
      } else {
        const dev = workspace.devices.find((d) => d.id === activeDeviceId);
        fitShellIntoBox(stage, singleFrame.shell, dev?.width || 0, dev?.height || 0);
      }
    } else if (mode === 'matrix') {
      for (const entry of matrixFrames) {
        fitShellIntoBox(entry.viewportBox, entry.shell, entry.device.width, entry.device.height);
      }
    }
    onModeChanged?.(mode);
  }

  function createGameIframe(deviceId) {
    const iframe = el('iframe', {
      class: 'pie-frame',
      'data-device-id': deviceId || 'responsive',
      referrerpolicy: 'no-referrer',
      sandbox: 'allow-forms allow-scripts allow-same-origin allow-downloads allow-popups',
      src: activePresetHref(),
    });
    iframe.addEventListener('load', () => {
      postToWindow(iframe.contentWindow, buildSyncPayload());
    });
    return iframe;
  }

  function mountSingleStage() {
    const stage = stageEl();
    if (!stage || !workspace) return;
    const href = activePresetHref();
    if (!singleFrame) {
      const iframe = createGameIframe(activeDeviceId);
      const shell = el('div', { class: 'pie-device-shell' }, iframe);
      stage.replaceChildren(shell);
      singleFrame = { shell, iframe, href };
    } else if (singleFrame.href !== href) {
      singleFrame.href = href;
      singleFrame.iframe.src = href;
    }
    singleFrame.iframe.dataset.deviceId = activeDeviceId;
    singleFrame.shell.classList.toggle('pie-framed-device', activeDeviceId !== 'responsive');
    updateFrameScales();
  }

  function mountMatrixStage() {
    const matrix = matrixEl();
    if (!matrix || !workspace) return;
    const href = activePresetHref();
    const devices = workspace.devices.slice(0, 4);
    const sameDevices =
      matrixFrames.length === devices.length &&
      matrixFrames.every((f, idx) => f.device.id === devices[idx].id);

    if (!sameDevices) {
      matrixFrames = devices.map((device) => {
        const iframe = createGameIframe(device.id);
        const shell = el('div', { class: 'pie-device-shell pie-framed-device' }, iframe);
        const viewportBox = el('div', { class: 'pie-matrix-viewport' }, shell);
        const tile = el('div', { class: 'pie-matrix-tile', 'data-matrix-device': device.id },
          el('header', { class: 'pie-matrix-header' },
            el('strong', {}, device.label),
            el('span', { class: 'pie-matrix-dims' }, `${device.width}×${device.height}`),
          ),
          viewportBox,
        );
        return { tile, viewportBox, shell, iframe, device, href };
      });
      matrix.replaceChildren(...matrixFrames.map((f) => f.tile));
    } else {
      for (const entry of matrixFrames) {
        if (entry.href !== href) {
          entry.href = href;
          entry.iframe.src = href;
        }
      }
    }
    updateFrameScales();
  }

  function syncToolbarUi() {
    const bar = pieToolbar();
    if (!bar) return;
    const available = Boolean(workspace || fallbackOrigin);
    bar.hidden = !available;
    if (!available) return;

    const sceneBtn = document.getElementById('view-mode-scene');
    const pieBtn = document.getElementById('view-mode-pie');
    const matrixBtn = document.getElementById('view-mode-matrix');
    const splitBtn = document.getElementById('pie-split-toggle');
    sceneBtn?.classList.toggle('on', mode === 'scene' && !splitPlay);
    pieBtn?.classList.toggle('on', mode === 'pie' && !splitPlay);
    matrixBtn?.classList.toggle('on', mode === 'matrix' && !splitPlay);
    if (splitBtn) {
      splitBtn.hidden = false;
      splitBtn.classList.toggle('on', splitPlay);
      splitBtn.setAttribute('aria-pressed', String(splitPlay));
    }

    const showPieControls = Boolean(workspace && (mode !== 'scene' || splitPlay));
    const presetSel = document.getElementById('pie-preset');
    if (presetSel) {
      presetSel.hidden = !showPieControls;
      if (workspace) {
        presetSel.replaceChildren(
          ...workspace.presets.map((p) => el('option', { value: p.id, selected: p.id === activePresetId }, p.label)),
        );
      }
    }

    const deviceSel = document.getElementById('pie-device');
    if (deviceSel) {
      deviceSel.hidden = !(workspace && (mode === 'pie' || splitPlay));
      if (workspace) {
        deviceSel.replaceChildren(
          el('option', { value: 'responsive', selected: activeDeviceId === 'responsive' }, t('pieResponsive')),
          ...workspace.devices.map((d) => el('option', { value: d.id, selected: d.id === activeDeviceId }, d.label)),
        );
      }
    }

    const inspectBtn = document.getElementById('pie-inspect-ui');
    if (inspectBtn) {
      inspectBtn.hidden = !showPieControls;
      inspectBtn.classList.toggle('on', uiPick);
      inspectBtn.setAttribute('aria-pressed', String(uiPick));
    }
  }

  function syncStageVisibility() {
    const vp = sceneViewportEl();
    const stage = stageEl();
    const matrix = matrixEl();
    const frame = previewFrameEl();
    const workbenchActive = Boolean(document.body?.classList?.contains('workbench-active'));

    document.body?.classList?.toggle('pie-split-active', splitPlay);
    frame?.classList?.toggle('pie-split-active', splitPlay);

    if (splitPlay) {
      if (vp) vp.hidden = workbenchActive;
      if (stage) stage.hidden = false;
      if (matrix) matrix.hidden = true;
      frame?.classList?.add('pie-active');
    } else {
      if (vp) vp.hidden = mode !== 'scene';
      if (stage) stage.hidden = mode !== 'pie';
      if (matrix) matrix.hidden = mode !== 'matrix';
      frame?.classList?.toggle('pie-active', mode !== 'scene');
    }
  }

  function setMode(nextMode) {
    if (nextMode !== 'scene' && !ensureActiveWorkspace()) return;
    if (splitPlay) {
      splitPlay = false;
    }
    mode = nextMode;
    syncStageVisibility();
    syncToolbarUi();
    if (mode === 'pie') {
      void preloadProjectDocs().then(() => {
        mountSingleStage();
        broadcastToFrames(buildSyncPayload());
      });
      mountSingleStage();
    } else if (mode === 'matrix') {
      void preloadProjectDocs().then(() => {
        mountMatrixStage();
        broadcastToFrames(buildSyncPayload());
      });
      mountMatrixStage();
    }
    onModeChanged?.(mode);
  }

  function setSplitPlay(next) {
    const want = Boolean(next);
    if (want) {
      if (!ensureActiveWorkspace()) return false;
      if (mode !== 'scene') {
        mode = 'scene';
      }
      splitPlay = true;
      syncStageVisibility();
      syncToolbarUi();
      void preloadProjectDocs().then(() => {
        if (splitPlay || mode === 'pie') {
          mountSingleStage();
          broadcastToFrames(buildSyncPayload());
        }
      });
      mountSingleStage();
      onModeChanged?.(mode);
      return true;
    }
    if (!splitPlay) return false;
    splitPlay = false;
    syncStageVisibility();
    syncToolbarUi();
    onModeChanged?.(mode);
    return false;
  }

  function toggleSplitPlay() {
    return setSplitPlay(!splitPlay);
  }

  function setFallbackOrigin(origin) {
    fallbackOrigin = typeof origin === 'string' && origin.trim() ? origin.trim() : null;
    syncToolbarUi();
  }

  function setPreset(presetId) {
    if (!ensureActiveWorkspace()) return false;
    if (!workspace.presets.some((p) => p.id === presetId)) return false;
    activePresetId = presetId;
    if (mode === 'pie' || splitPlay) mountSingleStage();
    else if (mode === 'matrix') mountMatrixStage();
    syncToolbarUi();
    return true;
  }

  function resolveDocIdByIdOrSchema(docId, schemaName) {
    const { assets, datas } = getContext();
    if (docId && (assets || []).some((a) => a.id === docId && a.kind === 'data')) {
      return docId;
    }
    if (schemaName) {
      for (const [id, openDoc] of datas?.entries() || []) {
        if (openDoc?.root?.schema === schemaName) return id;
      }
      for (const [id, cached] of preloadedDatas.entries()) {
        if (cached?.root?.schema === schemaName) return id;
      }
    }
    return null;
  }

  function handleWindowMessage(event) {
    if (!workspace || event.origin !== workspace.origin) return;
    const frames = mountedIframes();
    if (!frames.some((f) => f.contentWindow === event.source)) return;
    const msg = event.data;
    if (!msg || typeof msg !== 'object' || msg.ganbateGame !== 1 || typeof msg.type !== 'string') return;

    switch (msg.type) {
      case 'game:ready': {
        if (Array.isArray(msg.presets) && msg.presets.length) {
          try {
            const dynamic = msg.presets.map((p, idx) => {
              const safe = safeOriginPath(p.path ?? workspace.entryPath, workspace.origin, 'preset');
              return {
                id: typeof p.id === 'string' && p.id.trim() ? p.id.trim() : `preset-${idx}`,
                label: typeof p.label === 'string' && p.label.trim() ? p.label.trim() : safe.path,
                path: safe.path,
                href: safe.href,
              };
            });
            workspace.presets = mergeBuiltinGameplayPresets(dynamic, workspace.origin);
            syncToolbarUi();
          } catch (err) {
            log?.(1, String(err.message || err));
          }
        }
        postToWindow(event.source, buildSyncPayload());
        if (preloadPromise) {
          preloadPromise.then(() => postToWindow(event.source, buildSyncPayload()));
        }
        break;
      }
      case 'game:select-data': {
        void preloadProjectDocs().then(() => {
          const targetId = resolveDocIdByIdOrSchema(msg.docId, msg.schema);
          if (!targetId) return;
          const path = Array.isArray(msg.path) ? msg.path.map((p) => String(p)) : null;
          onSelectData?.(targetId, path);
        });
        break;
      }
      case 'game:edit-data': {
        if (!Array.isArray(msg.ops) || !msg.ops.length) break;
        void preloadProjectDocs().then(() => {
          const targetId = resolveDocIdByIdOrSchema(msg.docId, msg.schema);
          if (!targetId) return;
          onEditDataFromGame?.({
            docId: targetId,
            label: typeof msg.label === 'string' && msg.label.trim() ? msg.label.trim() : 'PIE Edit',
            merge: typeof msg.merge === 'string' && msg.merge ? msg.merge : undefined,
            ops: msg.ops,
          });
        });
        break;
      }
      case 'game:select-entity': {
        if (typeof msg.entityId === 'string') {
          onSelectEntity?.(msg.entityId);
        }
        break;
      }
      default:
        break;
    }
  }

  async function loadWorkspaces(project, advertisedWorkspaces, directManifest = null) {
    const gen = ++loadGeneration;
    const originHref = studioOriginHref();
    const params = new URLSearchParams(globalThis.location?.search || '');
    const rawOverride = params.get('gameOrigin') || params.get('toolOrigin');
    const allowSameOrigin = Boolean(getContext().allowSameOrigin);
    const override = normalizeLoopbackQueryOverride(rawOverride, originHref, { allowSameOrigin });

    if (directManifest && typeof directManifest === 'object') {
      try {
        workspace = resolveGameWorkspace(directManifest, originHref, override, { allowSameOrigin });
        if (!workspace.presets.some((p) => p.id === activePresetId)) {
          activePresetId = workspace.presets[0]?.id || 'default';
        }
        void preloadProjectDocs().then(() => {
          if (mode !== 'scene' || splitPlay) broadcastToFrames(buildSyncPayload());
        });
        if (mode === 'pie' || splitPlay) mountSingleStage();
        else if (mode === 'matrix') mountMatrixStage();
        syncToolbarUi();
        onWorkspaceLoaded?.(workspace);
      } catch (err) {
        workspace = null;
        if (splitPlay) setSplitPlay(false);
        if (mode !== 'scene') setMode('scene');
        syncToolbarUi();
        onWorkspaceLoaded?.(null);
        log?.(2, String(err.message || err));
      }
      return;
    }

    const paths = Array.isArray(advertisedWorkspaces)
      ? advertisedWorkspaces
      : override
        ? (project?.editor || []).map((dir) => `${dir}/studio-game.json`)
        : [];

    if (!paths.length) {
      workspace = null;
      if (splitPlay) setSplitPlay(false);
      if (mode !== 'scene') setMode('scene');
      syncToolbarUi();
      onWorkspaceLoaded?.(null);
      return;
    }

    try {
      let resolved = null;
      for (const relPath of paths) {
        const res = await fetch(projectAssetUrl(relPath));
        if (res.status === 404) continue;
        if (!res.ok) throw new Error(`Could not load ${relPath}`);
        const rawManifest = await res.json();
        resolved = resolveGameWorkspace(rawManifest, originHref, override, { allowSameOrigin });
        break;
      }
      if (gen !== loadGeneration) return;
      workspace = resolved;
      if (workspace) {
        if (!workspace.presets.some((p) => p.id === activePresetId)) {
          activePresetId = workspace.presets[0]?.id || 'default';
        }
        void preloadProjectDocs().then(() => {
          if (mode !== 'scene' || splitPlay) broadcastToFrames(buildSyncPayload());
        });
        if (mode === 'pie' || splitPlay) mountSingleStage();
        else if (mode === 'matrix') mountMatrixStage();
      } else {
        if (splitPlay) setSplitPlay(false);
        if (mode !== 'scene') setMode('scene');
      }
      syncToolbarUi();
      onWorkspaceLoaded?.(workspace);
    } catch (err) {
      if (gen !== loadGeneration) return;
      workspace = null;
      if (splitPlay) setSplitPlay(false);
      if (mode !== 'scene') setMode('scene');
      syncToolbarUi();
      onWorkspaceLoaded?.(null);
      log?.(2, String(err.message || err));
    }
  }

  function notifyDocUpdate(doc, ops = null) {
    if (!doc) return;
    if (doc.kind === 'data' && doc.root) {
      preloadedDatas.set(doc.id, { id: doc.id, path: doc.path, root: structuredClone(doc.root) });
    } else if (doc.kind === 'look' && doc.root) {
      preloadedLooks.set(doc.id, { id: doc.id, path: doc.path, root: structuredClone(doc.root) });
    }
    if (!workspace || (mode === 'scene' && !splitPlay)) return;
    const { kinds } = getContext();
    const schemaName = doc.root?.schema || null;
    const effectiveValues = doc.kind === 'data'
      ? materializeDataValues(doc.root?.values, kinds?.[schemaName])
      : undefined;
    broadcastToFrames({
      ganbateStudio: 1,
      type: 'studio:doc-update',
      kind: doc.kind,
      doc: doc.id,
      path: doc.path,
      rev: doc.rev,
      schema: schemaName,
      name: doc.root?.name ?? null,
      values: effectiveValues,
      rawValues: doc.kind === 'data' ? structuredClone(doc.root?.values || {}) : undefined,
      root: doc.kind !== 'data' ? doc.root : undefined,
      ops: ops || null,
    });
  }

  function notifyEnvironmentChanged() {
    if (!workspace || (mode === 'scene' && !splitPlay)) return;
    broadcastToFrames(buildSyncPayload());
  }

  function sendClock({ playing, time, speed }) {
    if (!workspace || (mode === 'scene' && !splitPlay)) return;
    broadcastToFrames({
      ganbateStudio: 1,
      type: 'studio:clock',
      playing: !!playing,
      time: Number(time) || 0,
      speed: Number(speed) || 1,
    });
  }

  function sendCue({ cueId, track, payload }) {
    if (!workspace || (mode === 'scene' && !splitPlay)) return;
    broadcastToFrames({
      ganbateStudio: 1,
      type: 'studio:cue',
      cueId,
      track,
      payload: payload ?? null,
    });
  }

  function setUiPick(next) {
    uiPick = !!next;
    syncToolbarUi();
    if (!workspace || (mode === 'scene' && !splitPlay)) return;
    broadcastToFrames({
      ganbateStudio: 1,
      type: 'studio:ui-pick',
      active: uiPick,
    });
  }

  /** Bounding client rect of the active layout target (single PIE frame or 3D `#viewport`). */
  function activeLayoutTargetElement() {
    if (mode === 'pie' && !splitPlay && singleFrame?.shell) return singleFrame.shell;
    if (mode === 'scene') return sceneViewportEl();
    return null;
  }

  function setupEvents() {
    globalThis.window?.addEventListener?.('message', handleWindowMessage);

    document.getElementById('view-mode-scene')?.addEventListener('click', () => setMode('scene'));
    document.getElementById('view-mode-pie')?.addEventListener('click', () => setMode('pie'));
    document.getElementById('view-mode-matrix')?.addEventListener('click', () => setMode('matrix'));
    document.getElementById('pie-split-toggle')?.addEventListener('click', () => toggleSplitPlay());

    document.getElementById('pie-preset')?.addEventListener('change', (ev) => {
      activePresetId = ev.target.value;
      if (mode === 'pie' || splitPlay) mountSingleStage();
      else if (mode === 'matrix') mountMatrixStage();
    });

    document.getElementById('pie-device')?.addEventListener('change', (ev) => {
      activeDeviceId = ev.target.value;
      if (mode === 'pie' || splitPlay) mountSingleStage();
    });

    document.getElementById('pie-inspect-ui')?.addEventListener('click', () => {
      setUiPick(!uiPick);
    });

    if (typeof ResizeObserver !== 'undefined') {
      resizeObserver = new ResizeObserver(() => updateFrameScales());
      const frame = previewFrameEl();
      if (frame) resizeObserver.observe(frame);
    }
    globalThis.window?.addEventListener?.('resize', updateFrameScales);
  }

  /** Converts relative `game:edit-data` ops into `Document` ops on `doc.root`. */
  function translateRelativeGameOps(doc, schemaDef, relativeOps) {
    const out = [];
    // Apply sequentially against a working clone of `values` so multiple array mutations in one batch compose!
    const workingValues = structuredClone(doc.root?.values || {});
    for (const op of relativeOps) {
      if (!op || typeof op !== 'object') continue;
      if (Array.isArray(op.set) && op.set.length) {
        const built = buildDataValueOps(workingValues, schemaDef, op.set, op.value, false);
        for (const b of built) {
          out.push(b);
          if (b.set && b.set[0] === 'values') {
            setDeepObject(workingValues, b.set.slice(1), structuredClone(b.value));
          }
        }
      } else if (Array.isArray(op.remove) && op.remove.length) {
        const built = buildDataValueOps(workingValues, schemaDef, op.remove, undefined, true);
        for (const b of built) {
          out.push(b);
          if (b.remove && b.remove[0] === 'values') {
            removeDeepObject(workingValues, b.remove.slice(1));
          } else if (b.set && b.set[0] === 'values') {
            setDeepObject(workingValues, b.set.slice(1), structuredClone(b.value));
          }
        }
      }
    }
    return out;
  }

  return {
    setupEvents,
    loadWorkspaces,
    preloadProjectDocs,
    collectLiveBundlePayload,
    notifyDocUpdate,
    notifyEnvironmentChanged,
    sendClock,
    sendCue,
    setUiPick,
    setMode,
    getMode: () => mode,
    isSplitPlay: () => splitPlay,
    setSplitPlay,
    toggleSplitPlay,
    setFallbackOrigin,
    setPreset,
    getActivePresetId: () => activePresetId,
    syncStageVisibility,
    getWorkspace: () => workspace,
    activeLayoutTargetElement,
    translateRelativeGameOps,
    syncToolbarUi,
  };
}

function setDeepObject(obj, keys, val) {
  if (!keys.length) return;
  let cur = obj;
  for (let i = 0; i < keys.length - 1; i++) {
    const k = keys[i];
    if (!cur[k] || typeof cur[k] !== 'object' || Array.isArray(cur[k])) cur[k] = {};
    cur = cur[k];
  }
  cur[keys[keys.length - 1]] = val;
}

function removeDeepObject(obj, keys) {
  if (!keys.length) return;
  let cur = obj;
  for (let i = 0; i < keys.length - 1; i++) {
    cur = cur?.[keys[i]];
    if (!cur || typeof cur !== 'object') return;
  }
  delete cur[keys[keys.length - 1]];
}
