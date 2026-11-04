import assert from 'node:assert/strict';
import test from 'node:test';
import {
  VFX_EMITTER_PRESETS,
  VFX_SCHEMA,
  createDefaultVfxDocument,
  mountVfxStudio,
  simulateVfxAt,
} from '../vfx-studio.js';

function createMiniDom() {
  class MiniElement {
    constructor(tag, ownerDocument) {
      this.tagName = String(tag).toUpperCase();
      this.ownerDocument = ownerDocument;
      this.className = '';
      this.textContent = '';
      this.value = '';
      this.type = '';
      this.dataset = {};
      this.style = {};
      this.children = [];
      this.listeners = new Map();
    }
    set innerHTML(val) {
      if (val === '') this.children = [];
    }
    get innerHTML() {
      return '';
    }
    getContext() {
      return {
        clearRect() {},
        beginPath() {},
        arc() {},
        fill() {},
      };
    }
    appendChild(child) {
      this.children.push(child);
      return child;
    }
    addEventListener(type, fn) {
      const list = this.listeners.get(type) ?? [];
      list.push(fn);
      this.listeners.set(type, list);
    }
    dispatch(type, extra = {}) {
      for (const fn of this.listeners.get(type) ?? []) {
        fn({ target: this, ...extra });
      }
    }
    find(predicate) {
      if (predicate(this)) return this;
      for (const ch of this.children) {
        const hit = ch.find?.(predicate);
        if (hit) return hit;
      }
      return null;
    }
  }
  const doc = {
    createElement(tag) {
      return new MiniElement(tag, doc);
    },
  };
  return new MiniElement('div', doc);
}

test('vfx presets and deterministic scrubbing reproduce identical particle states and budget stats', () => {
  assert.deepEqual(Object.keys(VFX_EMITTER_PRESETS), [
    'blossom_drift',
    'ember_updraft',
    'radial_burst',
    'mist_plume',
  ]);
  const doc = createDefaultVfxDocument('ambient_canopy', 'Ambient Canopy');
  assert.equal(doc.schema, VFX_SCHEMA);

  const a = simulateVfxAt(doc, 1.8);
  const b = simulateVfxAt(doc, 1.8);
  assert.deepEqual(a, b);
  assert.ok(a.stats.activeParticles > 0);
  assert.equal(a.stats.withinBudget, true);
  assert.ok(a.stats.alphaParticles > 0);
  assert.ok(a.stats.additiveParticles > 0);
});

test('mountVfxStudio renders budget HUD, adds emitter presets, and edits emitter curl turbulence', () => {
  const container = createMiniDom();
  const doc = createDefaultVfxDocument('courtyard_fx', 'Courtyard FX');
  const edits = [];

  const handle = mountVfxStudio(container, {
    getActiveDocument: () => ({ id: 'vfx_1', path: 'content/vfx/courtyard.vfx.json', root: doc }),
    edit: (id, ops, label) => edits.push({ id, ops, label }),
  });

  const hud = container.find((el) => el.dataset?.role === 'vfx-budget-hud');
  assert.ok(hud);
  assert.match(hud.textContent, /pts · OD/);

  const addRadial = container.find((el) => el.dataset?.preset === 'radial_burst');
  assert.ok(addRadial);
  addRadial.dispatch('click');
  assert.equal(edits.length, 1);
  assert.equal(edits[0].ops[0].set[0], 'emitters');

  const curlSlider = container.find((el) => el.dataset?.role === 'emitter-curl-amp');
  assert.ok(curlSlider);
  curlSlider.value = '1.25';
  curlSlider.dispatch('input');
  assert.equal(edits.length, 2);
  assert.deepEqual(edits[1].ops, [{ set: ['emitters', '0', 'curlNoise', 'amplitude'], value: 1.25 }]);

  handle.destroy();
  assert.equal(container.children.length, 0);
});
