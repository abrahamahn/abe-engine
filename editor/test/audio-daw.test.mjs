import assert from 'node:assert/strict';
import test from 'node:test';

import { extractWavPeaks } from '../asset-previews.js';
import {
  MASTER_BUS_ID,
  analyzeStereoFrames,
  bounceEventToWavBytes,
  buildApplyPresetOps,
  buildPlaceLayerOp,
  computeMidSideMatrix,
  createDefaultStrip,
  createStarterSoundBank,
  decodeQoaBytes,
  evaluateFadeCurve,
  evaluateStripResponseDb,
  mountAudioDaw,
  normalizeSoundBankRoot,
  orderedBuses,
  orderedLayers,
  validateSoundBankRoot,
} from '../audio-daw.js';
import { applyOps } from '../protocol.js';

// Minimal zero-dependency DOM shim so `mountAudioDaw(container, studioApi)` runs in pure Node.
function createFakeDocument() {
  class FakeNode {
    constructor(tag, ownerDocument) {
      this.tagName = String(tag).toUpperCase();
      this.ownerDocument = ownerDocument;
      this.attributes = new Map();
      this.children = [];
      this.parentNode = null;
      this.listeners = new Map();
      this.className = '';
      this.value = '';
      this.textContent = '';
    }
    setAttribute(k, v) {
      this.attributes.set(k, String(v));
      if (k === 'id') this.id = String(v);
    }
    getAttribute(k) {
      return this.attributes.get(k) ?? null;
    }
    addEventListener(type, fn) {
      if (!this.listeners.has(type)) this.listeners.set(type, []);
      this.listeners.get(type).push(fn);
    }
    removeEventListener(type, fn) {
      const arr = this.listeners.get(type);
      if (!arr) return;
      const idx = arr.indexOf(fn);
      if (idx >= 0) arr.splice(idx, 1);
    }
    dispatchEvent(type, eventObj = {}) {
      const fns = [...(this.listeners.get(type) || [])];
      const evt = { target: this, stopPropagation() {}, ...eventObj };
      for (const fn of fns) fn(evt);
    }
    click() {
      this.dispatchEvent('click');
    }
    append(...nodes) {
      for (const n of nodes) {
        if (n == null) continue;
        if (n.parentNode) n.parentNode.removeChild(n);
        n.parentNode = this;
        this.children.push(n);
      }
    }
    removeChild(child) {
      const i = this.children.indexOf(child);
      if (i >= 0) {
        this.children.splice(i, 1);
        child.parentNode = null;
      }
    }
    remove() {
      if (this.parentNode) this.parentNode.removeChild(this);
    }
    replaceChildren(...nodes) {
      for (const c of this.children) c.parentNode = null;
      this.children = [];
      this.append(...nodes);
    }
    queryByAttr(attr, val) {
      if (this.attributes?.get(attr) === val) return this;
      for (const c of this.children || []) {
        if (typeof c.queryByAttr === 'function') {
          const hit = c.queryByAttr(attr, val);
          if (hit) return hit;
        }
      }
      return null;
    }
    queryAllByTag(tag, out = []) {
      if (this.tagName === String(tag).toUpperCase()) out.push(this);
      for (const c of this.children || []) {
        if (typeof c.queryAllByTag === 'function') c.queryAllByTag(tag, out);
      }
      return out;
    }
  }

  const doc = {
    head: null,
    createElement(tag) {
      return new FakeNode(tag, doc);
    },
    createElementNS(_ns, tag) {
      return new FakeNode(tag, doc);
    },
    createTextNode(text) {
      return { nodeType: 3, textContent: String(text), parentNode: null };
    },
    getElementById(id) {
      return doc.head?.queryByAttr('id', id) || null;
    },
    addEventListener() {},
    removeEventListener() {},
  };
  doc.head = new FakeNode('head', doc);
  const container = new FakeNode('div', doc);
  return { doc, container };
}

test('starter SoundBank normalizes arrays and keyed objects and validates cleanly', () => {
  const starter = createStarterSoundBank();
  assert.deepEqual(validateSoundBankRoot(starter), []);
  const buses = orderedBuses(starter);
  assert.equal(buses.at(-1).id, MASTER_BUS_ID);

  // Array input normalizes to canonical keyed map
  const fromArrays = normalizeSoundBankRoot({
    ganbate: 'soundbank',
    version: 1,
    id: '11223344556677889900aabbccddeeff',
    name: 'RPG Foley',
    buses: [
      { id: 'combat', label: 'Combat' },
      { id: 'master', label: 'Master', role: 'master' },
    ],
    assets: [
      {
        id: 'slash_01',
        bus: 'combat',
        path: 'content/audio/slash_01.wav',
        defaultDurationMs: 180,
        transientOffsetMs: 12,
      },
    ],
    events: [
      {
        id: 'evt_slash',
        bus: 'combat',
        durationMs: 400,
        contactMarkerMs: 90,
        layers: [{ id: 'l1', assetId: 'slash_01', bus: 'combat', startMs: 78, durationMs: 180 }],
      },
    ],
  });
  assert.deepEqual(validateSoundBankRoot(fromArrays), []);
  assert.ok(fromArrays.buses.combat);
  assert.equal(fromArrays.events.evt_slash.layers.l1.startMs, 78);
});

test('DSP math, Mid/Side width, biquad EQ response, fade curves, QOA decode, and 48kHz WAV bounce work', () => {
  assert.deepEqual(computeMidSideMatrix('stereo', 0), [[0.5, 0.5], [0.5, 0.5]]);
  assert.deepEqual(computeMidSideMatrix('stereo', 100), [[1, 0], [0, 1]]);
  assert.deepEqual(computeMidSideMatrix('stereo', 200), [[1.5, -0.5], [-0.5, 1.5]]);

  const strip = createDefaultStrip();
  strip.highPass = { enabled: true, frequencyHz: 200, q: 0.707 };
  strip.eq = {
    enabled: true,
    lowGainDb: 0,
    lowFreqHz: 120,
    midGainDb: 6,
    midFreqHz: 1500,
    midQ: 1.2,
    highGainDb: -3,
    highFreqHz: 8000,
  };
  const [low40, mid1500, high15k] = evaluateStripResponseDb(strip, [40, 1500, 15000], 48000);
  assert.ok(low40 < -12, `expected HPF attenuation at 40 Hz, got ${low40}`);
  assert.ok(mid1500 > 5, `expected +6 dB peaking boost at 1500 Hz, got ${mid1500}`);
  assert.ok(high15k < -2, `expected high-shelf cut at 15 kHz, got ${high15k}`);

  for (const curve of ['linear', 'cosine', 's-curve']) {
    assert.equal(evaluateFadeCurve(0, curve), 0);
    assert.ok(Math.abs(evaluateFadeCurve(0.5, curve) - 0.5) < 1e-6);
    assert.ok(Math.abs(evaluateFadeCurve(1, curve) - 1) < 1e-6);
  }

  // Antiphase stereo correlation = -1
  const l = new Float32Array([0.5, -0.5, 0.5, -0.5]);
  const r = new Float32Array([-0.5, 0.5, -0.5, 0.5]);
  assert.ok(analyzeStereoFrames(l, r).correlation < -0.99);

  // Synthetic 1-slice mono QOA packet decodes cleanly
  const qoa = new Uint8Array(8 + 8 + 16 + 8);
  const dv = new DataView(qoa.buffer);
  dv.setUint32(0, 0x716f6166, false); // "qoaf"
  dv.setUint32(4, 20, false); // 20 samples
  dv.setUint32(8, (1 << 24) | 48000, false); // 1 ch, 48000 Hz
  dv.setUint16(12, 20, false); // fsamples = 20
  dv.setUint16(14, 8 + 16 + 8, false); // fsize = 32
  // LMS weights: [0, 0, -8192, 16384]
  dv.setInt16(28, -8192, false);
  dv.setInt16(30, 16384, false);
  // Slice: scalefactor=4, alternating residuals
  dv.setUint32(32, 0x4aaab555, false);
  dv.setUint32(36, 0xaaab5550, false);
  const decodedQoa = decodeQoaBytes(qoa);
  assert.ok(decodedQoa);
  assert.equal(decodedQoa.sampleRate, 48000);
  assert.equal(decodedQoa.left.length, 20);

  // Offline 48 kHz 16-bit WAV bounce produces valid RIFF/WAVE decodable by asset-previews
  const starter = createStarterSoundBank();
  const wav = bounceEventToWavBytes(starter, 'evt_surface_hit');
  assert.ok(wav && wav.byteLength > 44);
  const meta = extractWavPeaks(wav, 32);
  assert.ok(meta, 'bounced WAV should parse cleanly');
  assert.equal(meta.sampleRate, 48000);
  assert.equal(meta.channels, 2);
});

test('mountAudioDaw integrates with StudioWorkbenchApi ops, undo/redo/save, and PIE cue bridge', async () => {
  const { container } = createFakeDocument();
  const initialRoot = createStarterSoundBank({
    id: 'aabbccddeeff00112233445566778899',
    name: 'Dungeon Foley Bank',
  });

  const history = [];
  const redoStack = [];
  let savedCount = 0;
  const pieCues = [];

  const docState = {
    id: initialRoot.id,
    path: 'content/audio/dungeon.soundbank.json',
    kind: 'soundbank',
    rev: 1,
    dirty: false,
    undo: null,
    redo: null,
    root: structuredClone(initialRoot),
  };

  const listeners = new Map();
  const studioApi = {
    getLocale: () => 'en',
    t: (k) => k,
    getAssets: () => [
      { id: docState.id, kind: 'soundbank', path: docState.path, bytes: 2048 },
      { id: 'snd_torch', kind: 'sound', path: 'content/audio/torch_crackle.wav', bytes: 8192 },
    ],
    listDocuments: ({ suffix } = {}) =>
      !suffix || docState.path.endsWith(suffix)
        ? [{ id: docState.id, kind: 'soundbank', path: docState.path, bytes: 2048 }]
        : [],
    getActiveDocument: () => structuredClone(docState),
    openDocument: async () => structuredClone(docState),
    assetUrl: (p) => `/project/${p}`,
    edit: async (_id, ops, label) => {
      history.push({ label, snapshot: structuredClone(docState.root) });
      redoStack.length = 0;
      applyOps(docState.root, ops);
      docState.rev += 1;
      docState.dirty = true;
      docState.undo = label || 'edit';
      docState.redo = null;
      return structuredClone(docState);
    },
    undo: async () => {
      const prev = history.pop();
      if (!prev) return null;
      redoStack.push({ label: prev.label, snapshot: structuredClone(docState.root) });
      docState.root = prev.snapshot;
      docState.rev += 1;
      docState.undo = history.at(-1)?.label ?? null;
      docState.redo = prev.label;
      return structuredClone(docState);
    },
    redo: async () => {
      const next = redoStack.pop();
      if (!next) return null;
      history.push({ label: next.label, snapshot: structuredClone(docState.root) });
      docState.root = next.snapshot;
      docState.rev += 1;
      docState.undo = next.label;
      docState.redo = redoStack.at(-1)?.label ?? null;
      return structuredClone(docState);
    },
    save: async () => {
      savedCount += 1;
      docState.dirty = false;
      return true;
    },
    sendPieCue: (cue) => pieCues.push(cue),
    on: (ev, fn) => {
      listeners.set(ev, fn);
      return () => listeners.delete(ev);
    },
  };

  const daw = mountAudioDaw(container, studioApi);
  assert.equal(daw.getState().docId, initialRoot.id);
  assert.equal(orderedLayers(daw.getState().root.events.evt_surface_hit).length, 2);

  // Place a multi-take pool onto the timeline at contactMarkerMs (140ms) - transientOffsetMs (18ms) = 122ms
  const placeBtn = container.queryByAttr('data-action', 'place-pool-foley_impact');
  assert.ok(placeBtn, 'Pool + Layer button rendered');
  placeBtn.click();
  await new Promise((r) => setTimeout(r, 5));

  const layersAfterPlace = orderedLayers(docState.root.events.evt_surface_hit);
  assert.equal(layersAfterPlace.length, 3);
  assert.equal(layersAfterPlace.at(-1).startMs, 122);

  // Apply Broadcast Punch preset via toolbar button
  const presetOps = buildApplyPresetOps(docState.root, 'broadcast-punch');
  assert.ok(presetOps.length > 0);
  await studioApi.edit(docState.id, presetOps, 'Apply preset');
  daw.setDocument(docState.id, structuredClone(docState));
  assert.equal(daw.getState().root.buses.master.strip.compressor.enabled, true);

  // Edit M/S stereo width slider on active clip/bus
  const widthInput = container.queryByAttr('data-dsp', 'stereoWidthPct');
  assert.ok(widthInput, 'M/S Stereo Width slider rendered');
  widthInput.value = '165';
  widthInput.dispatchEvent('input');
  await new Promise((r) => setTimeout(r, 5));

  // Trigger event -> fires PIE cue bridge and updates meters
  await daw.auditionEvent('evt_surface_hit');
  assert.equal(pieCues.length, 1);
  assert.equal(pieCues[0].cueId, 'surface_hit');
  assert.equal(pieCues[0].track, 'audio');
  assert.ok(daw.getState().meters.master.peakLDb > -50);

  // Undo, Redo, and Save toolbar actions round-trip through StudioWorkbenchApi
  const revBeforeUndo = docState.rev;
  container.queryByAttr('data-action', 'undo').click();
  await new Promise((r) => setTimeout(r, 5));
  assert.equal(docState.rev, revBeforeUndo + 1);

  container.queryByAttr('data-action', 'redo').click();
  await new Promise((r) => setTimeout(r, 5));
  assert.equal(docState.rev, revBeforeUndo + 2);

  container.queryByAttr('data-action', 'save').click();
  await new Promise((r) => setTimeout(r, 5));
  assert.equal(savedCount, 1);
  assert.equal(daw.getState().dirty, false);

  // Bounce WAV directly from instance
  const bounced = daw.bounceEventWav('evt_surface_hit');
  assert.ok(bounced && bounced.byteLength > 1000);

  daw.destroy();
  assert.equal(listeners.size, 0, 'all StudioWorkbenchApi subscriptions cleaned up on destroy');
});
