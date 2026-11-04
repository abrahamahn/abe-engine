// Asset library logic and Content Browser UI for Ganbate Studio.
// Pure helpers (`buildFolderTree`, `filterAssets`, `kindCounts`, `placementEntity`,
// `raycastDropPoint`, `buildRenamedPath`, `formatBytes`) stay free of DOM globals so they can be
// unit-tested in Node.
import {
  createPreviewManager,
  drawGlbPreviewCanvas,
  drawLookPreviewCanvas,
  drawMaterialPreviewCanvas,
  drawWaveformCanvas,
} from './asset-previews.js';
import { orbitBasis } from './cameras.js';
import * as M from './math.js';

export const ASSET_DRAG_MIME = 'application/x-ganbate-asset';
export const KIND_ORDER = Object.freeze([
  'scene', 'prefab', 'model', 'look', 'data', 'material', 'texture', 'sound',
  'soundbank', 'character', 'seq', 'vfx',
]);

const SUFFIXES = [
  '.scene.json', '.prefab.json', '.material.json', '.data.json', '.look.json',
  '.soundbank.json', '.character.json', '.seq.json', '.vfx.json',
  '.glb', '.png', '.jpg', '.jpeg', '.webp', '.wav', '.ogg',
];

export function projectAssetUrl(relPath) {
  return '/project/' + relPath.split('/').map(encodeURIComponent).join('/');
}

export function assetBasename(relPath) {
  const idx = relPath.lastIndexOf('/');
  return idx >= 0 ? relPath.slice(idx + 1) : relPath;
}

export function assetFolder(relPath) {
  const idx = relPath.lastIndexOf('/');
  return idx >= 0 ? relPath.slice(idx + 1).length === 0 ? relPath.slice(0, idx) : relPath.slice(0, idx) : '';
}

export function assetExtension(relPath) {
  const base = assetBasename(relPath);
  const lower = base.toLowerCase();
  for (const suffix of SUFFIXES) {
    if (lower.endsWith(suffix) && base.length > suffix.length) {
      return base.slice(base.length - suffix.length);
    }
  }
  const dot = base.lastIndexOf('.');
  return dot > 0 ? base.slice(dot) : '';
}

export function assetStem(relPath) {
  const base = assetBasename(relPath);
  const ext = assetExtension(relPath);
  return ext ? base.slice(0, base.length - ext.length) : base;
}

/** Builds a clean relative path for renaming or moving `oldPath`, preserving compound suffixes
 * such as `.scene.json` or `.look.json` when the user types a bare stem or folder path. */
export function buildRenamedPath(oldPath, rawInput) {
  if (typeof rawInput !== 'string') return null;
  const normalized = rawInput.trim().replace(/\\/g, '/').replace(/^\/+/, '');
  if (!normalized) return null;
  const ext = assetExtension(oldPath);
  const extLower = ext.toLowerCase();

  if (normalized.endsWith('/')) {
    return `${normalized.replace(/\/+$/, '')}/${assetBasename(oldPath)}`;
  }
  if (normalized.includes('/')) {
    const lower = normalized.toLowerCase();
    if (extLower && !lower.endsWith(extLower) && !assetBasename(normalized).includes('.')) {
      return normalized + ext;
    }
    return normalized;
  }
  const dir = assetFolder(oldPath);
  const fileName = extLower && normalized.toLowerCase().endsWith(extLower)
    ? normalized
    : normalized + ext;
  return dir ? `${dir}/${fileName}` : fileName;
}

export function formatBytes(bytes) {
  const n = Number(bytes);
  if (!Number.isFinite(n) || n <= 0) return '0 B';
  if (n < 1024) return `${Math.round(n)} B`;
  if (n < 1024 * 1024) {
    const kb = n / 1024;
    return `${kb >= 10 ? Math.round(kb) : kb.toFixed(1)} KB`;
  }
  const mb = n / (1024 * 1024);
  return `${mb >= 10 ? Math.round(mb) : mb.toFixed(1)} MB`;
}

export function inFolder(assetPath, folder) {
  if (!folder) return true;
  return assetPath === folder || assetPath.startsWith(folder + '/');
}

/** Builds a sorted folder tree from `assets` and `contentRoots`, compacting empty single-child
 * wrapper directories (such as `public` -> `games` -> `public/games`) unless `preserveFolder`
 * names the intermediate path. */
export function buildFolderTree(assets, contentRoots = [], preserveFolder = '') {
  const nodes = new Map();
  const ensure = (path) => {
    if (!path) return null;
    if (nodes.has(path)) return nodes.get(path);
    const parts = path.split('/');
    let acc = '';
    let prev = null;
    for (const part of parts) {
      acc = acc ? `${acc}/${part}` : part;
      let n = nodes.get(acc);
      if (!n) {
        n = { path: acc, name: part, directCount: 0, count: 0, childMap: new Map() };
        nodes.set(acc, n);
        if (prev) prev.childMap.set(acc, n);
      }
      prev = n;
    }
    return prev;
  };

  for (const root of contentRoots) {
    if (typeof root === 'string' && root.trim()) ensure(root.trim().replace(/^\/+|\/+$/g, ''));
  }
  for (const asset of assets) {
    const dir = assetFolder(asset.path);
    if (!dir) continue;
    const leaf = ensure(dir);
    if (leaf) leaf.directCount += 1;
    const parts = dir.split('/');
    let acc = '';
    for (const part of parts) {
      acc = acc ? `${acc}/${part}` : part;
      nodes.get(acc).count += 1;
    }
  }

  const roots = [...nodes.values()]
    .filter((n) => !n.path.includes('/'))
    .sort((a, b) => a.name.localeCompare(b.name));

  const finalize = (node) => {
    let cur = node;
    let label = cur.name;
    while (
      cur.directCount === 0 &&
      cur.childMap.size === 1 &&
      cur.path !== preserveFolder
    ) {
      const [onlyChild] = cur.childMap.values();
      label = `${label}/${onlyChild.name}`;
      cur = onlyChild;
    }
    const children = [...cur.childMap.values()]
      .sort((a, b) => a.name.localeCompare(b.name))
      .map(finalize);
    return {
      path: cur.path,
      name: label,
      count: cur.count,
      directCount: cur.directCount,
      children,
    };
  };

  return roots.map(finalize);
}

function kindRank(kind) {
  const idx = KIND_ORDER.indexOf(kind);
  return idx >= 0 ? idx : KIND_ORDER.length;
}

export function filterAssets(
  assets,
  {
    folder = '',
    collection = 'all',
    kind = 'all',
    query = '',
    sort = 'kind',
    favorites = null,
    recent = null,
    usageCounts = null,
  } = {},
) {
  const tokens = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  const favSet = favorites instanceof Set ? favorites : Array.isArray(favorites) ? new Set(favorites) : null;
  const recentArr = Array.isArray(recent) ? recent : null;

  const filtered = assets.filter((a) => {
    if (collection === 'favorites') {
      if (!favSet || !favSet.has(a.id)) return false;
    } else if (collection === 'recent') {
      if (!recentArr || !recentArr.includes(a.id)) return false;
    } else if (collection === 'unused') {
      if (!usageCounts || (usageCounts[a.id] || 0) > 0) return false;
    } else if (folder && !inFolder(assetFolder(a.path), folder)) {
      return false;
    }
    if (kind && kind !== 'all' && a.kind !== kind) return false;
    if (tokens.length) {
      const hay = `${a.path} ${a.kind} ${a.id}`.toLowerCase();
      for (const tok of tokens) {
        if (!hay.includes(tok)) return false;
      }
    }
    return true;
  });

  if (collection === 'recent' && sort === 'kind' && recentArr) {
    const order = new Map(recentArr.map((id, idx) => [id, idx]));
    return filtered.sort((a, b) => (order.get(a.id) ?? 999) - (order.get(b.id) ?? 999));
  }

  return filtered.sort((a, b) => {
    if (sort === 'name') {
      return assetBasename(a.path).localeCompare(assetBasename(b.path)) || a.path.localeCompare(b.path);
    }
    if (sort === 'folder') {
      return assetFolder(a.path).localeCompare(assetFolder(b.path)) || assetBasename(a.path).localeCompare(assetBasename(b.path));
    }
    if (sort === 'size') {
      const diff = (b.bytes || 0) - (a.bytes || 0);
      return diff !== 0 ? diff : a.path.localeCompare(b.path);
    }
    const kr = kindRank(a.kind) - kindRank(b.kind);
    return kr !== 0 ? kr : a.path.localeCompare(b.path);
  });
}

export function kindCounts(
  assets,
  { folder = '', collection = 'all', query = '', favorites = null, recent = null, usageCounts = null } = {},
) {
  const scoped = filterAssets(assets, { folder, collection, kind: 'all', query, favorites, recent, usageCounts });
  const counts = { all: scoped.length };
  for (const k of KIND_ORDER) counts[k] = 0;
  for (const a of scoped) counts[a.kind] = (counts[a.kind] || 0) + 1;
  return counts;
}

export function canPlaceAsset(asset, doc) {
  if (!asset || (asset.kind !== 'model' && asset.kind !== 'prefab')) {
    return { ok: false, reason: null };
  }
  if (!doc || doc.unavailable || (doc.kind !== 'scene' && doc.kind !== 'prefab')) {
    return { ok: false, reason: 'openSceneToPlace' };
  }
  if (asset.kind === 'prefab' && asset.id === doc.id) {
    return { ok: false, reason: 'cannotPlaceSelf' };
  }
  return { ok: true, reason: null };
}

export function placementEntity(asset, { parent = null, localTranslation = null } = {}) {
  const transform = {};
  if (Array.isArray(localTranslation)) {
    const clean = localTranslation.map((v) => M.tidy(v, 3));
    if (clean.some((v) => Math.abs(v) >= 1e-5)) transform.translation = clean;
  }
  const components = { transform };
  const entity = { name: assetStem(asset.path) || asset.kind, components };
  if (parent) entity.parent = parent;
  if (asset.kind === 'model') {
    components.mesh = { model: asset.id };
  } else if (asset.kind === 'prefab') {
    entity.prefab = { asset: asset.id };
  }
  return entity;
}

/** Projects a viewport CSS pixel `(clientX, clientY)` onto the horizontal plane near the orbit
 * target (or onto the orbit target plane when looking nearly horizontally). */
export function raycastDropPoint(view, clientX, clientY, rect) {
  if (!rect || !(rect.width > 0) || !(rect.height > 0)) {
    return (view.target || [0, 0, 0]).map((v) => M.tidy(v, 3));
  }
  const { back, up, right } = orbitBasis(view);
  const dist = view.distance ?? 10;
  const target = view.target || [0, 0, 0];
  const eye = target.map((v, i) => v + dist * back[i]);
  const shift = view.shift || [0, 0];
  const nx = ((clientX - rect.left) / rect.width) * 2 - 1 - (shift[0] || 0);
  const ny = 1 - ((clientY - rect.top) / rect.height) * 2 - (shift[1] || 0);
  const aspect = rect.width / rect.height;
  const tanHalf = Math.tan(((view.fov ?? 50) * Math.PI) / 360);
  const dir = [0, 1, 2].map((i) => -back[i] + right[i] * (nx * aspect * tanHalf) + up[i] * (ny * tanHalf));
  const planeY = Math.abs(target[1]) <= 1.5 ? 0 : target[1];
  if (Math.abs(dir[1]) > 0.04) {
    const hitT = (planeY - eye[1]) / dir[1];
    if (hitT > 0.1 && hitT <= Math.max(50, dist * 6)) {
      return [
        M.tidy(eye[0] + dir[0] * hitT, 3),
        M.tidy(planeY, 3),
        M.tidy(eye[2] + dir[2] * hitT, 3),
      ];
    }
  }
  return [0, 1, 2].map((i) => M.tidy(
    target[i] + (right[i] * nx * aspect + up[i] * ny) * tanHalf * dist,
    3,
  ));
}

// ---- SVG Vector Previews ----------------------------------------------------------------------

const SVG_NS = 'http://www.w3.org/2000/svg';
function svgEl(tag, attrs = {}, ...children) {
  const node = document.createElementNS(SVG_NS, tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (v !== undefined && v !== null) node.setAttribute(k, String(v));
  }
  for (const c of children) if (c) node.append(c);
  return node;
}

function kindGraphic(kind) {
  const base = { viewBox: '0 0 32 32', fill: 'none', stroke: 'currentColor', 'stroke-width': '1.7', 'stroke-linecap': 'round', 'stroke-linejoin': 'round', class: 'asset-svg', 'aria-hidden': 'true' };
  switch (kind) {
    case 'scene':
      return svgEl('svg', base,
        svgEl('rect', { x: 4, y: 6, width: 24, height: 20, rx: 3 }),
        svgEl('circle', { cx: 21.5, cy: 12.5, r: 2.3 }),
        svgEl('path', { d: 'M4 22l6.5-6 5 4.5 4.5-3.5 8 6' }));
    case 'prefab':
      return svgEl('svg', base,
        svgEl('path', { d: 'M16 4l10 5.5v11L16 26 6 20.5v-11L16 4z' }),
        svgEl('path', { d: 'M6 9.5L16 15l10-5.5M16 15v11' }),
        svgEl('path', { d: 'M11 6.8l10 5.5', opacity: '0.55' }));
    case 'model':
      return svgEl('svg', base,
        svgEl('path', { d: 'M16 4.5l9.5 5.2v12.6L16 27.5l-9.5-5.2V9.7L16 4.5z' }),
        svgEl('path', { d: 'M6.5 9.7L16 15l9.5-5.3M16 15v12.5' }));
    case 'look':
      return svgEl('svg', base,
        svgEl('circle', { cx: 16, cy: 14, r: 5 }),
        svgEl('path', { d: 'M16 5v2.2M9.5 7.8l1.6 1.6M22.5 7.8l-1.6 1.6M5 23h22M8 26.5h16' }));
    case 'data':
      return svgEl('svg', base,
        svgEl('rect', { x: 6, y: 5, width: 20, height: 22, rx: 2.5 }),
        svgEl('path', { d: 'M10.5 11h11M10.5 16h11M10.5 21h7' }));
    case 'soundbank':
      return svgEl('svg', base,
        svgEl('rect', { x: 5, y: 6, width: 22, height: 20, rx: 3 }),
        svgEl('path', { d: 'M9 19v-6M13 21V11M17 18v-4M21 20v-8' }));
    case 'character':
      return svgEl('svg', base,
        svgEl('circle', { cx: 16, cy: 8.5, r: 3.2 }),
        svgEl('path', { d: 'M9 15h14M16 11.7V21M16 21l-4 6M16 21l4 6' }));
    case 'seq':
      return svgEl('svg', base,
        svgEl('rect', { x: 4, y: 6, width: 24, height: 20, rx: 2.5 }),
        svgEl('path', { d: 'M8 12h8M14 16h10M10 20h7M13 6v20' }));
    case 'vfx':
      return svgEl('svg', base,
        svgEl('path', { d: 'M16 5l2.4 6.6L25 14l-6.6 2.4L16 23l-2.4-6.6L7 14l6.6-2.4L16 5z' }),
        svgEl('circle', { cx: 24, cy: 23, r: 1.6 }));
    case 'material':
      return svgEl('svg', base,
        svgEl('circle', { cx: 16, cy: 16, r: 10 }),
        svgEl('path', { d: 'M12 11.5a4.5 4.5 0 0 1 5-1', opacity: '0.85' }),
        svgEl('path', { d: 'M7 18.5c3.5 2.5 14.5 2.5 18 0', opacity: '0.5' }));
    case 'sound':
      return svgEl('svg', base,
        svgEl('path', { d: 'M6 16v0M10 12v8M14 8v16M18 11v10M22 9v14M26 14v4' }));
    default:
      return svgEl('svg', base,
        svgEl('rect', { x: 5, y: 5, width: 22, height: 22, rx: 3 }),
        svgEl('path', { d: 'M5 21l6-6 4.5 4.5 3.5-3.5 8 7' }));
  }
}

// ---- Content Browser Controller ---------------------------------------------------------------

function readPref(key, fallback) {
  try {
    return localStorage.getItem(key) || fallback;
  } catch {
    return fallback;
  }
}
function writePref(key, value) {
  try { localStorage.setItem(key, value); } catch { /* ignore */ }
}
function readJsonPref(key, fallback) {
  try {
    const raw = localStorage.getItem(key);
    return raw ? JSON.parse(raw) : fallback;
  } catch {
    return fallback;
  }
}
function writeJsonPref(key, value) {
  try { localStorage.setItem(key, JSON.stringify(value)); } catch { /* ignore */ }
}

export function createAssetLibrary({
  el,
  t,
  getContext,
  onOpenDoc,
  onInspectLook,
  onInspectData,
  onOpenWorkbenchDoc,
  onImportModel,
  onPlaceAsset,
  onMoveAsset,
  onImportFiles,
  onRequestUsage,
  onJumpToReference,
  onStartDrag,
  onEndDrag,
}) {
  const savedScale = Math.max(112, Math.min(220, Number(readPref('ganbate.studio.assetScale', '152')) || 152));
  const savedSort = readPref('ganbate.studio.assetSort', 'kind');
  const ui = {
    folder: '',
    collection: 'all', // 'all' | 'favorites' | 'recent' | 'unused'
    kind: 'all',
    query: '',
    sort: ['kind', 'name', 'folder', 'size'].includes(savedSort) ? savedSort : 'kind',
    scale: savedScale,
    detailsOpen: readPref('ganbate.studio.assetDetails', '1') !== '0',
    view: readPref('ganbate.studio.assetView', 'grid') === 'list' ? 'list' : 'grid',
    selectedId: null,
    collapsedFolders: new Set(),
    playingId: null,
    audio: null,
    copiedTimer: 0,
    copiedId: null,
    projectKey: '',
    favorites: new Set(),
    recent: [],
    usageCounts: {},
    usageReady: false,
    usesByAsset: new Map(),
    turntable: { yaw: 0.68, pitch: 0.42 },
  };

  function applyScaleCss() {
    if (typeof document === 'undefined') return;
    const thumbH = Math.round(ui.scale * 0.48);
    document.documentElement.style.setProperty('--asset-card-min', `${ui.scale}px`);
    document.documentElement.style.setProperty('--asset-thumb-h', `${thumbH}px`);
  }

  const previews = createPreviewManager({
    urlForAsset: projectAssetUrl,
    onMetadata: (assetId) => {
      if (ui.selectedId === assetId) drawAssetInspector();
    },
  });

  function syncProjectPrefs(project) {
    const pid = project?.id || 'default';
    if (ui.projectKey === pid) return;
    ui.projectKey = pid;
    const favArr = readJsonPref(`ganbate.studio.favs.${pid}`, []);
    ui.favorites = new Set(Array.isArray(favArr) ? favArr : []);
    const recArr = readJsonPref(`ganbate.studio.recent.${pid}`, []);
    ui.recent = Array.isArray(recArr) ? recArr : [];
  }

  function toggleFavorite(id) {
    if (!id) return;
    if (ui.favorites.has(id)) ui.favorites.delete(id);
    else ui.favorites.add(id);
    if (ui.projectKey) writeJsonPref(`ganbate.studio.favs.${ui.projectKey}`, [...ui.favorites]);
    draw();
  }

  function recordRecent(id) {
    if (!id) return;
    ui.recent = [id, ...ui.recent.filter((x) => x !== id)].slice(0, 16);
    if (ui.projectKey) writeJsonPref(`ganbate.studio.recent.${ui.projectKey}`, ui.recent);
  }

  function setUsageCounts(counts) {
    ui.usageCounts = counts && typeof counts === 'object' ? counts : {};
    ui.usageReady = true;
    drawFoldersFromContext();
    if (ui.collection === 'unused') draw();
    else drawAssetInspector();
  }

  function setAssetUses(id, uses) {
    ui.usesByAsset.set(id, Array.isArray(uses) ? uses : []);
    ui.usageCounts[id] = Array.isArray(uses) ? uses.length : 0;
    if (ui.selectedId === id) drawAssetInspector();
    drawFoldersFromContext();
  }

  function notifyAssetMoved(id) {
    ui.selectedId = id;
    onRequestUsage?.(id);
    draw();
  }

  function stopAudio() {
    if (ui.audio) {
      ui.audio.pause();
      ui.audio = null;
    }
    if (ui.playingId) {
      ui.playingId = null;
      draw();
    }
  }

  function toggleSound(asset) {
    if (ui.playingId === asset.id) {
      stopAudio();
      return;
    }
    recordRecent(asset.id);
    if (ui.audio) {
      ui.audio.pause();
      ui.audio = null;
    }
    const audio = new Audio(projectAssetUrl(asset.path));
    ui.playingId = asset.id;
    ui.audio = audio;
    const clearIfSelf = () => {
      if (ui.audio === audio) {
        ui.audio = null;
        ui.playingId = null;
        draw();
      }
    };
    audio.addEventListener('ended', clearIfSelf);
    audio.addEventListener('error', clearIfSelf);
    audio.play().catch(clearIfSelf);
    draw();
  }

  function makePlaceControl(asset, doc) {
    const check = canPlaceAsset(asset, doc);
    const disabled = !check.ok;
    const tip = check.reason ? t(check.reason) : t('placeInSceneTip');
    const activate = (ev) => {
      ev.stopPropagation();
      if (disabled) return;
      recordRecent(asset.id);
      onPlaceAsset(asset);
    };
    // Rendered as an accessible role="button" chip so `#assets-list [data-id] button`
    // continues to uniquely address each row's document/import button.
    return el('span', {
      role: 'button',
      tabindex: disabled ? '-1' : '0',
      class: `asset-place${disabled ? ' disabled' : ''}`,
      'aria-disabled': disabled ? 'true' : null,
      title: tip,
      onclick: activate,
      onkeydown: (ev) => {
        if (ev.key === 'Enter' || ev.key === ' ') {
          ev.preventDefault();
          activate(ev);
        }
      },
    }, '+ ' + t('placeInScene'));
  }

  function makeFavoriteChip(asset) {
    const fav = ui.favorites.has(asset.id);
    const activate = (ev) => {
      ev.stopPropagation();
      toggleFavorite(asset.id);
    };
    return el('span', {
      role: 'button',
      tabindex: '0',
      class: `asset-fav${fav ? ' on' : ''}`,
      title: fav ? t('unfavoriteAsset') : t('favoriteAsset'),
      'aria-pressed': String(fav),
      onclick: activate,
      onkeydown: (ev) => {
        if (ev.key === 'Enter' || ev.key === ' ') {
          ev.preventDefault();
          activate(ev);
        }
      },
    }, fav ? '★' : '☆');
  }

  const WORKBENCH_BY_KIND = {
    material: 'material',
    soundbank: 'audio',
    character: 'character',
    seq: 'sequencer',
    vfx: 'vfx',
  };

  function primaryAction(asset, doc) {
    if (asset.kind === 'scene' || asset.kind === 'prefab') {
      return () => {
        recordRecent(asset.id);
        onOpenDoc(asset.id);
      };
    }
    if (asset.kind === 'look') {
      return () => {
        recordRecent(asset.id);
        onInspectLook(asset.id);
      };
    }
    if (asset.kind === 'data') {
      return () => {
        recordRecent(asset.id);
        onInspectData?.(asset.id);
      };
    }
    if (WORKBENCH_BY_KIND[asset.kind] && onOpenWorkbenchDoc) {
      const wb = WORKBENCH_BY_KIND[asset.kind];
      return () => {
        recordRecent(asset.id);
        onOpenWorkbenchDoc(wb, asset.id);
      };
    }
    if (asset.kind === 'model') {
      return () => {
        recordRecent(asset.id);
        if (canPlaceAsset(asset, doc).ok) onPlaceAsset(asset);
        else onImportModel(asset);
      };
    }
    if (asset.kind === 'sound') return () => toggleSound(asset);
    return null;
  }

  function promptRenameAsset(asset) {
    if (!asset || !onMoveAsset) return;
    const stem = assetStem(asset.path);
    const answer = prompt(t('renameAssetPrompt', assetBasename(asset.path)), stem);
    if (answer === null) return;
    const target = buildRenamedPath(asset.path, answer);
    if (target && target !== asset.path) onMoveAsset(asset, target);
  }

  function promptMoveAsset(asset) {
    if (!asset || !onMoveAsset) return;
    const answer = prompt(t('moveAssetPrompt', assetBasename(asset.path)), asset.path);
    if (answer === null) return;
    const target = buildRenamedPath(asset.path, answer);
    if (target && target !== asset.path) onMoveAsset(asset, target);
  }

  function wireDrag(node, asset) {
    node.setAttribute('draggable', 'true');
    node.addEventListener('dragstart', (ev) => {
      closeContextMenu();
      if (ev.dataTransfer) {
        ev.dataTransfer.effectAllowed = 'copyMove';
        ev.dataTransfer.setData(ASSET_DRAG_MIME, asset.id);
        ev.dataTransfer.setData('text/plain', asset.path);
      }
      onStartDrag?.(asset);
      node.classList.add('dragging');
    });
    node.addEventListener('dragend', () => {
      node.classList.remove('dragging');
      onEndDrag?.();
    });
  }

  function renderPreviewVisual(asset, { width = 180, height = 96, interactive = false } = {}) {
    const { looks } = getContext();
    if (asset.kind === 'texture') {
      const box = el('div', { class: 'asset-thumb texture-checker' });
      const img = el('img', {
        src: projectAssetUrl(asset.path),
        alt: '',
        loading: 'lazy',
        decoding: 'async',
        draggable: 'false',
      });
      img.addEventListener('load', () => {
        if (img.naturalWidth > 0 && img.naturalHeight > 0) {
          previews.recordTextureDimensions(asset, img.naturalWidth, img.naturalHeight);
          const pill = box.querySelector('.asset-meta-pill');
          if (pill) pill.textContent = `${img.naturalWidth}×${img.naturalHeight}`;
          else box.append(el('span', { class: 'asset-meta-pill' }, `${img.naturalWidth}×${img.naturalHeight}`));
        }
      }, { once: true });
      img.addEventListener('error', () => {
        img.replaceWith(kindGraphic('texture'));
      }, { once: true });
      box.append(img);
      const cached = previews.getMeta(asset);
      if (cached?.width) {
        box.append(el('span', { class: 'asset-meta-pill' }, `${cached.width}×${cached.height}`));
      }
      return box;
    }

    if (asset.kind === 'model') {
      const box = el('div', { class: 'asset-thumb kind-model' });
      const canvas = el('canvas', { class: 'asset-preview-canvas' });
      const paint = (info) => {
        drawGlbPreviewCanvas(canvas, info, {
          width,
          height,
          yaw: interactive ? ui.turntable.yaw : 0.68,
          pitch: interactive ? ui.turntable.pitch : 0.42,
        });
        let pill = box.querySelector('.asset-meta-pill');
        if (info?.triangles) {
          const text = `${info.triangles} tris`;
          if (pill) pill.textContent = text;
          else box.append(el('span', { class: 'asset-meta-pill' }, text));
        }
      };
      const cached = previews.getMeta(asset);
      if (cached) {
        box.append(canvas);
        paint(cached);
      } else {
        const placeholder = kindGraphic('model');
        box.append(placeholder);
        previews.loadEntry(asset).then((info) => {
          if (!info || !box.isConnected) return;
          placeholder.replaceWith(canvas);
          paint(info);
        });
      }
      if (interactive) {
        box.addEventListener('pointerdown', (ev) => {
          if (ev.button !== 0) return;
          const info = previews.getMeta(asset);
          if (!info) return;
          ev.preventDefault();
          box.setPointerCapture(ev.pointerId);
          let lx = ev.clientX, ly = ev.clientY;
          const onMove = (me) => {
            ui.turntable.yaw += (me.clientX - lx) * 0.016;
            ui.turntable.pitch = Math.max(-1.05, Math.min(1.25, ui.turntable.pitch + (me.clientY - ly) * 0.012));
            lx = me.clientX;
            ly = me.clientY;
            if (canvas.isConnected) paint(info);
          };
          const onUp = () => {
            box.removeEventListener('pointermove', onMove);
            box.removeEventListener('pointerup', onUp);
            box.removeEventListener('pointercancel', onUp);
          };
          box.addEventListener('pointermove', onMove);
          box.addEventListener('pointerup', onUp);
          box.addEventListener('pointercancel', onUp);
        });
      }
      return box;
    }

    if (asset.kind === 'look') {
      const box = el('div', { class: 'asset-thumb kind-look' });
      const canvas = el('canvas', { class: 'asset-preview-canvas' });
      box.append(canvas);
      const liveLook = looks?.get(asset.id)?.root;
      if (liveLook) {
        drawLookPreviewCanvas(canvas, liveLook, { width, height });
      } else {
        const cached = previews.getMeta(asset);
        drawLookPreviewCanvas(canvas, cached?.json || {}, { width, height });
        if (!cached) {
          previews.loadEntry(asset).then((entry) => {
            if (entry?.json && canvas.isConnected) {
              drawLookPreviewCanvas(canvas, entry.json, { width, height });
            }
          });
        }
      }
      return box;
    }

    if (asset.kind === 'material') {
      const box = el('div', { class: 'asset-thumb kind-material' });
      const canvas = el('canvas', { class: 'asset-preview-canvas' });
      box.append(canvas);
      const cached = previews.getMeta(asset);
      drawMaterialPreviewCanvas(canvas, cached?.json || {}, { width, height });
      if (!cached) {
        previews.loadEntry(asset).then((entry) => {
          if (entry?.json && canvas.isConnected) {
            drawMaterialPreviewCanvas(canvas, entry.json, { width, height });
          }
        });
      }
      return box;
    }

    if (asset.kind === 'sound') {
      const box = el('div', { class: 'asset-thumb kind-sound' });
      const canvas = el('canvas', { class: 'asset-preview-canvas' });
      const playing = ui.playingId === asset.id;
      const paint = (wave) => {
        drawWaveformCanvas(canvas, wave, { width, height, playing });
        if (wave?.durationSec >= 0) {
          const label = wave.durationSec >= 10
            ? `${Math.round(wave.durationSec)}s`
            : `${wave.durationSec.toFixed(2)}s`;
          const existing = box.querySelector('.asset-meta-pill');
          if (existing) existing.textContent = label;
          else box.append(el('span', { class: 'asset-meta-pill' }, label));
        }
      };
      const cached = previews.getMeta(asset);
      if (cached) {
        box.append(canvas);
        paint(cached);
      } else {
        const placeholder = kindGraphic('sound');
        box.append(placeholder);
        previews.loadEntry(asset).then((wave) => {
          if (!wave || !box.isConnected) return;
          placeholder.replaceWith(canvas);
          paint(wave);
        });
      }
      return box;
    }

    return el('div', { class: `asset-thumb kind-${asset.kind}` }, kindGraphic(asset.kind));
  }

  function renderAssetActions(asset, doc) {
    const actions = el('span', { class: 'asset-actions' });
    if (asset.kind === 'scene') {
      actions.append(el('button', {
        type: 'button',
        class: 'mini',
        onclick: (ev) => { ev.stopPropagation(); recordRecent(asset.id); onOpenDoc(asset.id); },
      }, t('open')));
    } else if (asset.kind === 'prefab') {
      actions.append(
        makePlaceControl(asset, doc),
        el('button', {
          type: 'button',
          class: 'mini',
          onclick: (ev) => { ev.stopPropagation(); recordRecent(asset.id); onOpenDoc(asset.id); },
        }, t('open')),
      );
    } else if (asset.kind === 'look') {
      actions.append(el('button', {
        type: 'button',
        class: 'mini',
        onclick: (ev) => { ev.stopPropagation(); recordRecent(asset.id); onInspectLook(asset.id); },
      }, t('editLook')));
    } else if (asset.kind === 'data') {
      actions.append(el('button', {
        type: 'button',
        class: 'mini',
        onclick: (ev) => { ev.stopPropagation(); recordRecent(asset.id); onInspectData?.(asset.id); },
      }, t('editData')));
    } else if (WORKBENCH_BY_KIND[asset.kind] && onOpenWorkbenchDoc) {
      const wb = WORKBENCH_BY_KIND[asset.kind];
      actions.append(el('button', {
        type: 'button',
        class: 'mini',
        onclick: (ev) => { ev.stopPropagation(); recordRecent(asset.id); onOpenWorkbenchDoc(wb, asset.id); },
      }, asset.kind === 'material' ? t('editMaterial') : t('open')));
    } else if (asset.kind === 'model') {
      actions.append(
        makePlaceControl(asset, doc),
        el('button', {
          type: 'button',
          class: 'mini',
          onclick: (ev) => { ev.stopPropagation(); recordRecent(asset.id); onImportModel(asset); },
        }, t('importAsScene')),
      );
    } else if (asset.kind === 'sound') {
      const playing = ui.playingId === asset.id;
      actions.append(el('button', {
        type: 'button',
        class: `mini${playing ? ' on' : ''}`,
        onclick: (ev) => { ev.stopPropagation(); toggleSound(asset); },
      }, playing ? t('stopSound') : t('previewSound')));
    }
    return actions;
  }

  // ---- Context Menu ---------------------------------------------------------------------------

  function closeContextMenu() {
    const menu = document.getElementById('asset-context-menu');
    if (menu) {
      menu.hidden = true;
      menu.replaceChildren();
    }
  }

  function openContextMenu(asset, clientX, clientY) {
    const menu = document.getElementById('asset-context-menu');
    if (!menu) return;
    selectAsset(asset.id);
    const { doc } = getContext();
    menu.replaceChildren();

    const item = (label, action, { shortcut = '', disabled = false } = {}) => {
      menu.append(el('button', {
        type: 'button',
        role: 'menuitem',
        disabled,
        onclick: () => {
          closeContextMenu();
          action();
        },
      },
        el('span', {}, label),
        shortcut ? el('span', { class: 'menu-shortcut' }, shortcut) : null,
      ));
    };

    if (asset.kind === 'scene' || asset.kind === 'prefab') {
      item(t('open'), () => { recordRecent(asset.id); onOpenDoc(asset.id); }, { shortcut: 'Enter' });
    } else if (asset.kind === 'look') {
      item(t('editLook'), () => { recordRecent(asset.id); onInspectLook(asset.id); }, { shortcut: 'Enter' });
    } else if (asset.kind === 'data') {
      item(t('editData'), () => { recordRecent(asset.id); onInspectData?.(asset.id); }, { shortcut: 'Enter' });
    } else if (WORKBENCH_BY_KIND[asset.kind] && onOpenWorkbenchDoc) {
      const wb = WORKBENCH_BY_KIND[asset.kind];
      item(t('open'), () => { recordRecent(asset.id); onOpenWorkbenchDoc(wb, asset.id); }, { shortcut: 'Enter' });
    } else if (asset.kind === 'sound') {
      item(ui.playingId === asset.id ? t('stopSound') : t('previewSound'), () => toggleSound(asset));
    }

    if (asset.kind === 'model' || asset.kind === 'prefab') {
      const check = canPlaceAsset(asset, doc);
      item('+ ' + t('placeInScene'), () => { recordRecent(asset.id); onPlaceAsset(asset); }, { disabled: !check.ok });
    }
    if (asset.kind === 'model') {
      item(t('importAsScene'), () => { recordRecent(asset.id); onImportModel(asset); });
      if (onOpenWorkbenchDoc) {
        item(t('openModelStudio'), () => { recordRecent(asset.id); onOpenWorkbenchDoc('model', asset.id); });
      }
    }

    menu.append(el('div', { class: 'menu-sep' }));
    const isFav = ui.favorites.has(asset.id);
    item(isFav ? '★ ' + t('unfavoriteAsset') : '☆ ' + t('favoriteAsset'), () => toggleFavorite(asset.id));
    item(t('renameAsset'), () => promptRenameAsset(asset), { shortcut: 'F2' });
    item(t('moveAsset'), () => promptMoveAsset(asset));

    menu.append(el('div', { class: 'menu-sep' }));
    item(t('copyId'), () => navigator.clipboard?.writeText(asset.id).catch(() => {}));
    item(t('copyPath'), () => navigator.clipboard?.writeText(asset.path).catch(() => {}));

    menu.hidden = false;
    const rect = menu.getBoundingClientRect();
    const left = Math.max(6, Math.min(window.innerWidth - rect.width - 6, clientX));
    const top = Math.max(6, Math.min(window.innerHeight - rect.height - 6, clientY));
    menu.style.left = `${left}px`;
    menu.style.top = `${top}px`;
  }

  // ---- Selection, Folders, Breadcrumbs & Asset Details Drawer ---------------------------------

  function selectAsset(id) {
    ui.selectedId = id;
    if (id) onRequestUsage?.(id);
    drawBar();
    drawAssetInspector();
    for (const item of document.querySelectorAll('#assets-list [data-id]')) {
      item.classList.toggle('asset-selected', item.dataset.id === id);
    }
  }

  function wireFolderDrop(row, folderPath) {
    if (!folderPath || !onMoveAsset) return;
    row.addEventListener('dragover', (ev) => {
      const types = ev.dataTransfer?.types;
      if (!types || ![...types].includes(ASSET_DRAG_MIME)) return;
      ev.preventDefault();
      ev.stopPropagation();
      if (ev.dataTransfer) ev.dataTransfer.dropEffect = 'move';
      row.classList.add('drop-target');
    });
    row.addEventListener('dragleave', (ev) => {
      if (!row.contains(ev.relatedTarget)) row.classList.remove('drop-target');
    });
    row.addEventListener('drop', (ev) => {
      row.classList.remove('drop-target');
      const types = ev.dataTransfer?.types;
      if (!types || ![...types].includes(ASSET_DRAG_MIME)) return;
      ev.preventDefault();
      ev.stopPropagation();
      const id = ev.dataTransfer.getData(ASSET_DRAG_MIME);
      const { assets, draggingAsset } = getContext();
      const asset = (id && assets.find((a) => a.id === id)) || draggingAsset;
      if (!asset) return;
      const target = `${folderPath}/${assetBasename(asset.path)}`;
      if (target !== asset.path) onMoveAsset(asset, target);
    });
  }

  function drawFoldersFromContext() {
    const { assets, drafts, project } = getContext();
    drawFolders(assets, drafts, project?.content || []);
  }

  function drawFolders(assets, drafts, contentRoots) {
    const nav = document.getElementById('assets-folders');
    if (!nav) return;
    const prevScroll = nav.scrollTop;
    nav.replaceChildren();

    const assetIds = new Set(assets.map((a) => a.id));
    const favCount = [...ui.favorites].filter((id) => assetIds.has(id)).length;
    const recentCount = ui.recent.filter((id) => assetIds.has(id)).length;
    const unusedCount = ui.usageReady
      ? assets.filter((a) => (ui.usageCounts[a.id] || 0) === 0).length
      : 0;

    const addCollection = (collection, icon, labelText, count, extraAttrs = {}) => {
      const active = ui.collection === collection && (collection !== 'all' || ui.folder === '');
      nav.append(el('div', {
        class: `folder-row${active ? ' active' : ''}`,
        'data-collection': collection,
        ...extraAttrs,
        onclick: () => {
          ui.collection = collection;
          if (collection === 'all') ui.folder = '';
          draw();
        },
      },
        el('span', { class: 'folder-twisty' }, icon),
        el('span', { class: 'folder-name' }, labelText),
        el('span', { class: 'folder-count' }, String(count)),
      ));
    };

    addCollection('all', '·', t('allFolders'), assets.length, { 'data-folder': '' });
    addCollection('favorites', '★', t('favorites'), favCount);
    addCollection('recent', '◷', t('recentAssets'), recentCount);
    addCollection('unused', '◌', t('unusedAssets'), unusedCount);

    if (drafts.length) {
      nav.append(el('div', { class: 'folder-section' }, t('missingDrafts')));
      for (const doc of drafts) {
        nav.append(el('div', {
          class: 'folder-row missing-draft',
          title: doc.path,
          onclick: () => (doc.kind === 'look' ? onInspectLook(doc.id) : doc.kind === 'data' ? onInspectData?.(doc.id) : onOpenDoc(doc.id)),
        },
          el('span', { class: 'folder-twisty' }, '!'),
          el('span', { class: 'folder-name' }, assetBasename(doc.path)),
        ));
      }
    }

    nav.append(el('div', { class: 'folder-divider' }, t('foldersHeading')));
    const tree = buildFolderTree(assets, contentRoots, ui.folder);
    const appendNode = (node, depth) => {
      const hasKids = node.children.length > 0;
      const collapsed = hasKids && ui.collapsedFolders.has(node.path);
      const active = ui.collection === 'all' && ui.folder === node.path;
      const row = el('div', {
        class: `folder-row${active ? ' active' : ''}`,
        style: `padding-left:${6 + depth * 12}px`,
        'data-folder': node.path,
        title: node.path,
        onclick: () => {
          ui.collection = 'all';
          ui.folder = node.path;
          draw();
        },
      },
        el('span', {
          class: 'folder-twisty',
          onclick: hasKids ? (ev) => {
            ev.stopPropagation();
            if (ui.collapsedFolders.has(node.path)) ui.collapsedFolders.delete(node.path);
            else ui.collapsedFolders.add(node.path);
            drawFolders(assets, drafts, contentRoots);
          } : null,
        }, hasKids ? (collapsed ? '▸' : '▾') : '·'),
        el('span', { class: 'folder-name' }, node.name),
        el('span', { class: 'folder-count' }, String(node.count)),
      );
      wireFolderDrop(row, node.path);
      nav.append(row);
      if (!collapsed) {
        for (const child of node.children) appendNode(child, depth + 1);
      }
    };
    for (const root of tree) appendNode(root, 0);
    nav.scrollTop = prevScroll;
  }

  function drawKinds(assets) {
    const bar = document.getElementById('assets-kinds');
    if (!bar) return;
    bar.replaceChildren();
    const counts = kindCounts(assets, {
      folder: ui.folder,
      collection: ui.collection,
      query: ui.query,
      favorites: ui.favorites,
      recent: ui.recent,
      usageCounts: ui.usageCounts,
    });
    const makeChip = (kind, labelText, count) => {
      const on = ui.kind === kind;
      return el('button', {
        type: 'button',
        class: `kind-pill kind-${kind}${on ? ' on' : ''}`,
        'data-kind-filter': kind,
        'aria-pressed': String(on),
        onclick: () => {
          ui.kind = on && kind !== 'all' ? 'all' : kind;
          draw();
        },
      }, labelText, el('span', { class: 'pill-count' }, String(count)));
    };
    bar.append(makeChip('all', t('allKinds'), counts.all));
    for (const kind of KIND_ORDER) {
      if (counts[kind] > 0 || ui.kind === kind) {
        bar.append(makeChip(kind, t('kind_' + kind), counts[kind] || 0));
      }
    }
  }

  function drawBar() {
    const bar = document.getElementById('assets-bar');
    if (!bar) return;
    bar.replaceChildren();
    const { assets, doc } = getContext();

    const crumbs = el('div', { class: 'assets-crumbs' });
    crumbs.append(el('button', {
      type: 'button',
      class: `crumb${ui.collection === 'all' && ui.folder === '' ? ' current' : ''}`,
      onclick: () => { ui.collection = 'all'; ui.folder = ''; draw(); },
    }, t('allFolders')));
    if (ui.collection !== 'all') {
      const collLabel = ui.collection === 'favorites'
        ? '★ ' + t('favorites')
        : ui.collection === 'recent'
          ? '◷ ' + t('recentAssets')
          : '◌ ' + t('unusedAssets');
      crumbs.append(
        el('span', { class: 'crumb-sep' }, '▸'),
        el('span', { class: 'crumb current' }, collLabel),
      );
    } else if (ui.folder) {
      const parts = ui.folder.split('/');
      let acc = '';
      for (const part of parts) {
        acc = acc ? `${acc}/${part}` : part;
        const target = acc;
        crumbs.append(
          el('span', { class: 'crumb-sep' }, '▸'),
          el('button', {
            type: 'button',
            class: `crumb${target === ui.folder ? ' current' : ''}`,
            onclick: () => { ui.collection = 'all'; ui.folder = target; draw(); },
          }, part),
        );
      }
    }
    if (ui.query || ui.kind !== 'all' || ui.collection !== 'all') {
      crumbs.append(el('button', {
        type: 'button',
        class: 'mini clear-filter',
        onclick: () => {
          ui.collection = 'all';
          ui.query = '';
          ui.kind = 'all';
          const search = document.getElementById('assets-search');
          if (search) search.value = '';
          draw();
        },
      }, t('clearFilters')));
    }
    bar.append(crumbs);

    const selected = ui.selectedId && assets.find((a) => a.id === ui.selectedId);
    if (selected) {
      const detail = el('div', { class: 'asset-detail-strip' });
      detail.append(el('span', { class: 'asset-detail-path', title: selected.path }, selected.path));
      const copied = ui.copiedId === selected.id;
      detail.append(el('button', {
        type: 'button',
        class: 'mini id-pill',
        title: `${t('copyId')}: ${selected.id}`,
        onclick: () => {
          navigator.clipboard?.writeText(selected.id).catch(() => {});
          ui.copiedId = selected.id;
          clearTimeout(ui.copiedTimer);
          ui.copiedTimer = setTimeout(() => { ui.copiedId = null; drawBar(); }, 1200);
          drawBar();
        },
      }, copied ? t('copiedId') : `id ${selected.id.slice(0, 8)}…`));
      const placeCheck = canPlaceAsset(selected, doc);
      if (selected.kind === 'model' || selected.kind === 'prefab') {
        detail.append(el('button', {
          type: 'button',
          id: 'place-selected-asset',
          class: 'mini',
          disabled: !placeCheck.ok,
          title: placeCheck.reason ? t(placeCheck.reason) : t('placeInSceneTip'),
          onclick: () => { recordRecent(selected.id); onPlaceAsset(selected); },
        }, '+ ' + t('placeInScene')));
      }
      bar.append(detail);
    }
  }

  function drawAssetInspector() {
    const body = document.getElementById('assets-body');
    const pane = document.getElementById('assets-inspector');
    const toggleBtn = document.getElementById('toggle-asset-details');
    if (toggleBtn) {
      toggleBtn.classList.toggle('on', ui.detailsOpen);
      toggleBtn.setAttribute('aria-pressed', String(ui.detailsOpen));
    }
    if (body) body.classList.toggle('with-inspector', ui.detailsOpen);
    if (!pane || !ui.detailsOpen) return;

    pane.replaceChildren();
    const { assets, doc } = getContext();
    const selected = ui.selectedId && assets.find((a) => a.id === ui.selectedId);
    if (!selected) {
      pane.append(el('p', { class: 'hint' }, t('selectAssetHint')));
      return;
    }

    const preview = renderPreviewVisual(selected, { width: 235, height: 108, interactive: selected.kind === 'model' });
    preview.classList.add('asset-inspector-preview');
    pane.append(preview);

    const fav = ui.favorites.has(selected.id);
    pane.append(el('div', { class: 'asset-inspector-title' },
      el('strong', { title: assetBasename(selected.path) }, assetStem(selected.path)),
      el('button', {
        type: 'button',
        id: 'asset-detail-fav',
        class: `mini${fav ? ' on' : ''}`,
        title: fav ? t('unfavoriteAsset') : t('favoriteAsset'),
        onclick: () => toggleFavorite(selected.id),
      }, fav ? '★' : '☆'),
    ));
    pane.append(el('div', { class: 'asset-inspector-path' }, selected.path));

    const stats = el('div', { class: 'asset-inspector-stats' });
    stats.append(el('span', { class: `kind-tag kind-${selected.kind}` }, t('kind_' + selected.kind)));
    if (selected.bytes > 0) {
      stats.append(el('span', { class: 'stat-chip' }, formatBytes(selected.bytes)));
    }
    const meta = previews.getMeta(selected);
    if (selected.kind === 'model' && meta?.triangles !== undefined) {
      stats.append(el('span', { class: 'stat-chip' },
        t('modelStats').replace('{v}', String(meta.vertices)).replace('{t}', String(meta.triangles))));
      if (meta.size?.some((s) => s > 0)) {
        const dims = meta.size.map((v) => M.tidy(v, 2)).join('×');
        stats.append(el('span', { class: 'stat-chip' }, t('boundsMeters', dims)));
      }
    } else if (selected.kind === 'texture' && meta?.width) {
      stats.append(el('span', { class: 'stat-chip' }, `${meta.width}×${meta.height} px`));
    } else if (selected.kind === 'sound' && meta?.durationSec !== undefined) {
      stats.append(el('span', { class: 'stat-chip' }, `${meta.durationSec.toFixed(2)}s · ${meta.sampleRate} Hz`));
    }
    pane.append(stats);

    const actions = el('div', { class: 'asset-inspector-actions' });
    if (selected.kind === 'scene' || selected.kind === 'prefab') {
      actions.append(el('button', {
        type: 'button',
        class: 'mini',
        onclick: () => { recordRecent(selected.id); onOpenDoc(selected.id); },
      }, t('open')));
    } else if (selected.kind === 'look') {
      actions.append(el('button', {
        type: 'button',
        class: 'mini',
        onclick: () => { recordRecent(selected.id); onInspectLook(selected.id); },
      }, t('editLook')));
    } else if (selected.kind === 'data') {
      actions.append(el('button', {
        type: 'button',
        class: 'mini',
        onclick: () => { recordRecent(selected.id); onInspectData?.(selected.id); },
      }, t('editData')));
    } else if (WORKBENCH_BY_KIND[selected.kind] && onOpenWorkbenchDoc) {
      const wb = WORKBENCH_BY_KIND[selected.kind];
      actions.append(el('button', {
        type: 'button',
        class: 'mini',
        onclick: () => { recordRecent(selected.id); onOpenWorkbenchDoc(wb, selected.id); },
      }, selected.kind === 'material' ? t('editMaterial') : t('open')));
    }
    if (selected.kind === 'model' && onOpenWorkbenchDoc) {
      actions.append(el('button', {
        type: 'button',
        class: 'mini',
        onclick: () => { recordRecent(selected.id); onOpenWorkbenchDoc('model', selected.id); },
      }, t('openModelStudio')));
    }
    if (selected.kind === 'model' || selected.kind === 'prefab') {
      const check = canPlaceAsset(selected, doc);
      actions.append(el('button', {
        type: 'button',
        class: 'mini',
        disabled: !check.ok,
        title: check.reason ? t(check.reason) : t('placeInSceneTip'),
        onclick: () => { recordRecent(selected.id); onPlaceAsset(selected); },
      }, '+ ' + t('placeInScene')));
    }
    actions.append(
      el('button', {
        type: 'button',
        id: 'asset-detail-rename',
        class: 'mini',
        onclick: () => promptRenameAsset(selected),
      }, t('renameAsset')),
      el('button', {
        type: 'button',
        id: 'asset-detail-move',
        class: 'mini',
        onclick: () => promptMoveAsset(selected),
      }, t('moveAsset')),
    );
    pane.append(actions);

    // Where Used Explorer
    const uses = ui.usesByAsset.get(selected.id);
    const countLabel = uses
      ? (uses.length ? t('usedInCount', uses.length) : t('unusedBadge'))
      : '';
    pane.append(el('div', { class: 'where-used-header' },
      el('span', {}, t('whereUsed')),
      el('span', {}, countLabel),
    ));

    const refBox = el('div', { id: 'asset-where-used', class: 'where-used-list' });
    if (!uses) {
      refBox.append(el('span', { class: 'hint' }, t('loadingReferences')));
    } else if (!uses.length) {
      refBox.append(el('span', { class: 'hint' }, t('noReferences')));
    } else {
      for (const ref of uses) {
        const docBase = assetBasename(ref.docPath || '');
        const labelText = ref.docKind === 'project'
          ? `${t('startupSceneRef')} (${docBase})`
          : ref.entityName
            ? `${docBase} · ${ref.entityName}`
            : docBase;
        const canJump = !!ref.doc && (ref.docKind === 'scene' || ref.docKind === 'prefab' || ref.docKind === 'data');
        refBox.append(el('button', {
          type: 'button',
          class: 'where-used-item',
          disabled: !canJump,
          'data-ref-doc': ref.doc || '',
          'data-ref-entity': ref.entity || '',
          title: `${ref.docPath} (${ref.field})`,
          onclick: () => canJump && onJumpToReference?.(ref),
        },
          el('span', { class: 'ref-main' }, labelText),
          el('span', { class: 'ref-field' }, ref.field),
        ));
      }
    }
    pane.append(refBox);
  }

  function draw() {
    const { assets, drafts, project, doc, inspectedLook, looks, inspectedData, datas, openDocs } = getContext();
    const list = document.getElementById('assets-list');
    if (!list) return;

    syncProjectPrefs(project);
    applyScaleCss();

    // If a rescanned project dropped the active folder, fall back cleanly to All Assets.
    if (ui.folder && !assets.some((a) => inFolder(assetFolder(a.path), ui.folder)) &&
        !(project?.content || []).some((c) => inFolder(c, ui.folder))) {
      ui.folder = '';
    }
    if (ui.selectedId && !assets.some((a) => a.id === ui.selectedId)) {
      ui.selectedId = null;
    }

    const viewGridBtn = document.getElementById('assets-view-grid');
    const viewListBtn = document.getElementById('assets-view-list');
    if (viewGridBtn) viewGridBtn.classList.toggle('on', ui.view === 'grid');
    if (viewListBtn) viewListBtn.classList.toggle('on', ui.view === 'list');
    const sortSel = document.getElementById('assets-sort');
    if (sortSel && sortSel.value !== ui.sort) sortSel.value = ui.sort;
    const scaleInput = document.getElementById('assets-scale');
    if (scaleInput && Number(scaleInput.value) !== ui.scale) scaleInput.value = String(ui.scale);

    drawFolders(assets, drafts, project?.content || []);
    drawKinds(assets);
    drawBar();
    drawAssetInspector();

    const filtered = filterAssets(assets, {
      folder: ui.folder,
      collection: ui.collection,
      kind: ui.kind,
      query: ui.query,
      sort: ui.sort,
      favorites: ui.favorites,
      recent: ui.recent,
      usageCounts: ui.usageCounts,
    });
    const countBadge = document.getElementById('assets-count');
    if (countBadge) {
      countBadge.textContent = filtered.length === assets.length
        ? String(assets.length)
        : `${filtered.length} / ${assets.length}`;
    }

    const requested = new URLSearchParams(location.search).get('asset');
    const prevScroll = list.scrollTop;
    list.className = ui.view === 'grid' ? 'view-grid' : 'view-list';
    list.replaceChildren();

    if (drafts.length && ui.collection === 'all' && !ui.folder && ui.kind === 'all' && !ui.query) {
      for (const draft of drafts) {
        const card = el('div', {
          class: 'asset missing-draft',
          title: draft.path,
          'data-kind': draft.kind,
        },
          ui.view === 'grid' ? renderPreviewVisual(draft, { width: ui.scale, height: Math.round(ui.scale * 0.48) }) : null,
          el('div', { class: 'asset-info' },
            el('span', { class: 'asset-name' }, assetBasename(draft.path)),
            el('span', { class: 'asset-sub' }, t('missingDrafts')),
          ),
          el('span', { class: 'asset-actions' },
            el('button', {
              type: 'button',
              class: 'mini',
              onclick: () => (draft.kind === 'look' ? onInspectLook(draft.id) : draft.kind === 'data' ? onInspectData?.(draft.id) : onOpenDoc(draft.id)),
            }, t('open')),
          ),
        );
        list.append(card);
      }
    }

    if (!filtered.length && (!drafts.length || ui.collection !== 'all' || ui.folder || ui.kind !== 'all' || ui.query)) {
      const hasFilter = ui.collection !== 'all' || ui.folder || ui.kind !== 'all' || ui.query;
      const empty = el('div', { class: 'assets-empty' },
        el('p', { class: 'hint' }, assets.length ? t('noMatchingAssets') : t('noProjectAssets')),
        assets.length && hasFilter
          ? el('button', {
              type: 'button',
              class: 'mini',
              onclick: () => {
                ui.collection = 'all';
                ui.folder = '';
                ui.kind = 'all';
                ui.query = '';
                const search = document.getElementById('assets-search');
                if (search) search.value = '';
                draw();
              },
            }, t('clearFilters'))
          : null,
      );
      list.append(empty);
      return;
    }

    const thumbHeight = Math.round(ui.scale * 0.48);
    for (const a of filtered) {
      const name = assetBasename(a.path);
      const folder = assetFolder(a.path);
      const sizeText = a.bytes > 0 ? formatBytes(a.bytes) : '';
      const subText = folder && sizeText ? `${folder} · ${sizeText}` : (folder || sizeText || '·');
      const isRequested = a.id === inspectedLook || a.id === inspectedData || a.path === requested;
      const isOpenDoc = doc?.id === a.id;
      const isDirty = (isOpenDoc && doc?.dirty)
        || (a.kind === 'look' && looks.get(a.id)?.dirty)
        || (a.kind === 'data' && datas?.get(a.id)?.dirty)
        || Boolean(openDocs?.get(a.id)?.dirty);
      const classes = [
        'asset',
        `kind-${a.kind}`,
        isRequested ? 'requested-asset' : '',
        isOpenDoc ? 'open-asset' : '',
        ui.selectedId === a.id ? 'asset-selected' : '',
      ].filter(Boolean).join(' ');

      const runPrimary = primaryAction(a, doc);
      const item = el('div', {
        class: classes,
        title: a.path,
        'data-id': a.id,
        'data-kind': a.kind,
        onclick: () => selectAsset(a.id),
        ondblclick: () => runPrimary?.(),
        oncontextmenu: (ev) => {
          ev.preventDefault();
          openContextMenu(a, ev.clientX, ev.clientY);
        },
      });
      wireDrag(item, a);

      if (ui.view === 'grid') {
        const preview = renderPreviewVisual(a, { width: ui.scale, height: thumbHeight });
        preview.append(
          el('span', { class: `kind-tag kind-${a.kind}` }, t('kind_' + a.kind)),
          makeFavoriteChip(a),
        );
        if (isDirty) preview.append(el('span', { class: 'badge dirty-dot' }, '•'));
        item.append(
          preview,
          el('div', { class: 'asset-info' },
            el('div', { class: 'asset-name' }, name),
            el('div', { class: 'asset-sub' }, subText),
          ),
          renderAssetActions(a, doc),
        );
      } else {
        item.append(
          makeFavoriteChip(a),
          el('span', { class: `kind-tag kind-${a.kind}` }, t('kind_' + a.kind)),
          el('span', { class: 'asset-name' }, name),
          isDirty ? el('span', { class: 'badge' }, '•') : null,
          el('span', { class: 'asset-sub' }, subText),
          renderAssetActions(a, doc),
        );
      }
      list.append(item);
    }
    list.scrollTop = prevScroll;
  }

  function isExternalFileDrag(ev) {
    const types = ev.dataTransfer?.types;
    if (!types) return false;
    const arr = [...types];
    return arr.includes('Files') && !arr.includes(ASSET_DRAG_MIME);
  }

  function setupEvents() {
    applyScaleCss();
    const search = document.getElementById('assets-search');
    if (search) {
      search.addEventListener('input', () => {
        ui.query = search.value;
        draw();
      });
      search.addEventListener('keydown', (ev) => {
        if (ev.key === 'Escape' && search.value) {
          ev.stopPropagation();
          search.value = '';
          ui.query = '';
          draw();
        }
      });
    }

    const sortSel = document.getElementById('assets-sort');
    if (sortSel) {
      sortSel.value = ui.sort;
      sortSel.addEventListener('change', () => {
        ui.sort = sortSel.value;
        writePref('ganbate.studio.assetSort', ui.sort);
        draw();
      });
    }

    const scaleInput = document.getElementById('assets-scale');
    if (scaleInput) {
      scaleInput.value = String(ui.scale);
      scaleInput.addEventListener('input', () => {
        ui.scale = Math.max(112, Math.min(220, Number(scaleInput.value) || 152));
        writePref('ganbate.studio.assetScale', String(ui.scale));
        applyScaleCss();
        draw();
      });
    }

    const detailsBtn = document.getElementById('toggle-asset-details');
    if (detailsBtn) {
      detailsBtn.onclick = () => {
        ui.detailsOpen = !ui.detailsOpen;
        writePref('ganbate.studio.assetDetails', ui.detailsOpen ? '1' : '0');
        drawAssetInspector();
      };
    }

    const importBtn = document.getElementById('import-assets');
    const importInput = document.getElementById('import-assets-input');
    if (importBtn && importInput) {
      importBtn.onclick = () => importInput.click();
      importInput.addEventListener('change', () => {
        const files = [...(importInput.files || [])];
        importInput.value = '';
        if (files.length) onImportFiles?.(files, ui.folder);
      });
    }

    const assetsPanel = document.getElementById('assets');
    const dropOverlay = document.getElementById('asset-drop-overlay');
    const clearFileDrop = () => {
      assetsPanel?.classList.remove('file-drop-active');
      if (dropOverlay) dropOverlay.hidden = true;
    };
    if (assetsPanel) {
      assetsPanel.addEventListener('dragover', (ev) => {
        if (!isExternalFileDrag(ev)) return;
        ev.preventDefault();
        if (ev.dataTransfer) ev.dataTransfer.dropEffect = 'copy';
        assetsPanel.classList.add('file-drop-active');
        if (dropOverlay) {
          const { project } = getContext();
          const targetDir = ui.folder || project?.content?.[0] || 'content';
          dropOverlay.textContent = t('dropFilesToImport', targetDir);
          dropOverlay.hidden = false;
        }
      });
      assetsPanel.addEventListener('dragleave', (ev) => {
        if (!assetsPanel.contains(ev.relatedTarget)) clearFileDrop();
      });
      assetsPanel.addEventListener('drop', (ev) => {
        if (!isExternalFileDrag(ev)) return;
        ev.preventDefault();
        clearFileDrop();
        const files = [...(ev.dataTransfer?.files || [])];
        if (files.length) onImportFiles?.(files, ui.folder);
      });
    }

    const gridBtn = document.getElementById('assets-view-grid');
    const listBtn = document.getElementById('assets-view-list');
    if (gridBtn) {
      gridBtn.onclick = () => {
        ui.view = 'grid';
        writePref('ganbate.studio.assetView', 'grid');
        draw();
      };
    }
    if (listBtn) {
      listBtn.onclick = () => {
        ui.view = 'list';
        writePref('ganbate.studio.assetView', 'list');
        draw();
      };
    }

    const list = document.getElementById('assets-list');
    if (list) {
      list.addEventListener('scroll', closeContextMenu, { passive: true });
      list.addEventListener('keydown', (ev) => {
        const items = [...list.querySelectorAll('[data-id]')];
        if (!items.length) return;
        const idx = items.findIndex((n) => n.dataset.id === ui.selectedId);
        if (ev.key === 'ArrowRight' || ev.key === 'ArrowDown') {
          ev.preventDefault();
          const next = items[Math.min(items.length - 1, idx + 1)] || items[0];
          selectAsset(next.dataset.id);
          next.scrollIntoView({ block: 'nearest' });
        } else if (ev.key === 'ArrowLeft' || ev.key === 'ArrowUp') {
          ev.preventDefault();
          const prev = items[Math.max(0, (idx < 0 ? 1 : idx) - 1)] || items[0];
          selectAsset(prev.dataset.id);
          prev.scrollIntoView({ block: 'nearest' });
        } else if (ev.key === 'Enter' && idx >= 0) {
          ev.preventDefault();
          const { assets, doc } = getContext();
          const asset = assets.find((a) => a.id === ui.selectedId);
          if (asset) primaryAction(asset, doc)?.();
        } else if (ev.key === 'F2' && idx >= 0) {
          ev.preventDefault();
          const { assets } = getContext();
          const asset = assets.find((a) => a.id === ui.selectedId);
          if (asset) promptRenameAsset(asset);
        }
      });
    }

    window.addEventListener('pointerdown', (ev) => {
      const menu = document.getElementById('asset-context-menu');
      if (menu && !menu.hidden && !menu.contains(ev.target)) closeContextMenu();
    });
    window.addEventListener('keydown', (ev) => {
      if (ev.key === 'Escape') closeContextMenu();
    });
  }

  function revealAsset(id) {
    if (!id) return;
    ui.selectedId = id;
    ui.collection = 'all';
    ui.folder = '';
    ui.kind = 'all';
    ui.query = '';
    const search = document.getElementById('assets-search');
    if (search) search.value = '';
    onRequestUsage?.(id);
    draw();
    document.querySelector(`#assets-list [data-id="${CSS.escape(id)}"]`)?.scrollIntoView({ block: 'nearest' });
  }

  return {
    draw,
    setupEvents,
    revealAsset,
    recordRecent,
    setUsageCounts,
    setAssetUses,
    notifyAssetMoved,
    stopAudio,
    state: ui,
  };
}
