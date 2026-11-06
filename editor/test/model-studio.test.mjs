import assert from 'node:assert/strict';
import test from 'node:test';
import { parseGlbPreview } from '../asset-previews.js';
import {
  ARCHITECTURAL_PRESETS,
  PRIMITIVE_KINDS,
  WORKBENCH_ID,
  buildModelPrefabOps,
  buildPrimitiveGlb,
  buildSemanticNodeDescriptor,
  computeImportFixup,
  decimatePreviewMesh,
  eulerDegToQuat,
  fitPhysicsCollider,
  mountModelStudio,
  normalizeModelMeta,
  projectSemanticNodePlacement,
  quatToEulerDeg,
  serializeModelMeta,
} from '../model-studio.js';

function createMockDom() {
  class MockNode {
    constructor(tag, ownerDocument) {
      this.tagName = String(tag || 'div').toUpperCase();
      this.ownerDocument = ownerDocument;
      this.className = '';
      this.style = {};
      this.attributes = new Map();
      this.children = [];
      this.listeners = new Map();
      this.textContent = '';
      this.clientWidth = 640;
      this.clientHeight = 420;
      this.width = 640;
      this.height = 420;
      this.parentNode = null;
    }
    setAttribute(k, v) { this.attributes.set(k, String(v)); }
    getAttribute(k) { return this.attributes.get(k) ?? null; }
    appendChild(child) {
      child.parentNode = this;
      this.children.push(child);
      return child;
    }
    append(...kids) {
      for (const k of kids) {
        if (k == null) continue;
        this.appendChild(typeof k === 'string' ? this.ownerDocument.createTextNode(k) : k);
      }
    }
    replaceChildren(...kids) {
      this.children = [];
      this.append(...kids);
    }
    remove() {
      if (!this.parentNode) return;
      this.parentNode.children = this.parentNode.children.filter((c) => c !== this);
      this.parentNode = null;
    }
    addEventListener(type, fn) {
      if (!this.listeners.has(type)) this.listeners.set(type, []);
      this.listeners.get(type).push(fn);
    }
    removeEventListener(type, fn) {
      const arr = this.listeners.get(type);
      if (arr) this.listeners.set(type, arr.filter((f) => f !== fn));
    }
    querySelector(sel) {
      if (sel === 'link[data-ganbate-model-studio]') {
        return this.children.find((c) => c.attributes?.get('data-ganbate-model-studio')) || null;
      }
      return null;
    }
    getContext() {
      return {
        setTransform() {},
        createLinearGradient: () => ({ addColorStop() {} }),
        fillRect() {},
        save() {},
        restore() {},
        beginPath() {},
        moveTo() {},
        lineTo() {},
        closePath() {},
        fill() {},
        stroke() {},
        arc() {},
        fillText() {},
        setLineDash() {},
      };
    }
  }

  const doc = {
    head: null,
    createElement(tag) { return new MockNode(tag, doc); },
    createTextNode(text) {
      const n = new MockNode('#text', doc);
      n.textContent = text;
      return n;
    },
    querySelector(sel) { return doc.head.querySelector(sel); },
  };
  doc.head = new MockNode('head', doc);
  const container = new MockNode('div', doc);
  return { doc, container };
}

test('computeImportFixup aligns bottom-center, centroid, cm/mm scale, and target height non-destructively', () => {
  const bounds = [[2, 4, -6], [6, 12, -2]]; // raw size: [4, 8, 4]

  const bottom = computeImportFixup(bounds, { pivot: 'bottom_center', scale: 1.0 });
  assert.equal(bottom.effectiveScale, 1);
  assert.deepEqual(bottom.transformedBounds, [[-2, 0, -2], [2, 8, 2]]);
  assert.deepEqual(bottom.transformedSize, [4, 8, 4]);

  const cmToM = computeImportFixup([[0, 0, 0], [100, 200, 50]], { pivot: 'bottom_center', scale: 0.01 });
  assert.deepEqual(cmToM.transformedSize, [1, 2, 0.5]);
  assert.equal(cmToM.transformedBounds[0][1], 0);

  const normalizedHeight = computeImportFixup(bounds, {
    pivot: 'centroid',
    target_height: 2.0,
    offset: [0, 0.25, 0],
  });
  assert.equal(normalizedHeight.effectiveScale, 0.25);
  assert.deepEqual(normalizedHeight.transformedSize, [1, 2, 1]);
  assert.deepEqual(normalizedHeight.transformedBounds, [[-0.5, -0.75, -0.5], [0.5, 1.25, 0.5]]);
});

test('buildPrimitiveGlb emits valid glTF 2.0 binaries for all graybox shapes and architectural presets', () => {
  for (const kind of PRIMITIVE_KINDS) {
    const glb = buildPrimitiveGlb(kind, { width: 3, height: 2.5, depth: 2, radius: 1, segments: 18 });
    assert.ok(glb instanceof Uint8Array && glb.byteLength > 80, `${kind} glb bytes`);
    const parsed = parseGlbPreview(glb);
    assert.ok(parsed, `${kind} parses cleanly through parseGlbPreview`);
    assert.ok(parsed.triangles >= 2, `${kind} has triangles`);
    assert.equal(parsed.hasNormals, true, `${kind} has normals`);
    assert.equal(parsed.hasUvs, true, `${kind} has UVs`);
    assert.ok(parsed.estimatedVramBytes > 0, `${kind} estimates VRAM`);
    assert.ok(Math.abs(parsed.bounds[0][1]) < 1e-4, `${kind} sits on Y=0 ground plane`);
    assert.equal(parsed.fullMesh.tangents.length, parsed.vertices * 4, `${kind} synthesizes tangent frames`);
  }

  for (const preset of ARCHITECTURAL_PRESETS) {
    const glb = buildPrimitiveGlb(preset.kind, preset);
    const parsed = parseGlbPreview(glb);
    assert.ok(parsed && parsed.triangles >= 12, `${preset.id} builds multi-part architectural geometry`);
    assert.ok(Math.abs(parsed.bounds[0][1]) < 1e-4, `${preset.id} rests on Y=0 ground plane`);
    const fixup = computeImportFixup(parsed.bounds, { pivot: 'bottom_center', scale: 1.0 });
    const desc = buildSemanticNodeDescriptor({
      path: `content/models/yozakura/${preset.name}.glb`,
      parsed,
      fixup,
      meta: {
        import: { pivot: 'bottom_center', scale: 1.0 },
        collider: fitPhysicsCollider(parsed.fullMesh, preset.colliderShape, fixup),
        sockets: preset.sockets,
      },
    });
    assert.equal(desc.schema, 'ganbate.semantic.node.v2');
    assert.equal(desc.bounds.groundRestError, 0);
    assert.ok(Math.abs(desc.bounds.surfaceY - preset.height) < 1e-3);

    const proj = projectSemanticNodePlacement(desc, [0.05, 0.29, -0.04], {
      supportSurfaceY: desc.affordances.domain === 'table_surface' ? 0.42 : 0.0,
      supportHalfExtents: [0.7, 0.21, 0.45],
      obstacles: [{ id: 'obs', position: [0, 0, 0], radius: 0.15 }],
    });
    assert.equal(proj.satisfied, true);
    assert.equal(proj.verticalDriftAfter, 0);
    assert.equal(proj.penetrationAfter, 0);
  }
});

test('decimatePreviewMesh simplifies LOD0..LOD3 monotonically', () => {
  const highPoly = parseGlbPreview(buildPrimitiveGlb('cylinder', { radius: 1, height: 2, segments: 64 }));
  const lod0 = decimatePreviewMesh(highPoly.fullMesh, 1.0);
  const lod1 = decimatePreviewMesh(highPoly.fullMesh, 0.5);
  const lod2 = decimatePreviewMesh(highPoly.fullMesh, 0.25);
  const lod3 = decimatePreviewMesh(highPoly.fullMesh, 0.125);

  assert.equal(lod0.triangles, highPoly.triangles);
  assert.ok(lod1.triangles < lod0.triangles, `LOD1 (${lod1.triangles}) < LOD0 (${lod0.triangles})`);
  assert.ok(lod2.triangles <= lod1.triangles, `LOD2 (${lod2.triangles}) <= LOD1 (${lod1.triangles})`);
  assert.ok(lod3.triangles <= lod2.triangles && lod3.triangles > 0, `LOD3 (${lod3.triangles}) <= LOD2`);
});

test('fitPhysicsCollider fits box, sphere, capsule, and convex_hull to post-fixup geometry', () => {
  const parsed = parseGlbPreview(buildPrimitiveGlb('arch', { width: 4, height: 3, depth: 1 }));
  const fixup = computeImportFixup(parsed.bounds, { pivot: 'bottom_center', scale: 1.0 });

  const box = fitPhysicsCollider(parsed.fullMesh, 'box', fixup);
  assert.equal(box.shape, 'box');
  assert.deepEqual(box.center, [0, 1.5, 0]);
  assert.deepEqual(box.half_extents, [2, 1.5, 0.5]);

  const sphere = fitPhysicsCollider(parsed.fullMesh, 'sphere', fixup);
  assert.equal(sphere.shape, 'sphere');
  assert.ok(sphere.radius >= 2.0);

  const capsule = fitPhysicsCollider(parsed.fullMesh, 'capsule', fixup);
  assert.equal(capsule.shape, 'capsule');
  assert.equal(capsule.radius, 2);
  assert.equal(capsule.height, 4);

  const hull = fitPhysicsCollider(parsed.fullMesh, 'convex_hull', fixup);
  assert.equal(hull.shape, 'convex_hull');
  assert.ok(hull.hull_points.length >= 8 && hull.hull_points.length <= 26);
});

test('socket quaternion helpers, meta serialization, and prefab generation produce schema-valid ops', () => {
  const q = eulerDegToQuat([0, 90, 0]);
  const deg = quatToEulerDeg(q);
  assert.ok(Math.abs(deg[1] - 90) < 0.1, `expected RY=90, got ${deg}`);

  const meta = normalizeModelMeta({
    import: { pivot: 'bottom-center', scale: 1, targetHeight: 1.8, activeLod: 1 },
    lods: [1, 0.55, 0.25, 0.12],
    collider: { shape: 'capsule', center: [0, 0.9, 0], radius: 0.35, height: 1.8 },
    sockets: {
      hand_r: { translation: [0.32, 1.1, 0.1], rotation: q },
      ground: { translation: [0, 0, 0], rotation: [0, 0, 0, 1] },
    },
  });
  const wire = serializeModelMeta(meta);
  assert.equal(wire.import.pivot, 'bottom_center');
  assert.equal(wire.import.target_height, 1.8);
  assert.equal(wire.import.active_lod, 1);

  const ops = buildModelPrefabOps(
    { id: '0123456789abcdef0123456789abcdef', path: 'content/models/statue.glb' },
    wire,
    ['0000000000000001', '0000000000000002', '0000000000000003'],
  );
  assert.equal(ops.length, 4); // name + root + 2 socket child entities
  const rootVal = ops[1].value;
  assert.equal(rootVal.components.mesh.model, '0123456789abcdef0123456789abcdef');
  assert.deepEqual(rootVal.components.collider, {
    shape: 'capsule',
    center: [0, 0.9, 0],
    radius: 0.35,
    half_height: 0.55,
  });
  const handEnt = ops.find((o) => o.value?.name === 'hand_r').value;
  assert.equal(handEnt.parent, '0000000000000001');
  assert.deepEqual(handEnt.components.transform.translation, [0.32, 1.1, 0.1]);
  assert.equal(handEnt.components.transform.rotation.length, 4);
});

test('mountModelStudio implements StudioWorkbenchInstance lifecycle with StudioWorkbenchApi', async () => {
  const { container } = createMockDom();
  const requests = [];
  const listeners = new Map();

  const registeredNodes = [];
  const appliedGbs = [];
  const instance = mountModelStudio(container, {
    listAssets: () => [],
    getProject: () => ({ id: 'proj', content: ['content'] }),
    getSelection: () => ({ assetId: null, entityIds: [] }),
    registerSemanticNode(desc) {
      registeredNodes.push(desc);
    },
    async applyGbs(script) {
      appliedGbs.push(script);
    },
    on(event, fn) {
      listeners.set(event, fn);
      return () => listeners.delete(event);
    },
    async hostRequest(msg) {
      requests.push(msg);
      if (msg.op === 'intake-list') {
        return {
          roots: [{ index: 0, path: '../ganbate-art', label: 'ganbate-art', available: true, count: 1 }],
          items: [{ intakeIndex: 0, rootLabel: 'ganbate-art', intakePath: 'props/column.glb', name: 'column.glb', kind: 'model', bytes: 512 }],
        };
      }
      return {};
    },
  });

  assert.equal(container.children[0].getAttribute('data-workbench'), WORKBENCH_ID);
  assert.equal(typeof instance.destroy, 'function');
  assert.equal(typeof instance.unmount, 'function');
  assert.equal(typeof instance.refresh, 'function');
  assert.equal(typeof instance.setDocument, 'function');
  assert.equal(typeof instance.onActivate, 'function');
  assert.equal(typeof instance.onDeactivate, 'function');

  const archGlb = buildPrimitiveGlb('arch', { width: 3, height: 3, depth: 0.8 });
  assert.equal(instance.previewBytes(archGlb, 'arch_blockout.glb'), true);
  assert.ok(instance.getState().parsed.triangles > 10);

  await instance.uploadFiles([{
    name: 'lantern_prop.glb',
    arrayBuffer: async () => archGlb.buffer.slice(archGlb.byteOffset, archGlb.byteOffset + archGlb.byteLength),
  }]);
  assert.ok(requests.some((r) => r.type === 'asset-import' && r.path === 'content/models/lantern_prop.glb'));

  const spawnedDesc = await instance.spawnInSemanticGraph();
  assert.equal(spawnedDesc.schema, 'ganbate.semantic.node.v2');
  assert.ok(registeredNodes.some((n) => n.id === 'lantern_prop'));
  assert.ok(appliedGbs.some((s) => s.includes('spawn lantern_prop')));

  const seededCount = await instance.seedArchitecturalCatalog();
  assert.equal(seededCount, ARCHITECTURAL_PRESETS.length);
  assert.ok(registeredNodes.some((n) => n.id === 'yozakura_salon_table'));

  instance.onDeactivate();
  assert.equal(instance.getState().active, false);
  instance.onActivate();
  assert.equal(instance.getState().active, true);
  instance.refresh();

  assert.ok(requests.some((r) => r.op === 'intake-list'));
  instance.destroy();
  assert.equal(listeners.size, 0);
  assert.equal(container.children.length, 0);
});
