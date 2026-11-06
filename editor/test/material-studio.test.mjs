import test from 'node:test';
import assert from 'node:assert/strict';
import { performance } from 'node:perf_hooks';

import {
  MATERIAL_PRESETS,
  SURFACE_MODELS,
  TONEMAP_MODES,
  buildLookPreviewPatch,
  buildMaterialDocOps,
  compileMaterialIntent,
  computeEv100,
  ev100ExposureScale,
  evaluateBrdfPreviewPixel,
  liftMaterialToSemanticDescriptor,
  linearToSrgbHex,
  mountMaterialStudio,
  renderShaderBallRgba,
  srgbHexToLinear,
  tonemapLinearRgb,
  verifyAndProjectPbrMaterial,
} from '../material-studio.js';

// Minimal zero-dependency DOM shim for headless Node unit tests.
class MockElement {
  constructor(ownerDocument, tag) {
    this.ownerDocument = ownerDocument;
    this.tagName = tag.toUpperCase();
    this.attributes = new Map();
    this.children = [];
    this.parentNode = null;
    this.listeners = new Map();
    this.className = '';
    this.value = '';
    this.disabled = false;
    this._text = '';
  }
  get firstChild() {
    return this.children[0] || null;
  }
  get textContent() {
    if (this.children.length === 0) return this._text;
    return this.children.map((c) => c.textContent).join('');
  }
  set textContent(v) {
    this.children = [];
    this._text = String(v ?? '');
  }
  setAttribute(k, v) {
    this.attributes.set(k, String(v));
    if (k === 'id') this.id = String(v);
    if (k === 'width') this.width = Number(v);
    if (k === 'height') this.height = Number(v);
    if (k === 'value') this.value = String(v);
  }
  getAttribute(k) {
    return this.attributes.get(k) ?? null;
  }
  append(...nodes) {
    for (const n of nodes) {
      if (n == null) continue;
      const child = typeof n === 'string' ? this.ownerDocument.createTextNode(n) : n;
      child.parentNode = this;
      this.children.push(child);
    }
  }
  removeChild(child) {
    const idx = this.children.indexOf(child);
    if (idx >= 0) {
      this.children.splice(idx, 1);
      child.parentNode = null;
    }
  }
  addEventListener(type, fn) {
    if (!this.listeners.has(type)) this.listeners.set(type, []);
    this.listeners.get(type).push(fn);
  }
  dispatchEvent(type, detail = {}) {
    const list = this.listeners.get(type) || [];
    for (const fn of list) fn({ target: this, ...detail });
  }
  getContext(kind) {
    if (kind !== '2d') return null;
    return {
      createImageData: (w, h) => ({ width: w, height: h, data: new Uint8ClampedArray(w * h * 4) }),
      putImageData: (img) => { this._putImage = img; },
    };
  }
  queryAll(predicate, acc = []) {
    if (predicate(this)) acc.push(this);
    for (const c of this.children) {
      if (typeof c.queryAll === 'function') c.queryAll(predicate, acc);
    }
    return acc;
  }
}

function createMockDocument() {
  return {
    createElement(tag) {
      return new MockElement(this, tag);
    },
    createTextNode(text) {
      return { textContent: String(text), parentNode: null };
    },
  };
}

test('sRGB <-> linear hex round-trips and physical camera EV100 math', () => {
  for (const hex of ['#000000', '#7a1612', '#d8ad54', '#233a66', '#ffffff']) {
    const lin = srgbHexToLinear(hex);
    assert.equal(linearToSrgbHex(lin), hex);
  }

  // Sunny f/8 at 1/128s ISO 100 => exact EV100 = 13.0 => 1.0x scale.
  const ev13 = computeEv100({ aperture: 8.0, shutterSpeed: 1 / 128, iso: 100, compensation: 0 });
  assert.ok(Math.abs(ev13 - 13.0) < 1e-6);
  assert.ok(Math.abs(ev100ExposureScale(ev13) - 1.0) < 1e-6);
  assert.ok(Math.abs(ev100ExposureScale(12.0) - 2.0) < 1e-6);
  assert.ok(Math.abs(ev100ExposureScale(14.0) - 0.5) < 1e-6);

  // +1 EV compensation doubles exposure.
  const evComp = computeEv100({ aperture: 8.0, shutterSpeed: 1 / 128, iso: 100, compensation: 1.0 });
  assert.ok(Math.abs(ev100ExposureScale(evComp) - 2.0) < 1e-6);
});

test('all 6 surface BRDF models and 3 tonemap curves produce valid distinct shader balls', () => {
  assert.equal(SURFACE_MODELS.length, 6);
  assert.equal(TONEMAP_MODES.length, 3);
  assert.equal(MATERIAL_PRESETS.length, 6);

  const outputs = new Map();
  for (const model of SURFACE_MODELS) {
    const lit = evaluateBrdfPreviewPixel(0.35, 0.45, Math.sqrt(1 - 0.35 ** 2 - 0.45 ** 2), {
      surface: model.id,
      baseColorHex: '#c84432',
      roughness: 0.24,
      metallic: model.id === 'standard_ggx' ? 0.8 : 0.0,
      clearcoat: 0.9,
      sheen: 0.85,
      subsurface: 0.8,
    });
    assert.equal(lit.length, 3);
    assert.ok(lit.every((c) => Number.isInteger(c) && c >= 0 && c <= 255));
    assert.ok(lit[0] + lit[1] + lit[2] > 30, `${model.id} should be visibly lit`);
    outputs.set(model.id, lit.join(','));
  }
  assert.notEqual(outputs.get('stylized_ramp'), outputs.get('standard_ggx'));
  assert.notEqual(outputs.get('standard_ggx'), outputs.get('charlie_cloth'));

  for (const tm of TONEMAP_MODES) {
    const mapped = tonemapLinearRgb([3.2, 1.1, 0.4], tm.id);
    assert.ok(mapped.every((c) => c >= 0 && c <= 1));
  }

  const ball = renderShaderBallRgba(48, 48, { surface: 'clearcoat' });
  assert.equal(ball.length, 48 * 48 * 4);
});

test('mountMaterialStudio aligns with StudioWorkbenchApi and responds in <50ms', async () => {
  const doc = createMockDocument();
  const container = doc.createElement('div');

  const docs = new Map([
    [
      'mat-1',
      {
        id: 'mat-1',
        path: 'content/materials/arch_stone.material.json',
        kind: 'material',
        dirty: false,
        version: 1,
        issues: [],
        root: {
          name: 'arch_stone',
          color: [0.22, 0.22, 0.20, 1],
          roughness: 0.75,
          metallic: 0,
          shading: 'standard',
          surface: 'stylized_ramp',
        },
      },
    ],
  ]);

  const edits = [];
  const previewLooks = [];
  const registeredMaterials = [];
  const graphPatches = [];
  let savedId = null;
  let undoneId = null;

  const studioApi = {
    listDocuments({ suffix } = {}) {
      return [...docs.values()].filter((d) => !suffix || d.path.endsWith(suffix));
    },
    getActiveDocument() {
      return docs.get('mat-1');
    },
    getDocument(id) {
      return docs.get(id) || null;
    },
    async openDocument(id) {
      return docs.get(id) || null;
    },
    async createDocument({ path, kind, initialRoot }) {
      const created = { id: 'mat-2', path, kind, dirty: true, version: 1, issues: [], root: initialRoot };
      docs.set(created.id, created);
      return created;
    },
    async edit(docId, ops, label, merge) {
      edits.push({ docId, ops, label, merge });
      const snap = docs.get(docId);
      for (const op of ops) {
        if (op.set) snap.root[op.set[0]] = op.value;
        if (op.remove) delete snap.root[op.remove[0]];
      }
      snap.version += 1;
      snap.dirty = true;
      return snap;
    },
    async undo(docId) {
      undoneId = docId;
      return true;
    },
    async redo() {
      return true;
    },
    async save(docId) {
      savedId = docId;
      docs.get(docId).dirty = false;
      return true;
    },
    registerSemanticMaterial(desc) {
      registeredMaterials.push(desc);
    },
    patchWorldGraph(patch) {
      graphPatches.push(patch);
    },
    getViewport() {
      return {
        setPreviewLook(patch) {
          previewLooks.push(patch);
        },
      };
    },
    on() {
      return () => {};
    },
    t(key) {
      return key;
    },
  };

  const instance = mountMaterialStudio(container, studioApi);
  assert.equal(container.children.length, 1);
  assert.equal(instance.getState().activeDocId, 'mat-1');
  assert.equal(instance.getState().name, 'arch_stone');

  // Verify context-aware slider filtering: stylized_ramp hides Charlie cloth sheen until woven_velvet is applied.
  const sheenInput = container.queryAll((el) => el.getAttribute?.('data-field') === 'sheen')[0];
  assert.ok(sheenInput);
  assert.equal(sheenInput.parentNode.hidden, true);

  // Verify preset application + <50ms live update budget (including shader-ball rasterization).
  const t0 = performance.now();
  assert.equal(instance.applyPreset('woven_velvet'), true);
  const elapsedMs = performance.now() - t0;
  assert.ok(elapsedMs < 50, `live preset + shader-ball update took ${elapsedMs.toFixed(2)}ms (want <50ms)`);
  assert.equal(sheenInput.parentNode.hidden, false);

  assert.equal(instance.getState().surface, 'charlie_cloth');
  assert.ok(previewLooks.length >= 1);
  assert.equal(previewLooks.at(-1).shading.model, 'charlie_cloth');
  assert.equal(edits.length, 1);
  assert.equal(edits[0].docId, 'mat-1');
  assert.equal(docs.get('mat-1').root.surface, 'charlie_cloth');
  assert.equal(docs.get('mat-1').root.sheen, 0.88);

  // Exercise slider input event -> live edit + EV100 camera preview patch.
  const roughSlider = container.queryAll((el) => el.getAttribute?.('data-field') === 'roughness')[0];
  assert.ok(roughSlider);
  roughSlider.value = '0.18';
  roughSlider.dispatchEvent('input');
  assert.equal(docs.get('mat-1').root.roughness, 0.18);

  const ops = buildMaterialDocOps(instance.getState());
  assert.ok(ops.some((o) => o.set?.[0] === 'surface' && o.value === 'charlie_cloth'));
  const patch = buildLookPreviewPatch(instance.getState());
  assert.equal(patch.post.tonemap, 'agx');

  // Undo, Save, and Create Document buttons.
  const undoBtn = container.queryAll((el) => el.getAttribute?.('data-action') === 'undo')[0];
  const saveBtn = container.queryAll((el) => el.getAttribute?.('data-action') === 'save')[0];
  const newBtn = container.queryAll((el) => el.getAttribute?.('data-action') === 'create-material')[0];
  undoBtn.dispatchEvent('click');
  saveBtn.dispatchEvent('click');
  newBtn.dispatchEvent('click');
  await new Promise((r) => setImmediate(r));
  assert.equal(undoneId, 'mat-1');
  assert.equal(savedId, 'mat-1');
  assert.ok(docs.has('mat-2'));
  assert.ok(registeredMaterials.length >= 1);
  assert.ok(graphPatches.length >= 1);
  assert.equal(graphPatches.at(-1).source, 'material-studio');

  // Exercise Tri-Modal Material Intent compiler through the mounted instance.
  const compiled = instance.applyIntent('weathered brushed brass under sunset EV11');
  assert.equal(compiled.projectedState.surface, 'standard_ggx');
  assert.ok(instance.getSemanticDescriptor().bsdfClass === 'microfacet_conductor');

  instance.destroy();
  assert.equal(container.children.length, 0);
});

test('verifyAndProjectPbrMaterial enforces physical BSDF constraints and liftMaterialToSemanticDescriptor lifts R^d -> G_mat', () => {
  // 1. Conductor + subsurface conflict + multi-lobe energy overflow + coal-black albedo
  const hallucinated = {
    name: 'hallucinated_metal_sss',
    surface: 'standard_ggx',
    baseColorHex: '#000000',
    roughness: 0.005,
    metallic: 0.45,
    clearcoat: 1.0,
    clearcoatRoughness: 0.005,
    sheen: 0.95,
    subsurface: 0.9,
  };
  const res = verifyAndProjectPbrMaterial(hallucinated, { autoProject: true });
  assert.equal(res.valid, false);
  assert.ok(res.issues.some((i) => i.code === 'ggx_roughness_singularity'));
  assert.ok(res.issues.some((i) => i.code === 'non_binary_metallic_zone'));
  assert.ok(res.issues.some((i) => i.code === 'energy_conservation_exceeded'));
  assert.ok(res.issues.some((i) => i.code === 'albedo_luminance_out_of_range'));
  assert.ok(res.perturbationL2 > 0);
  assert.ok(res.energyBudget <= 1.001, `projected energyBudget ${res.energyBudget} must obey <= 1.0`);
  assert.ok(res.luminanceY >= 0.025, `projected luminanceY ${res.luminanceY} must obey physical floor`);

  // 2. Conductor + subsurface exclusion
  const condSss = verifyAndProjectPbrMaterial(
    { surface: 'standard_ggx', baseColorHex: '#d8ad54', metallic: 0.95, subsurface: 0.8 },
    { autoProject: true },
  );
  assert.ok(condSss.issues.some((i) => i.code === 'conductor_subsurface_conflict'));
  assert.equal(condSss.projectedState.subsurface, 0);

  // 3. Inverse Semantic Material Lifting L_mat
  const lifted = liftMaterialToSemanticDescriptor(condSss.projectedState, 'lantern_01');
  assert.equal(lifted.schema, 'ganbate.semantic.material.v1');
  assert.equal(lifted.targetEntity, 'lantern_01');
  assert.equal(lifted.bsdfClass, 'microfacet_conductor');
  assert.equal(lifted.chromaticity, 'warm_gold_amber');
  assert.equal(lifted.edges.length, 3);

  // 4. Natural-language Material Intent compilation
  const jade = compileMaterialIntent('translucent jade subsurface porcelain under night EV9');
  assert.equal(jade.projectedState.surface, 'subsurface_skin');
  assert.equal(jade.projectedState.baseColorHex, '#2d6e5e');
});

test('3D scene entity binding filters out point lights and syncs BSDF state with scene entities', async () => {
  const doc = createMockDocument();
  const container = doc.createElement('div');
  const sceneEdits = [];
  const selectionHandlers = [];

  const sceneDoc = {
    id: 'scene-1',
    path: 'content/scenes/pavilion.scene.json',
    kind: 'scene',
    root: {
      entities: {
        'light-01': { name: 'point-light-01', components: { light: { intensity: 5 } } },
        'ent-lantern': { name: 'courtyard-lantern', components: { mesh: { node: 'lantern' } } },
        'ent-brass': { name: 'brass-hardware', components: { mesh: { node: 'hardware' } } },
      },
    },
  };

  const studioApi = {
    listDocuments() {
      return [];
    },
    getActiveDocument() {
      return sceneDoc;
    },
    getState() {
      return { activeDocument: sceneDoc };
    },
    async edit(docId, ops, label) {
      sceneEdits.push({ docId, ops, label });
      return sceneDoc;
    },
    on(event, fn) {
      if (event === 'selection') selectionHandlers.push(fn);
      return () => {};
    },
  };

  const instance = mountMaterialStudio(container, studioApi);

  // Point light (`light-01`) is filtered out in favor of 3D mesh surface entities (`ent-lantern`, `ent-brass`).
  const entityButtons = container.queryAll((el) => el.getAttribute?.('data-scene-entity') !== null);
  assert.equal(entityButtons.length, 2);
  assert.equal(entityButtons[0].getAttribute('data-scene-entity'), 'ent-lantern');
  assert.equal(entityButtons[1].getAttribute('data-scene-entity'), 'ent-brass');

  // Simulate clicking `ent-brass` in the 3D scene viewport via `selection` listener.
  assert.equal(selectionHandlers.length, 1);
  selectionHandlers[0]({ entityIds: ['ent-brass'] });
  assert.equal(instance.getState().focusedEntityId, 'ent-brass');
  assert.equal(instance.getState().surface, 'standard_ggx');
  assert.equal(instance.getSemanticDescriptor().targetEntity, 'ent-brass');
  assert.equal(instance.getSemanticDescriptor().bsdfClass, 'microfacet_conductor');

  await new Promise((r) => setImmediate(r));
  assert.ok(sceneEdits.length >= 1);
  assert.equal(sceneEdits.at(-1).docId, 'scene-1');
  assert.deepEqual(sceneEdits.at(-1).ops[0].set, ['entities', 'ent-brass', 'components', 'material']);

  instance.destroy();
});
