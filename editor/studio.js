// ABE Studio's interface. It talks to the studio host (`abe-studio`) over the edit protocol and
// draws the scene with the engine's web runtime, the same renderer a web game ships. The host
// owns the files and the undo history; this page shows a document, turns gestures into ops and
// applies the ops the host confirms.
import { Host, applyOps } from './protocol.js';
import * as M from './math.js';
import { t, label, lang, setLang, translate } from './i18n.js';

const $ = (sel) => document.querySelector(sel);
function el(tag, attrs = {}, ...children) {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k.startsWith('on')) e.addEventListener(k.slice(2), v);
    else if (k === 'class') e.className = v;
    else if (v !== undefined && v !== null && v !== false) e.setAttribute(k, v === true ? '' : v);
  }
  for (const c of children.flat()) if (c !== null && c !== undefined) e.append(c);
  return e;
}

const state = {
  host: null,
  project: null,
  kinds: {}, // component kind -> schema definition
  assets: [],
  doc: null, // {id, rev, path, kind, root, dirty, undo, redo, issues}
  selection: null,
  ids: [],
  mode: 'move',
  view: { target: [0, 1, 0], yaw: 0.7, pitch: 0.35, distance: 22, fov: 50 },
  time: 17.5,
  weather: 'clear',
  engine: null,
  canvas: null,
  cooked: null, // the last `cooked` message
  records: new Map(), // entity id -> record index
  current: [], // record index -> world matrix now
  inst: [], // world instance -> matrix now
  lamps: [], // world lamp -> position now
  cookTimer: 0,
  cooking: false,
  drag: null,
  hover: null,
  fps: 0,
  loadGen: 0, // counts viewport loads: a load that a newer one overtook drops its engine
  wanted: null, // the document last asked for, opened again after a reconnect
  inspectorStale: false, // the inspector skipped a redraw while a field was being typed into
};
window.__studio = state;
// For tests and plugins that need the viewport's geometry.
window.__studioView = {
  gizmo: () => gizmo(),
  boundsOf: (id) => boundsOf(id),
  project: (p) => project(p),
  select: (id) => select(id),
};

// ---- Console ---------------------------------------------------------------------------------

function log(level, text) {
  const line = el('div', { class: `log ${['info', 'warn', 'error'][level] || 'info'}` },
    `${new Date().toLocaleTimeString()}  ${text}`);
  const out = $('#console-lines');
  out.append(line);
  while (out.childElementCount > 400) out.firstChild.remove();
  out.scrollTop = out.scrollHeight;
}

// ---- Documents -------------------------------------------------------------------------------

const entities = () => state.doc?.root.entities || {};
const entity = (id) => entities()[id];

function sortKey(id) {
  const e = entity(id) || {};
  return [e.order === undefined ? 1 : 0, e.order || '', e.name || '', id];
}
function compareKeys(a, b) {
  for (let i = 0; i < a.length; i++) {
    if (a[i] < b[i]) return -1;
    if (a[i] > b[i]) return 1;
  }
  return 0;
}
function children(parent) {
  return Object.keys(entities())
    .filter((id) => (entities()[id].parent ?? null) === parent)
    .sort((a, b) => compareKeys(sortKey(a), sortKey(b)));
}
function subtree(id) {
  const out = [id];
  for (let i = 0; i < out.length; i++) out.push(...children(out[i]));
  return out;
}

/** `n` fresh entity ids from the host, or null without a connection. Callers that wait at the
 * same time share one request. */
async function takeIds(n) {
  while (state.ids.length < n) {
    if (!state.host.open) return null;
    if (!state.idsAsked) {
      state.idsAsked = new Promise((resolve) => {
        state.onIds = () => {
          state.idsAsked = null;
          state.onIds = null;
          resolve();
        };
      });
      state.host.send({ type: 'ids', count: Math.max(64, n) });
    }
    await state.idsAsked;
  }
  return state.ids.splice(0, n);
}

/** Queues a request about the open document behind the ones before it (see `Host.edit`):
 * `make(doc)` builds it when its turn comes. It is skipped once that document is closed. */
function queued(make, answers) {
  const id = state.doc?.id;
  if (!id) return Promise.resolve(null);
  return state.host.edit(() => (state.doc?.id === id && !state.doc.closed ? make(state.doc) : null), answers);
}

/** Sends one edit to the open document; resolves with the host's answer. */
function edit(label, ops, merge) {
  return queued((doc) => ({ type: 'edit', doc: doc.id, rev: doc.rev, label, merge, ops }))
    .then((answer) => {
      if (answer?.type === 'error') {
        log(2, `${label}: ${answer.message}`);
        // A field may still show the refused value: show the document's.
        drawInspector();
      }
      return answer;
    });
}

/** Undoes or redoes (`type`) in the open document after the edits made before it. The host
 * answers only when there is something to undo or redo: nothing is sent when its last word was
 * that there is not, and the queue moves on when another client's change leaves nothing. */
function undoRedo(type) {
  return queued((doc) => (doc[type] != null ? { type, doc: doc.id } : null),
    (m) => m.type === 'ops' && m.doc === state.doc?.id && m[type] === null);
}

/** Saves the open document after the edits made before it, the field being typed into too.
 * When another tool changed the file since it was opened, asks before overwriting it. */
async function save(force = false) {
  commitTyping();
  const answer = await queued((doc) => ({ type: 'save', doc: doc.id, ...(force ? { force: true } : {}) }),
    (m) => m.type === 'saved' && m.doc === state.doc?.id);
  if (answer?.type === 'error' && answer.code === 'changed-on-disk' && !force
      && window.confirm(t('overwriteChanged'))) {
    return save(true);
  }
  return answer;
}

/** Commits the field being typed into (it commits when it loses focus) and keeps the focus there. */
function commitTyping() {
  const a = document.activeElement;
  if (!a || !['INPUT', 'SELECT', 'TEXTAREA'].includes(a.tagName)) return;
  a.blur();
  a.focus({ preventScroll: true });
}

function setField(id, kind, field, value, merge) {
  return edit(`${t('vSet')} ${field}`, [{ set: ['entities', id, 'components', kind, field], value }], merge);
}

// ---- Messages from the host -------------------------------------------------------------------

function onMessage(msg) {
  switch (msg.type) {
    case 'welcome': {
      state.project = msg.project;
      state.kinds = {};
      for (const file of msg.schemas) Object.assign(state.kinds, file.components);
      state.assets = msg.assets;
      $('#project-name').textContent = msg.project.name || 'Project';
      drawAssets();
      // After a reconnect, the document that was open (or being opened) opens again.
      if (state.wanted) state.host.send({ type: 'open', doc: state.wanted });
      else if (msg.project.startup) openDoc(msg.project.startup);
      break;
    }
    case 'assets':
      state.assets = msg.assets;
      drawAssets();
      break;
    case 'doc':
      state.doc = { id: msg.doc, rev: msg.rev, path: msg.path, kind: msg.kind, root: msg.root, dirty: msg.dirty, undo: msg.undo, redo: msg.redo, issues: msg.issues };
      if (state.selection && !entity(state.selection)) state.selection = null;
      for (const i of msg.issues) log(1, `${i.path}: ${i.message}`);
      drawAll();
      requestCook(0);
      break;
    case 'ops':
      if (!state.doc || msg.doc !== state.doc.id) break;
      applyOps(state.doc.root, msg.ops);
      Object.assign(state.doc, { rev: msg.rev, dirty: msg.dirty, undo: msg.undo, redo: msg.redo });
      if (state.selection && !entity(state.selection)) state.selection = null;
      followOps(msg.ops);
      drawAll();
      break;
    case 'saved':
      // A save of an older revision leaves later edits unsaved.
      if (state.doc && msg.doc === state.doc.id && msg.rev === state.doc.rev) {
        state.doc.dirty = false;
        log(0, `saved ${msg.path}`);
        drawToolbar();
      }
      break;
    case 'ids':
      state.ids.push(...msg.ids);
      state.onIds?.();
      break;
    case 'created':
      log(0, `created ${msg.path}`);
      openDoc(msg.doc);
      break;
    case 'cooked':
      if (msg.doc !== state.doc?.id) break; // a cook of a document closed since
      state.cooking = false;
      for (const w of msg.warnings) log(1, w);
      loadWorld(msg).catch((e) => log(2, `viewport: ${e.message}`));
      break;
    case 'select':
      break;
    case 'error':
      log(2, msg.message);
      break;
    default:
      break;
  }
}

function openDoc(id) {
  if (state.doc && state.doc.id !== id) {
    state.doc.closed = true; // what is still queued for it is dropped (see `queued`)
    state.host.send({ type: 'close', doc: state.doc.id });
  }
  state.wanted = id;
  state.selection = null;
  state.host.send({ type: 'open', doc: id });
}

/** Moves what the viewport shows after confirmed ops: transforms move live, anything else
 * re-cooks the scene. */
function followOps(ops) {
  let structural = false;
  const moved = new Set();
  for (const op of ops) {
    const p = op.set || op.remove;
    if (p[0] !== 'entities') continue;
    if (p.length >= 4 && p[2] === 'components' && p[3] === 'transform') moved.add(p[1]);
    else if (p.length === 3 && (p[2] === 'name' || p[2] === 'order')) continue;
    else if (p.length >= 4 && p[2] === 'components' && p[3] !== 'mesh' && p[3] !== 'light') continue;
    else structural = true;
  }
  if (structural || !state.cooked) {
    requestCook(150);
    return;
  }
  for (const id of moved) {
    if (!state.records.has(id) || state.drag?.id === id) continue;
    placeEntity(id, worldFromDoc(id));
  }
}

// ---- Viewport --------------------------------------------------------------------------------

function requestCook(delay) {
  clearTimeout(state.cookTimer);
  state.cookTimer = setTimeout(() => {
    if (!state.doc) return;
    state.cooking = true;
    state.host.send({ type: 'cook', doc: state.doc.id });
  }, delay);
}

function lookAsset() {
  for (const e of Object.values(entities())) {
    const env = e.components?.environment;
    if (env) return env;
  }
  return null;
}

async function loadWorld(cooked) {
  // Cooks can arrive while an earlier one is still loading: only the newest load shows its
  // world, and every engine a newer load replaces or overtakes is destroyed.
  const gen = ++state.loadGen;
  const env = lookAsset();
  let look = {};
  if (env?.look) {
    const a = state.assets.find((x) => x.id === env.look);
    if (a) {
      const r = await fetch(`/project/${a.path.split('/').map(encodeURIComponent).join('/')}`);
      if (!r.ok) throw new Error(`${a.path}: HTTP ${r.status}`);
      look = await r.json();
    }
  }
  if (gen !== state.loadGen) return;
  if (env?.time !== undefined) state.time = env.time;
  if (env?.weather) state.weather = env.weather;
  const name = cooked.file.split('/').pop();
  const old = state.canvas;
  const canvas = el('canvas', { id: 'viewport-canvas' });
  const host = $('#viewport');
  if (state.engine) {
    try { state.engine.destroy(); } catch { /* the old engine is going anyway */ }
  }
  state.engine = null;
  if (old) old.replaceWith(canvas);
  else host.prepend(canvas);
  state.canvas = canvas;
  const { createEngine } = await import('/engine/abe.js');
  const api = new URLSearchParams(location.search).get('api') || 'auto';
  const engine = await createEngine({
    canvas,
    api,
    wasm: '/engine/abe_web.wasm',
    world: `/cooked/${name}`,
    look,
    onLog: (level, text) => log(level, `engine: ${text}`),
  });
  if (gen !== state.loadGen) {
    engine.destroy();
    return;
  }
  engine.fitCanvas(Math.min(window.devicePixelRatio || 1, 2));
  engine.setWeather(state.weather);
  state.engine = engine;
  state.cooked = cooked;
  state.records = new Map(cooked.entities.map((r, i) => [r.id, i]));
  state.current = cooked.entities.map((r) => r.transform.slice());
  state.inst = [];
  for (let i = 0; i < engine.worldInstances(); i++) state.inst.push(engine.worldInstance(i).matrix);
  state.lamps = (cooked.lamps || []).map((p) => p.slice());
  // The document may have moved on while the cook ran: bring transforms up to date.
  for (const id of Object.keys(entities())) {
    if (state.records.has(id)) placeEntity(id, worldFromDoc(id));
  }
  log(0, `viewport: ${cooked.entities.length} entities, ${state.inst.length} instances (${engine.api})`);
  if (!state.framed) {
    state.framed = true;
    frameAll();
  }
}

function localOf(id) {
  const t = entity(id)?.components?.transform || {};
  return M.fromTRS(t.translation, t.rotation, t.scale);
}

/** The entity's world matrix as the document has it now (parents as currently drawn). */
function worldFromDoc(id) {
  const e = entity(id);
  const parent = e?.parent;
  const parentWorld = parent && state.records.has(parent) ? state.current[state.records.get(parent)] : M.IDENTITY;
  return M.multiply(parentWorld, localOf(id));
}

/** Moves entity `id` (and everything beneath it) so its world matrix becomes `world`. */
function placeEntity(id, world) {
  const r = state.records.get(id);
  if (r === undefined || !state.engine) return;
  const inv = M.invert(state.current[r]);
  if (!inv) {
    // Flattened (a scale of 0), it cannot be moved by a difference: cook the scene again when
    // the document has changed since this cook (not while a drag is still going on).
    if (state.doc && state.doc.rev > state.cooked.rev && !state.drag) requestCook(150);
    return;
  }
  const d = M.multiply(world, inv);
  const recs = state.cooked.entities;
  const under = [r];
  for (let i = r + 1; i < recs.length; i++) {
    let p = recs[i].parent;
    while (p !== null && p !== undefined && p > r) p = recs[p].parent;
    if (p === r) under.push(i);
  }
  for (const k of under) {
    state.current[k] = M.multiply(d, state.current[k]);
    const [first, count] = recs[k].instances;
    for (let j = first; j < first + count; j++) {
      state.inst[j] = M.multiply(d, state.inst[j]);
      state.engine.setWorldInstance(j, state.inst[j], true);
    }
    const [lf, lc] = recs[k].lights;
    for (let l = lf; l < lf + lc; l++) {
      if (!state.lamps[l]) continue;
      state.lamps[l] = M.point(d, state.lamps[l]);
      state.engine.setWorldLight(l, state.lamps[l]);
    }
  }
}

function eye() {
  const v = state.view;
  const c = Math.cos(v.pitch);
  return [
    v.target[0] + v.distance * c * Math.sin(v.yaw),
    v.target[1] + v.distance * Math.sin(v.pitch),
    v.target[2] + v.distance * c * Math.cos(v.yaw),
  ];
}

function boundsOf(id) {
  const r = state.records.get(id);
  if (r === undefined) return null;
  const recs = state.cooked.entities;
  let lo = [Infinity, Infinity, Infinity];
  let hi = [-Infinity, -Infinity, -Infinity];
  const add = (k) => {
    const b = recs[k].bounds;
    if (!b) return;
    // The cooked bounds moved by how far the record has moved since.
    const d = M.multiply(state.current[k], M.invert(recs[k].transform) || M.IDENTITY);
    for (const x of [b[0][0], b[1][0]]) for (const y of [b[0][1], b[1][1]]) for (const z of [b[0][2], b[1][2]]) {
      const p = M.point(d, [x, y, z]);
      lo = lo.map((v, i) => Math.min(v, p[i]));
      hi = hi.map((v, i) => Math.max(v, p[i]));
    }
  };
  add(r);
  for (let i = r + 1; i < recs.length; i++) {
    let p = recs[i].parent;
    while (p !== null && p !== undefined && p > r) p = recs[p].parent;
    if (p === r) add(i);
  }
  return lo[0] <= hi[0] ? [lo, hi] : null;
}

function frameBounds(b) {
  if (!b) return;
  const c = [0, 1, 2].map((i) => (b[0][i] + b[1][i]) / 2);
  const size = Math.max(0.5, Math.hypot(b[1][0] - b[0][0], b[1][1] - b[0][1], b[1][2] - b[0][2]));
  state.view.target = c;
  state.view.distance = Math.min(2000, size * 1.4 + 1);
}

function frameAll() {
  if (!state.cooked) return;
  let lo = [Infinity, Infinity, Infinity];
  let hi = [-Infinity, -Infinity, -Infinity];
  for (const r of state.cooked.entities) {
    if (!r.bounds) continue;
    lo = lo.map((v, i) => Math.min(v, r.bounds[0][i]));
    hi = hi.map((v, i) => Math.max(v, r.bounds[1][i]));
  }
  if (lo[0] <= hi[0]) frameBounds([lo, hi]);
}

function project(p) {
  return state.engine ? state.engine.project(p) : null;
}

/** The record under a CSS pixel, as the nearest ancestor the document itself holds. */
function entityAt(x, y) {
  const hit = state.engine?.pickWorld(x, y);
  if (!hit) return null;
  const recs = state.cooked.entities;
  let r = recs.findIndex((rec) => hit.instance >= rec.instances[0] && hit.instance < rec.instances[0] + rec.instances[1]);
  while (r >= 0 && !entity(recs[r].id)) r = recs[r].parent ?? -1;
  return r >= 0 ? recs[r].id : null;
}

const AXES = [[1, 0, 0], [0, 1, 0], [0, 0, 1]];
const AXIS_COLOURS = ['#ff5a5a', '#6fe36f', '#5a8cff'];

function gizmo() {
  const id = state.selection;
  const r = id && state.records.get(id);
  if (r === undefined || r === null) return null;
  const m = state.current[r];
  const o = [m[12], m[13], m[14]];
  const po = project(o);
  if (!po) return null;
  const len = Math.max(0.3, state.view.distance * 0.12);
  const ends = AXES.map((a) => project([o[0] + a[0] * len, o[1] + a[1] * len, o[2] + a[2] * len]));
  return { o, po, ends, len };
}

function axisUnder(x, y) {
  const g = gizmo();
  if (!g) return -1;
  let best = -1;
  let bestD = 9;
  g.ends.forEach((e, i) => {
    if (!e) return;
    const vx = e.x - g.po.x;
    const vy = e.y - g.po.y;
    const t = Math.max(0, Math.min(1, ((x - g.po.x) * vx + (y - g.po.y) * vy) / (vx * vx + vy * vy || 1)));
    const d = Math.hypot(g.po.x + vx * t - x, g.po.y + vy * t - y);
    if (d < bestD) {
      bestD = d;
      best = i;
    }
  });
  return best;
}

function drawOverlay() {
  const c = $('#overlay');
  const box = $('#viewport').getBoundingClientRect();
  if (c.width !== Math.round(box.width) || c.height !== Math.round(box.height)) {
    c.width = Math.round(box.width);
    c.height = Math.round(box.height);
  }
  const g2 = c.getContext('2d');
  g2.clearRect(0, 0, c.width, c.height);
  if (!state.engine || !state.cooked) return;
  // Cameras: a dot and the way they look.
  for (const [id, e] of Object.entries(entities())) {
    if (!e.components?.camera || !state.records.has(id)) continue;
    const m = state.current[state.records.get(id)];
    const o = project([m[12], m[13], m[14]]);
    const f = project(M.point(m, [0, 0, -1]));
    if (!o) continue;
    g2.strokeStyle = '#e8c86a';
    g2.fillStyle = '#e8c86a';
    g2.beginPath();
    g2.arc(o.x, o.y, 4, 0, Math.PI * 2);
    g2.fill();
    if (f) {
      g2.beginPath();
      g2.moveTo(o.x, o.y);
      g2.lineTo(f.x, f.y);
      g2.stroke();
    }
  }
  const id = state.selection;
  if (!id) return;
  const b = boundsOf(id);
  if (b) {
    const corners = [];
    for (const x of [b[0][0], b[1][0]]) for (const y of [b[0][1], b[1][1]]) for (const z of [b[0][2], b[1][2]]) corners.push(project([x, y, z]));
    const edges = [[0, 1], [2, 3], [4, 5], [6, 7], [0, 2], [1, 3], [4, 6], [5, 7], [0, 4], [1, 5], [2, 6], [3, 7]];
    g2.strokeStyle = 'rgba(255, 210, 90, 0.9)';
    g2.lineWidth = 1;
    g2.beginPath();
    for (const [a, z] of edges) {
      if (!corners[a] || !corners[z]) continue;
      g2.moveTo(corners[a].x, corners[a].y);
      g2.lineTo(corners[z].x, corners[z].y);
    }
    g2.stroke();
  }
  const g = gizmo();
  if (!g) return;
  g2.lineWidth = 3;
  g.ends.forEach((e, i) => {
    if (!e) return;
    const hot = state.hover === i || state.drag?.axis === i;
    g2.strokeStyle = hot ? '#ffffff' : AXIS_COLOURS[i];
    g2.beginPath();
    g2.moveTo(g.po.x, g.po.y);
    g2.lineTo(e.x, e.y);
    g2.stroke();
    g2.fillStyle = hot ? '#ffffff' : AXIS_COLOURS[i];
    if (state.mode === 'move') {
      g2.beginPath();
      g2.arc(e.x, e.y, 5, 0, Math.PI * 2);
      g2.fill();
    } else {
      g2.fillRect(e.x - 4, e.y - 4, 8, 8);
    }
  });
}

function renderLoop(t) {
  if (state.engine) {
    try {
      state.engine.setView({ eye: eye(), target: state.view.target, fov: state.view.fov });
      state.engine.setClock(state.time, t / 1000);
      state.engine.render();
    } catch (e) {
      log(2, `viewport: ${e.message}`);
      state.engine = null;
    }
  }
  drawOverlay();
  const now = performance.now();
  state.fps = state.last ? 0.9 * state.fps + 0.1 * (1000 / Math.max(1, now - state.last)) : 0;
  state.last = now;
  requestAnimationFrame(renderLoop);
}

function startDrag(axis, x, y) {
  const id = state.selection;
  const r = state.records.get(id);
  const start = state.current[r].slice();
  const o = [start[12], start[13], start[14]];
  const po = project(o);
  const pa = project(M.point(M.fromTRS(o), AXES[axis]));
  if (!po || !pa) return;
  state.drag = { id, axis, x, y, start, o, screen: [pa.x - po.x, pa.y - po.y], world: start };
}

function dragTo(x, y, snap) {
  const d = state.drag;
  const dx = x - d.x;
  const dy = y - d.y;
  let world;
  if (state.mode === 'move') {
    const s = d.screen;
    let t = (dx * s[0] + dy * s[1]) / (s[0] * s[0] + s[1] * s[1] || 1);
    if (snap) t = Math.round(t / 0.25) * 0.25;
    const a = AXES[d.axis];
    world = M.multiply(M.fromTRS([a[0] * t, a[1] * t, a[2] * t]), d.start);
  } else if (state.mode === 'rotate') {
    let deg = dx * 0.5;
    if (snap) deg = Math.round(deg / 15) * 15;
    const half = (deg * Math.PI) / 360;
    const a = AXES[d.axis];
    const q = [a[0] * Math.sin(half), a[1] * Math.sin(half), a[2] * Math.sin(half), Math.cos(half)];
    world = M.multiply(M.fromTRS(d.o), M.multiply(M.fromTRS([0, 0, 0], q), M.multiply(M.fromTRS(d.o.map((v) => -v)), d.start)));
  } else {
    let f = Math.exp(dx / 200);
    if (snap) f = Math.max(0.25, Math.round(f * 4) / 4);
    world = M.multiply(M.fromTRS(d.o), M.multiply(M.fromTRS([0, 0, 0], [0, 0, 0, 1], [f, f, f]), M.multiply(M.fromTRS(d.o.map((v) => -v)), d.start)));
  }
  d.world = world;
  placeEntity(d.id, world);
}

function endDrag() {
  const d = state.drag;
  state.drag = null;
  if (!d) return;
  const e = entity(d.id);
  const parentWorld = e.parent && state.records.has(e.parent) ? state.current[state.records.get(e.parent)] : M.IDENTITY;
  const local = M.multiply(M.invert(parentWorld) || M.IDENTITY, d.world);
  const { t: tr, q, s } = M.decompose(local);
  const old = e.components?.transform || {};
  const same = (a, b, def) => (a || def).every((v, i) => Math.abs(v - b[i]) < 1e-6);
  const ops = [];
  const path = (f) => ['entities', d.id, 'components', 'transform', f];
  if (!same(old.translation, tr, [0, 0, 0])) ops.push({ set: path('translation'), value: tr.map((v) => M.tidy(v)) });
  if (!same(old.rotation, q, [0, 0, 0, 1])) ops.push({ set: path('rotation'), value: q.map((v) => M.tidy(v, 6)) });
  if (!same(old.scale, s, [1, 1, 1])) ops.push({ set: path('scale'), value: s.map((v) => M.tidy(v)) });
  if (!ops.length) return;
  const verb = { move: t('vMove'), rotate: t('vRotate'), scale: t('vScale') }[state.mode];
  edit(`${verb} ${e.name}`, ops).then((answer) => {
    // A refused (or unsent) edit: put the entity back where the document has it.
    if (answer?.type !== 'ops') placeEntity(d.id, worldFromDoc(d.id));
  });
}

function viewportEvents() {
  const vp = $('#viewport');
  let orbit = null;
  vp.addEventListener('contextmenu', (e) => e.preventDefault());
  vp.addEventListener('pointerdown', (e) => {
    const r = vp.getBoundingClientRect();
    const x = e.clientX - r.left;
    const y = e.clientY - r.top;
    vp.setPointerCapture(e.pointerId);
    if (e.button === 0) {
      const axis = axisUnder(x, y);
      if (axis >= 0) {
        startDrag(axis, x, y);
        return;
      }
      state.click = { x, y };
    } else {
      orbit = { x: e.clientX, y: e.clientY, pan: e.button === 1 || e.shiftKey };
    }
  });
  vp.addEventListener('pointermove', (e) => {
    const r = vp.getBoundingClientRect();
    const x = e.clientX - r.left;
    const y = e.clientY - r.top;
    if (state.drag) {
      dragTo(x, y, e.ctrlKey || e.metaKey);
      return;
    }
    if (orbit) {
      const dx = e.clientX - orbit.x;
      const dy = e.clientY - orbit.y;
      orbit.x = e.clientX;
      orbit.y = e.clientY;
      const v = state.view;
      if (orbit.pan) {
        const k = v.distance * 0.0015;
        const right = [Math.cos(v.yaw), 0, -Math.sin(v.yaw)];
        v.target = [v.target[0] - right[0] * dx * k, v.target[1] + dy * k, v.target[2] - right[2] * dx * k];
      } else {
        v.yaw -= dx * 0.005;
        v.pitch = Math.max(-1.5, Math.min(1.5, v.pitch + dy * 0.005));
      }
      return;
    }
    state.hover = axisUnder(x, y);
  });
  vp.addEventListener('pointerup', (e) => {
    const r = vp.getBoundingClientRect();
    const x = e.clientX - r.left;
    const y = e.clientY - r.top;
    if (state.drag) endDrag();
    else if (state.click && Math.hypot(x - state.click.x, y - state.click.y) < 4) select(entityAt(x, y));
    state.click = null;
    orbit = null;
  });
  vp.addEventListener('wheel', (e) => {
    e.preventDefault();
    state.view.distance = Math.max(0.2, Math.min(5000, state.view.distance * Math.exp(e.deltaY * 0.001)));
  }, { passive: false });
}

/** Marks the inspector field that holds typing not yet committed (see drawInspector). */
function inspectorEvents() {
  const box = $('#inspector-body');
  box.addEventListener('input', (e) => { e.target.dataset.typed = '1'; });
  box.addEventListener('change', (e) => { delete e.target.dataset.typed; }, true);
  box.addEventListener('focusout', (e) => {
    delete e.target.dataset.typed;
    // A redraw waited for this field: draw now, once the focus has moved on.
    if (state.inspectorStale) setTimeout(() => state.inspectorStale && drawInspector());
  });
}

// ---- Panels ----------------------------------------------------------------------------------

function select(id) {
  state.selection = id;
  drawOutliner();
  drawInspector();
  if (state.doc) state.host.send({ type: 'select', doc: state.doc.id, entities: id ? [id] : [] });
}

function drawAll() {
  drawToolbar();
  drawOutliner();
  drawInspector();
}

function drawToolbar() {
  const d = state.doc;
  $('#doc-name').textContent = d ? `${d.path}${d.dirty ? ' •' : ''}` : t('noDocument');
  $('#undo').disabled = !d?.undo;
  $('#undo').title = d?.undo ? t('undoX', d.undo) : t('nothingToUndo');
  $('#redo').disabled = !d?.redo;
  $('#redo').title = d?.redo ? t('redoX', d.redo) : t('nothingToRedo');
  $('#save').disabled = !d?.dirty;
  for (const m of ['move', 'rotate', 'scale']) $(`#mode-${m}`).classList.toggle('on', state.mode === m);
}

function drawAssets() {
  const list = $('#assets-list');
  list.replaceChildren();
  const kinds = ['scene', 'prefab', 'model', 'look', 'texture', 'material', 'sound'];
  for (const kind of kinds) {
    const items = state.assets.filter((a) => a.kind === kind).sort((a, b) => a.path.localeCompare(b.path));
    if (!items.length) continue;
    list.append(el('div', { class: 'group' }, kind));
    for (const a of items) {
      const name = a.path.split('/').pop();
      const row = el('div', { class: 'asset', title: a.path, 'data-id': a.id }, name);
      if (kind === 'scene' || kind === 'prefab') {
        row.addEventListener('dblclick', () => openDoc(a.id));
        row.append(el('button', { class: 'mini', onclick: () => openDoc(a.id) }, t('open')));
      }
      if (kind === 'model') {
        row.append(el('button', {
          class: 'mini',
          onclick: () => {
            const stem = name.replace(/\.glb$/i, '');
            const path = prompt(t('newSceneFile'), `content/scenes/${stem}.scene.json`);
            if (path) state.host.send({ type: 'import', model: a.id, path, name: stem });
          },
        }, t('importAsScene')));
      }
      list.append(row);
    }
  }
}

function drawOutliner() {
  const tree = $('#outliner-tree');
  tree.replaceChildren();
  if (!state.doc) return;
  const filter = $('#outliner-filter').value.trim().toLowerCase();
  const row = (id, depth) => {
    const e = entity(id);
    const kids = children(id);
    const match = !filter || (e.name || '').toLowerCase().includes(filter);
    const r = el('div', {
      class: `node${state.selection === id ? ' selected' : ''}${match ? '' : ' dim'}`,
      style: `padding-left:${8 + depth * 14}px`,
      'data-id': id,
      onclick: () => select(id),
      ondblclick: () => rename(id),
    }, el('span', { class: 'twisty' }, kids.length ? '▾' : '·'), e.name || id,
    e.prefab ? el('span', { class: 'badge' }, 'prefab') : null);
    tree.append(r);
    for (const k of kids) row(k, depth + 1);
  };
  for (const id of children(null)) row(id, 0);
}

function rename(id) {
  const e = entity(id);
  const name = prompt(t('name'), e.name || '');
  if (name !== null && name !== e.name) edit(`${t('vRename')} ${e.name}`, [{ set: ['entities', id, 'name'], value: name }]);
}

async function addEntity() {
  if (!state.doc) return;
  const parent = state.selection;
  const [id] = (await takeIds(1)) || [];
  if (!id) return;
  const value = { name: t('entity'), components: { transform: {} } };
  if (parent) value.parent = parent;
  else value.components.transform.translation = state.view.target.map((v) => M.tidy(v, 3));
  const answer = await edit(t('vAddEntity'), [{ set: ['entities', id], value }]);
  if (answer?.type === 'ops') select(id);
}

/** Deletes the selection and everything beneath it. The host works out the ops, so entities
 * added under what a prefab instance shows go with the instance. */
function deleteSelection() {
  const id = state.selection;
  if (!id) return;
  const label = `${t('vDelete')} ${entity(id).name}`;
  queued((doc) => ({ type: 'delete', doc: doc.id, rev: doc.rev, label, ids: [id] })).then((answer) => {
    if (answer?.type === 'error') log(2, `${label}: ${answer.message}`);
  });
  select(null);
}

async function duplicateSelection() {
  const id = state.selection;
  if (!id) return;
  const ids = subtree(id);
  const fresh = await takeIds(ids.length);
  if (!fresh || !ids.every((x) => entity(x))) return;
  const map = new Map(ids.map((x, i) => [x, fresh[i]]));
  const ops = ids.map((x) => {
    const e = structuredClone(entity(x));
    if (x === id) {
      e.name = `${e.name} ${t('copy')}`;
      delete e.order;
    } else if (map.has(e.parent)) e.parent = map.get(e.parent);
    return { set: ['entities', map.get(x)], value: e };
  });
  const answer = await edit(`${t('vDuplicate')} ${entity(id).name}`, ops);
  if (answer?.type === 'ops') select(map.get(id));
}

function fieldWidget(id, kind, name, schema, stored) {
  const def = schema.default;
  const value = stored === undefined ? def : stored;
  const commit = (v, merge) => setField(id, kind, name, v, merge);
  const ty = schema.type;
  const unit = schema.unit ? ` ${schema.unit}` : '';
  // An emptied or unreadable number commits nothing (it would read as 0): the field shows the
  // last value again.
  const num = (v, onchange, step = 'any') => {
    let last = v ?? 0;
    return el('input', { type: 'number', step, value: String(last), onchange: (e) => {
      const n = e.target.value.trim() === '' ? NaN : Number(e.target.value);
      if (!Number.isFinite(n)) {
        e.target.value = String(last);
        return;
      }
      last = n;
      onchange(n);
    } });
  };
  let input;
  if (ty === 'bool') {
    input = el('input', { type: 'checkbox', onchange: (e) => commit(e.target.checked) });
    input.checked = !!value;
  } else if (ty === 'float' || ty === 'int') {
    input = num(value, (v) => commit(ty === 'int' ? Math.round(v) : v), ty === 'int' ? '1' : 'any');
  } else if (ty === 'string') {
    input = el('input', { type: 'text', value: value ?? '', onchange: (e) => commit(e.target.value) });
  } else if (ty === 'enum') {
    input = el('select', { onchange: (e) => commit(e.target.value) }, schema.values.map((v) => el('option', { value: v, selected: v === value }, v)));
  } else if (ty === 'vec2' || ty === 'vec3' || ty === 'vec4') {
    // No value and no default (a new child's translation): zeros, so it can be typed.
    const v = (value ?? new Array(Number(ty.slice(3))).fill(0)).slice();
    input = el('span', { class: 'vec' }, v.map((x, i) => num(x, (n) => {
      v[i] = n;
      commit(v.slice());
    })));
  } else if (ty === 'quat') {
    const eul = M.eulerFromQuat(value || [0, 0, 0, 1]);
    input = el('span', { class: 'vec', title: t('eulerTip') }, eul.map((x, i) => num(x, (n) => {
      eul[i] = n;
      commit(M.quatFromEuler(eul).map((q) => M.tidy(q, 6)));
    })));
  } else if (ty === 'rgb' || ty === 'rgba') {
    const v = (value || [1, 1, 1, 1]).slice();
    // Each part keeps `v` up to date, so the other part commits it whole.
    const color = el('input', { type: 'color', value: M.hexOfLinear(v), onchange: (e) => {
      const c = M.linearOfHex(e.target.value);
      v.splice(0, 3, ...c);
      commit(ty === 'rgba' ? [...c, v[3] ?? 1] : c);
    } });
    input = el('span', { class: 'vec' }, color, ty === 'rgba' ? num(v[3] ?? 1, (a) => {
      v[3] = a;
      commit([v[0], v[1], v[2], a]);
    }) : null);
  } else if (ty === 'asset') {
    const options = state.assets.filter((a) => !schema.kind || a.kind === schema.kind);
    input = el('select', { onchange: (e) => commit(e.target.value || null) },
      el('option', { value: '' }, t('none')),
      options.map((a) => el('option', { value: a.id, selected: a.id === value }, a.path.split('/').pop())));
  } else {
    input = el('input', { type: 'text', value: JSON.stringify(value ?? null), onchange: (e) => {
      try { commit(JSON.parse(e.target.value)); } catch (err) { log(2, `${name}: ${err.message}`); }
    } });
  }
  const reset = stored !== undefined
    ? el('button', { class: 'mini', title: t('resetDefault'), onclick: () => edit(`${t('vReset')} ${name}`, [{ remove: ['entities', id, 'components', kind, name] }]) }, '↺')
    : null;
  return el('div', { class: `field${stored === undefined ? ' default' : ''}`, title: schema.doc || '', 'data-key': `${kind}.${name}` },
    el('label', {}, `${label(schema, name)}${unit}`), input, reset);
}

const CONTROLS = 'input, select, button';
const controlsOf = (row) => (row.matches(CONTROLS) ? [row] : [...row.querySelectorAll(CONTROLS)]);
const valuesOf = (row) => JSON.stringify(controlsOf(row).map((c) => [c.tagName, c.type, c.value, c.checked]));

/**
 * Draws the inspector for the selection. Keyboard focus in the panel stays put: the focused row
 * is kept when it would be drawn the same (its caret and selection stay too), otherwise the
 * focus moves to the same control of the new row, otherwise to the panel itself, so a key
 * meant for a field never reaches the shortcuts. While a field holds typing not yet committed,
 * the panel is left alone until the field is committed or left.
 */
function drawInspector() {
  const box = $('#inspector-body');
  const id = state.selection;
  const focused = box.contains(document.activeElement) ? document.activeElement : null;
  const same = box.dataset.entity === id && !!entity(id);
  if (focused?.dataset.typed && same) {
    state.inspectorStale = true;
    return;
  }
  state.inspectorStale = false;
  const row = focused && [...box.children].find((r) => r.contains(focused));
  const at = row ? controlsOf(row).indexOf(focused) : -1;
  buildInspector(box, id);
  if (!focused) return;
  const fresh = same && row?.dataset.key ? [...box.children].find((r) => r.dataset.key === row.dataset.key) : null;
  let target = box;
  if (fresh && valuesOf(fresh) === valuesOf(row)) {
    fresh.replaceWith(row);
    target = focused;
  } else if (fresh) {
    target = controlsOf(fresh)[at] || box;
  }
  target.focus({ preventScroll: true });
}

function buildInspector(box, id) {
  box.replaceChildren();
  const e = id && entity(id);
  box.dataset.entity = e ? id : '';
  if (!e) {
    box.append(el('p', { class: 'hint' }, state.doc ? t('selectHint') : t('openHint')));
    return;
  }
  box.append(el('div', { class: 'field', 'data-key': '#name' }, el('label', {}, t('name')),
    el('input', { type: 'text', value: e.name || '', onchange: (ev) => edit(t('vRename'), [{ set: ['entities', id, 'name'], value: ev.target.value }]) })));
  box.append(el('div', { class: 'meta' }, `id ${id}`));
  const comps = e.components || {};
  for (const [kind, stored] of Object.entries(comps).sort(([a], [b]) => a.localeCompare(b))) {
    const schema = state.kinds[kind];
    const head = el('div', { class: 'component', 'data-key': `${kind}#` }, el('span', {}, label(schema, kind)),
      el('button', { class: 'mini', title: t('removeComponent'), onclick: () => edit(`${t('vRemove')} ${kind}`, [{ remove: ['entities', id, 'components', kind] }]) }, '✕'));
    box.append(head);
    if (!schema) {
      box.append(el('p', { class: 'hint' }, t('noSchema', kind)));
      continue;
    }
    for (const [name, f] of Object.entries(schema.fields || {}).sort(([a], [b]) => a.localeCompare(b))) {
      if (f.hidden) continue;
      box.append(fieldWidget(id, kind, name, f, stored[name]));
    }
    if (kind === 'camera') {
      box.append(el('button', { 'data-key': `${kind}#look`, onclick: () => lookThrough(id) }, t('lookThrough')));
    }
  }
  const missing = Object.keys(state.kinds).filter((k) => !comps[k]).sort();
  if (missing.length) {
    box.append(el('select', {
      class: 'add',
      'data-key': '#add',
      onchange: (ev) => {
        const kind = ev.target.value;
        if (kind) edit(`${t('vAdd')} ${kind}`, [{ set: ['entities', id, 'components', kind], value: {} }]);
      },
    }, el('option', { value: '' }, t('addComponent')), missing.map((k) => el('option', { value: k }, label(state.kinds[k], k)))));
  }
}

function lookThrough(id) {
  const m = state.current[state.records.get(id)];
  if (!m) return;
  const o = [m[12], m[13], m[14]];
  const f = M.vector(m, [0, 0, -1]);
  const n = Math.hypot(...f) || 1;
  state.view.target = o.map((v, i) => v + (f[i] / n) * 5);
  state.view.distance = 5;
  state.view.yaw = Math.atan2(-f[0], -f[2]);
  state.view.pitch = Math.asin(Math.max(-1, Math.min(1, -f[1] / n)));
  const cam = entity(id).components.camera;
  if (cam.fov) state.view.fov = cam.fov;
}

// ---- Commands and keys ------------------------------------------------------------------------

/** Delete and Backspace delete the selection only where it was chosen: the viewport or the
 * outliner, never a field or button elsewhere. */
const inScene = (node) => !!node && ($('#viewport').contains(node) || $('#outliner').contains(node));

function commands() {
  $('#save').onclick = save;
  $('#undo').onclick = () => undoRedo('undo');
  $('#redo').onclick = () => undoRedo('redo');
  $('#mode-move').onclick = () => setMode('move');
  $('#mode-rotate').onclick = () => setMode('rotate');
  $('#mode-scale').onclick = () => setMode('scale');
  $('#add-entity').onclick = addEntity;
  $('#delete-entity').onclick = deleteSelection;
  $('#duplicate-entity').onclick = duplicateSelection;
  $('#frame').onclick = () => frameBounds(state.selection ? boundsOf(state.selection) : null);
  $('#recook').onclick = () => requestCook(0);
  $('#new-scene').onclick = () => {
    const path = prompt(t('newSceneFile'), 'content/scenes/new.scene.json');
    if (path) state.host.send({ type: 'create', kind: 'scene', path, name: path.split('/').pop().replace('.scene.json', '') });
  };
  $('#outliner-filter').oninput = drawOutliner;
  $('#time').oninput = (e) => {
    state.time = Number(e.target.value);
    $('#time-label').textContent = `${state.time.toFixed(1)} h`;
  };
  window.addEventListener('keydown', (e) => {
    const typing = ['INPUT', 'SELECT', 'TEXTAREA'].includes(document.activeElement?.tagName);
    const mod = e.ctrlKey || e.metaKey;
    if (mod && e.key.toLowerCase() === 's') {
      e.preventDefault();
      save();
      return;
    }
    if (typing) return;
    if (mod && e.key.toLowerCase() === 'z') {
      e.preventDefault();
      undoRedo(e.shiftKey ? 'redo' : 'undo');
    } else if (mod && e.key.toLowerCase() === 'y') {
      e.preventDefault();
      undoRedo('redo');
    } else if (mod && e.key.toLowerCase() === 'd') {
      e.preventDefault();
      duplicateSelection();
    } else if (e.key === 'Delete' || e.key === 'Backspace') {
      if (inScene(document.activeElement)) deleteSelection();
    } else if (e.key === 'f') {
      frameBounds(state.selection ? boundsOf(state.selection) : null);
    } else if (e.key === 'w') setMode('move');
    else if (e.key === 'e') setMode('rotate');
    else if (e.key === 'r') setMode('scale');
    else if (e.key === 'Escape') select(null);
  });
}

function setMode(m) {
  state.mode = m;
  drawToolbar();
}

// ---- Start -----------------------------------------------------------------------------------

async function main() {
  translate();
  $('#language').onclick = () => {
    setLang(lang() === 'ko' ? 'en' : 'ko');
    translate();
    drawAll();
    drawAssets();
  };
  commands();
  viewportEvents();
  inspectorEvents();
  requestAnimationFrame(renderLoop);
  const token = new URLSearchParams(location.search).get('token');
  const url = `${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}/api/ws${token ? `?token=${token}` : ''}`;
  state.host = new Host(url, onMessage, (s) => {
    $('#connection').textContent = t(s);
    $('#connection').className = s;
    if (s === 'disconnected') {
      log(1, 'the studio host closed the connection; connecting again');
      state.onIds?.(); // whoever waits for ids gives up (see takeIds)
    }
    // The first connection, and each one after the host went away, starts with hello.
    if (s === 'connected') state.host.send({ type: 'hello', client: 'editor' });
  });
  setInterval(() => {
    $('#fps').textContent = state.engine ? `${state.fps.toFixed(0)} fps` : '';
  }, 500);
  await state.host.connect();
}

main().catch((e) => log(2, e.message));
