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
  createRotaryKnob,
  createStarterSoundBank,
  createVerticalFader,
  decodeQoaBytes,
  describeKnobArc,
  evaluateFadeCurve,
  evaluateStripResponseDb,
  faderNormToVolume,
  mountAudioDaw,
  normalizeSoundBankRoot,
  orderedBuses,
  orderedLayers,
  validateSoundBankRoot,
  volumeToFaderNorm,
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
    getBoundingClientRect() {
      return { left: 0, top: 0, right: 600, bottom: 124, width: 600, height: 124 };
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

  const docListeners = new Map();
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
    addEventListener(type, fn) {
      if (!docListeners.has(type)) docListeners.set(type, []);
      docListeners.get(type).push(fn);
    },
    removeEventListener(type, fn) {
      const arr = docListeners.get(type);
      if (!arr) return;
      const idx = arr.indexOf(fn);
      if (idx >= 0) arr.splice(idx, 1);
    },
    dispatchEvent(type, eventObj = {}) {
      const fns = [...(docListeners.get(type) || [])];
      const evt = { preventDefault() {}, stopPropagation() {}, ...eventObj };
      for (const fn of fns) fn(evt);
    },
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

test('hardware rotary knob and vertical fader support vertical drag, Shift 10x precision, dblclick reset, keyboard, and bipolar arcs', async () => {
  const { doc, container } = createFakeDocument();

  // 1. Standalone bipolar rotary knob: drag, Shift+Drag 10x fine precision, dblclick reset, keyboard
  const observed = [];
  const knobRoot = createRotaryKnob(doc, {
    label: 'EQ Mid Gain',
    value: 0,
    min: -18,
    max: 18,
    step: 0.5,
    defaultValue: 0,
    bipolar: true,
    accent: 'gold',
    format: (v) => `${v >= 0 ? '+' : ''}${v.toFixed(1)} dB`,
    onChange: (v) => observed.push(v),
  });
  const dial = knobRoot.queryByAttr('role', 'slider');
  assert.ok(dial);
  assert.equal(dial.getAttribute('data-bipolar'), 'true');
  assert.equal(dial.getAttribute('aria-valuetext'), '+0.0 dB');
  assert.ok(describeKnobArc(22, 22, 18.2, 0, 90).startsWith('M '));

  // Normal vertical drag up by 66px (half of 132px full span => +0.5 * 36 = +18 dB)
  dial.dispatchEvent('pointerdown', { clientY: 200, shiftKey: false, preventDefault() {} });
  doc.dispatchEvent('pointermove', { clientY: 167, shiftKey: false }); // +33px = 0.25 * 36 = +9.0 dB
  doc.dispatchEvent('pointerup', {});
  assert.equal(observed.at(-1), 9);
  assert.equal(dial.getAttribute('aria-valuetext'), '+9.0 dB');

  // Double-click resets immediately to defaultValue (0 dB)
  dial.dispatchEvent('dblclick', { preventDefault() {} });
  assert.equal(observed.at(-1), 0);

  // Shift+Drag 10x fine precision: +33px upward moves only 0.025 * 36 = +0.9 dB (10x finer than +9.0 dB!)
  dial.dispatchEvent('pointerdown', { clientY: 200, shiftKey: true, preventDefault() {} });
  doc.dispatchEvent('pointermove', { clientY: 167, shiftKey: true });
  doc.dispatchEvent('pointerup', {});
  assert.equal(observed.at(-1), 0.9);

  // Keyboard ArrowUp (+0.5 dB -> 1.4 -> 1.5 quantized) & Home reset (0 dB)
  dial.dispatchEvent('keydown', { key: 'ArrowUp', shiftKey: false, preventDefault() {} });
  assert.equal(observed.at(-1), 1.5);
  dial.dispatchEvent('keydown', { key: 'Home', shiftKey: false, preventDefault() {} });
  assert.equal(observed.at(-1), 0);

  // 2. Studio console vertical fader law & unity reset
  assert.ok(Math.abs(volumeToFaderNorm(1.0, 2.0) - Math.SQRT1_2) < 1e-6);
  assert.ok(Math.abs(faderNormToVolume(volumeToFaderNorm(1.0, 2.0), 2.0) - 1.0) < 1e-3);

  let faderVal = 0.5;
  const faderRoot = createVerticalFader(doc, {
    ariaLabel: 'SFX volume',
    value: faderVal,
    max: 2.0,
    defaultValue: 1.0,
    busId: 'sfx',
    onChange: (v) => {
      faderVal = v;
    },
  });
  const faderTrack = faderRoot.queryByAttr('role', 'slider');
  faderTrack.dispatchEvent('dblclick', { preventDefault() {} });
  assert.equal(faderVal, 1.0);

  // 3. Mounted workbench exposes 3D Spatializer knobs & illuminated LOOP / BYP buttons
  const daw = mountAudioDaw(container, {});
  const spatialMaxKnob = container.queryByAttr('data-dsp', 'spatial.maxDistance');
  assert.ok(spatialMaxKnob, '3D Spatializer Max Distance rotary knob rendered');
  spatialMaxKnob.dispatchEvent('pointerdown', { clientY: 200, shiftKey: false, preventDefault() {} });
  doc.dispatchEvent('pointermove', { clientY: 150, shiftKey: false });
  doc.dispatchEvent('pointerup', {});
  assert.ok(daw.getState().root.buses.master.strip.spatial.maxDistance > 35);

  const loopBtn = container.queryByAttr('data-action', 'toggle-loop');
  assert.ok(loopBtn);
  assert.equal(loopBtn.getAttribute('aria-pressed'), 'false');
  loopBtn.click();
  assert.equal(container.queryByAttr('data-action', 'toggle-loop').getAttribute('aria-pressed'), 'true');
  daw.destroy();
});

test('continuous multi-step mouse dragging on clips, resize/fade handles, contact line, slider bars, knobs, and faders preserves DOM identity mid-drag', () => {
  const { doc, container } = createFakeDocument();
  const daw = mountAudioDaw(container, {});

  // 1. Timeline clip body move across multiple pointermove steps without detaching DOM node
  const clipEl = container.queryByAttr('data-layer-id', 'lyr_contact');
  assert.ok(clipEl, 'Contact foley clip bar rendered');
  const origStart = daw.getState().root.events.evt_surface_hit.layers.lyr_contact.startMs;

  clipEl.dispatchEvent('pointerdown', { clientX: 100, preventDefault() {} });
  doc.dispatchEvent('pointermove', { clientX: 136 }); // +36px at 0.72 px/ms = +50ms
  assert.equal(container.queryByAttr('data-layer-id', 'lyr_contact'), clipEl, 'clipEl stays mounted mid-drag');
  assert.equal(daw.getState().root.events.evt_surface_hit.layers.lyr_contact.startMs, origStart + 50);

  doc.dispatchEvent('pointermove', { clientX: 172 }); // +72px = +100ms
  assert.equal(container.queryByAttr('data-layer-id', 'lyr_contact'), clipEl, 'clipEl stays mounted on 2nd move');
  assert.equal(daw.getState().root.events.evt_surface_hit.layers.lyr_contact.startMs, origStart + 100);
  doc.dispatchEvent('pointerup', {});
  assert.equal(daw.getState().root.events.evt_surface_hit.layers.lyr_contact.startMs, origStart + 100);

  // 2. Timeline clip right-edge resize handle drag
  const clipAfterMove = container.queryByAttr('data-layer-id', 'lyr_contact');
  const resizeRight = clipAfterMove.queryByAttr('data-clip-handle', 'right');
  assert.ok(resizeRight, 'Right resize handle rendered on clip');
  const origDur = daw.getState().root.events.evt_surface_hit.layers.lyr_contact.durationMs;

  resizeRight.dispatchEvent('pointerdown', { clientX: 200, preventDefault() {} });
  doc.dispatchEvent('pointermove', { clientX: 236 }); // +50ms
  assert.equal(container.queryByAttr('data-layer-id', 'lyr_contact'), clipAfterMove, 'clip stays mounted during resize');
  assert.equal(daw.getState().root.events.evt_surface_hit.layers.lyr_contact.durationMs, origDur + 50);
  doc.dispatchEvent('pointerup', {});
  assert.equal(daw.getState().root.events.evt_surface_hit.layers.lyr_contact.durationMs, origDur + 50);

  // 3. Timeline clip fade-in handle drag
  const clipAfterResize = container.queryByAttr('data-layer-id', 'lyr_contact');
  const fadeInHandle = clipAfterResize.queryByAttr('data-clip-handle', 'fade-in');
  assert.ok(fadeInHandle, 'Fade-in handle rendered on clip');
  fadeInHandle.dispatchEvent('pointerdown', { clientX: 100, preventDefault() {} });
  doc.dispatchEvent('pointermove', { clientX: 129 }); // +40ms
  assert.equal(container.queryByAttr('data-layer-id', 'lyr_contact'), clipAfterResize);
  assert.ok(daw.getState().root.events.evt_surface_hit.layers.lyr_contact.fadeInMs >= 40);
  doc.dispatchEvent('pointerup', {});

  // 4. Contact marker line direct drag across multiple pointermove steps
  const contactLine = container.queryByAttr('data-role', 'contact-line');
  assert.ok(contactLine, 'Contact marker line rendered');
  contactLine.dispatchEvent('pointerdown', { clientX: 168 + 72, preventDefault() {} }); // 100ms
  doc.dispatchEvent('pointermove', { clientX: 168 + 144 }); // 200ms
  assert.equal(container.queryByAttr('data-role', 'contact-line'), contactLine, 'contactLine stays mounted mid-drag');
  assert.equal(daw.getState().root.events.evt_surface_hit.contactMarkerMs, 200);
  doc.dispatchEvent('pointerup', {});
  assert.equal(daw.getState().root.events.evt_surface_hit.contactMarkerMs, 200);

  // 5. DSP horizontal slider bar + EQ live curve update without DOM replacement mid-drag
  // Switch inspector back to Master bus
  const masterBusStrip = container.queryByAttr('data-bus-id', 'master');
  masterBusStrip.queryByAttr('data-action', 'inspect-bus-master')?.click();
  const midGainBar = container.queryByAttr('data-knob-bar', 'eq.midGainDb');
  assert.ok(midGainBar, 'Draggable horizontal slider bar rendered for EQ Mid Gain');
  midGainBar.dispatchEvent('pointerdown', { clientX: 50 });
  midGainBar.value = '6';
  midGainBar.dispatchEvent('input', { target: midGainBar });
  assert.equal(container.queryByAttr('data-knob-bar', 'eq.midGainDb'), midGainBar, 'slider bar stays mounted mid-drag');
  midGainBar.value = '9.5';
  midGainBar.dispatchEvent('input', { target: midGainBar });
  assert.equal(container.queryByAttr('data-knob-bar', 'eq.midGainDb'), midGainBar, 'slider bar stays mounted on 2nd input');
  doc.dispatchEvent('pointerup', {});

  // 6. Mixing Console vertical fader multi-step drag without DOM replacement mid-drag
  const sfxFader = container.queryByAttr('data-fader-bus', 'sfx');
  assert.ok(sfxFader, 'SFX vertical fader rendered');
  sfxFader.dispatchEvent('pointerdown', { clientY: 60, shiftKey: false, preventDefault() {} });
  doc.dispatchEvent('pointermove', { clientY: 40, shiftKey: false });
  assert.equal(container.queryByAttr('data-fader-bus', 'sfx'), sfxFader, 'fader stays mounted mid-drag');
  doc.dispatchEvent('pointermove', { clientY: 25, shiftKey: false });
  assert.equal(container.queryByAttr('data-fader-bus', 'sfx'), sfxFader, 'fader stays mounted on 2nd move');
  doc.dispatchEvent('pointerup', {});
  assert.ok(daw.getState().root.buses.sfx.volume > 1.0);

  daw.destroy();
});
