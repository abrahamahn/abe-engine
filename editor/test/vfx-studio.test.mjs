import assert from 'node:assert/strict';
import test from 'node:test';
import {
  ALL_VFX_STUDIO_DOCS,
  VFX_ACADEMIC_PRESET_DOCS,
  VFX_ACADEMIC_SCENE_PRESETS,
  VFX_EMITTER_PRESETS,
  VFX_SCHEMA,
  YOZAKURA_TABLE_VFX_DOCS,
  YOZAKURA_VFX_TABLE_GUIDES,
  createDefaultVfxDocument,
  liftVfxSpatialToSemantic,
  mountVfxStudio,
  projectVfxConstraints,
  renderEmbeddedVfxInspector,
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

test('mountVfxStudio exposes Yozakura table VFX guide, seat anchor selector, and full burst/physics inspector', () => {
  assert.deepEqual(Object.keys(YOZAKURA_TABLE_VFX_DOCS), [
    'table-win-burst',
    'all-in-chip-sparks',
    'yozakura-sakura-petals',
    'yozakura-lantern-embers',
  ]);
  assert.equal(
    YOZAKURA_VFX_TABLE_GUIDES['table-win-burst'].sequencePath,
    'content/sequences/poker-pot-award.seq.json',
  );

  const container = createMiniDom();
  const edits = [];
  const handle = mountVfxStudio(container, {
    edit: (id, ops, label) => edits.push({ id, ops, label }),
  });

  // Defaults to table-win-burst with placement & trigger guide visible
  const guideCard = container.find((el) => el.dataset?.role === 'vfx-table-guide');
  assert.ok(guideCard);

  const burstCountSlider = container.find((el) => el.dataset?.role === 'emitter-burst-count');
  assert.ok(burstCountSlider);
  assert.equal(burstCountSlider.value, '84');
  burstCountSlider.value = '140';
  burstCountSlider.dispatch('input');
  assert.equal(edits.length, 1);
  assert.deepEqual(edits[0].ops, [
    { set: ['emitters', '0', 'bursts'], value: [{ timeS: 0, count: 140 }] },
  ]);

  const docSwitcher = container.find((el) => el.dataset?.vfxDoc === 'all-in-chip-sparks');
  assert.ok(docSwitcher);
  docSwitcher.dispatch('click');

  const allInBurstSlider = container.find((el) => el.dataset?.role === 'emitter-burst-count');
  assert.equal(allInBurstSlider.value, '90');

  handle.destroy();
});

test('Tri-Modal VFX Core Engine: Inverse Semantic Lifting (L_vfx), Dual Constraint Projection, and Embedded Inspector', () => {
  assert.deepEqual(Object.keys(VFX_ACADEMIC_PRESET_DOCS), [
    'yozakura-tea-steam',
    'ik-contact-glint',
  ]);
  assert.equal(Object.keys(ALL_VFX_STUDIO_DOCS).length, 6);
  assert.deepEqual(Object.keys(VFX_ACADEMIC_SCENE_PRESETS), [
    'yozakura_pavilion',
    'character_ik_studio',
    'tabletop_salon',
  ]);

  // 1. Inverse Semantic Lifting (L_vfx : R^3 -> G_rel)
  const liftedTea = liftVfxSpatialToSemantic(
    [0.16, 0.45, -0.04],
    'yozakura_pavilion',
    'yozakura-tea-steam',
  );
  assert.equal(liftedTea.nearestSocket.id, 'tea_set');
  assert.equal(liftedTea.relation, 'rises_from');
  assert.match(liftedTea.symbolicFormula, /rises_from\(yozakura-tea-steam, tea_set\)/);

  // 2. Dual Spatial + GPU Frame-Budget Minimal-Perturbation Projection (\hat{\theta} -> \theta^*)
  const hallucinatedNode = {
    id: 'vfx_tea_steam',
    type: 'vfx_emitter',
    vfxId: 'yozakura-tea-steam',
    scenePresetId: 'yozakura_pavilion',
    rawPosition: [0.15, -0.25, -0.05], // underground / inside solid tea_table
    position: [0.15, -0.25, -0.05],
    spawnRate: 220, // excessive overdraw
    burstCount: 190,
    sizeBase: 0.095,
    maxParticles: 320,
    blendMode: 'additive',
  };

  const proj = projectVfxConstraints(hallucinatedNode, {
    scenePresetId: 'yozakura_pavilion',
  });
  assert.equal(proj.wasProjected, true);
  assert.equal(proj.spatialProjection.clamped, true);
  assert.equal(proj.spatialProjection.projectedPosition[1], 0.44);
  assert.ok(proj.spatialProjection.deltaL2 > 0.6);
  assert.equal(proj.budgetProjection.clamped, true);
  assert.ok(proj.budgetProjection.projectedGpuMs <= proj.budgetProjection.budgetMaxGpuMs);

  // 3. Embedded Inspector widget for studio.js #inspector-body
  const inspectorContainer = createMiniDom();
  const changedNodes = [];
  const inspector = renderEmbeddedVfxInspector(
    inspectorContainer,
    {
      id: 'vfx_tea_steam',
      type: 'vfx_emitter',
      vfxId: 'yozakura-tea-steam',
      scenePresetId: 'yozakura_pavilion',
      position: [0.15, 0.44, -0.05],
      spawnRate: 16,
    },
    {
      onChange: (updated) => changedNodes.push(updated),
    },
  );
  assert.ok(inspector);
  const injectBtn = inspectorContainer.find(
    (el) => el.dataset?.role === 'embedded-vfx-inject-hallucination',
  );
  assert.ok(injectBtn);
  injectBtn.dispatch('click');
  assert.ok(changedNodes.length >= 1);

  const projectBtn = inspectorContainer.find(
    (el) => el.dataset?.role === 'embedded-vfx-project-btn',
  );
  assert.ok(projectBtn);
  projectBtn.dispatch('click');
  assert.equal(inspector.getState().position[1], 0.44);
});

