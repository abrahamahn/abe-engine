import assert from 'node:assert/strict';
import test from 'node:test';
import {
  bindCharacterShapes,
  createCharacterRigController,
  encodePoseCommandPacket,
  hermiteFade,
  poseModelClip,
  quatFromEuler,
  sampleBlendSpace1D,
  sampleBlendSpace2D,
} from '../../web/character-customization.mjs';
import {
  CHARACTER_SCHEMA,
  buildBoneOffsetOps,
  buildPresetApplyOps,
  createDefaultCharacterDocument,
  evaluateCharacterPoseSpec,
  filterBoneTree,
  groupMorphTargets,
  mountCharacterStudio,
} from '../character-studio.js';

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

test('character-customization single-call packet encoding, blend spaces, and rig controller', () => {
  assert.equal(hermiteFade(0, 0.2), 0);
  assert.equal(hermiteFade(0.1, 0.2), 0.5);
  assert.equal(hermiteFade(0.25, 0.2), 1);

  const bs1 = sampleBlendSpace1D(
    [
      { clip: 0, position: 0 },
      { clip: 1, position: 1 },
    ],
    0.25,
    0.5,
  );
  assert.equal(bs1.length, 2);
  assert.equal(bs1[0].weight, 1);
  assert.ok(Math.abs(bs1[1].weight - 0.25) < 1e-6);

  const bs2 = sampleBlendSpace2D(
    [
      { clip: 0, position: [0, 0] },
      { clip: 1, position: [1, 0] },
      { clip: 2, position: [0, 1] },
    ],
    [1, 0],
    0.4,
  );
  assert.equal(bs2.length, 1);
  assert.equal(bs2[0].clip, 1);

  let singleCalls = 0;
  let lastPacket = null;
  const wasmEngine = {
    snapshotModelPose() {},
    poseModelClip(instance, packet) {
      assert.equal(instance, 7);
      singleCalls++;
      lastPacket = packet;
    },
    modelClip() {
      throw new Error('should not fall back to per-joint JS calls when poseModelClip exists');
    },
    modelNode() {
      return { translation: [0, 0, 0], rotation: [0, 0, 0, 1], scale: [1, 1, 1] };
    },
    setModelNode() {},
    modelMorph() {},
    updateModel() {},
  };

  const info = {
    nodes: [
      { name: 'hips', mesh: null },
      { name: 'neck_01', mesh: null },
      { name: 'head', mesh: 0 },
    ],
    meshes: [{ targetNames: ['blink', 'smile'], targets: 2 }],
    clips: [
      { name: 'idle_calm', duration: 2.0, nodes: [0, 1, 2] },
      { name: 'gesture_greet', duration: 1.5, nodes: [0, 1, 2] },
    ],
  };

  const rig = createCharacterRigController(wasmEngine, 7, info, { defaultCrossfadeSeconds: 0.2 });
  rig.evaluate({
    clockSeconds: 0,
    stateKey: 'idle',
    clip: 'idle_calm',
    time: 0.4,
    lookAt: { targetModel: [0.2, 1.6, 2.0], joints: [['neck_01', 0.35], ['head', 0.65]] },
    bonePoses: { hips: [0, 0.1, 0] },
    morphWeights: { blink: 0.8 },
  });
  rig.evaluate({
    clockSeconds: 0.05,
    stateKey: 'greet',
    clip: 'gesture_greet',
    time: 0.05,
  });
  assert.equal(singleCalls, 2);
  assert.ok(lastPacket instanceof Float32Array);
  // Crossfade active on transition -> flags bit 4 set
  assert.ok((Math.round(lastPacket[0]) & 4) !== 0);

  // Verify bindCharacterShapes compatibility
  const morphCalls = [];
  const binder = bindCharacterShapes(
    {
      modelMorph: (...args) => morphCalls.push(args),
      updateModel: () => {},
    },
    3,
    info,
    { blink: [0, 1], smile: [0, 1] },
  );
  assert.deepEqual(binder.available, ['blink', 'smile']);
  assert.equal(binder.apply({ blink: 1.5 }), true);
  assert.deepEqual(morphCalls[0], [3, 2, 0, 1]);
  assert.equal(quatFromEuler(0, 0, 0)[3], 1);
  assert.ok(encodePoseCommandPacket({ clips: [{ clip: 0, time: 0.2, weight: 1 }] }).length >= 12);
});

test('mountCharacterStudio renders rig hierarchy, emits studioApi.edit ops, and forwards single-call poses', () => {
  const container = createMiniDom();
  const edits = [];
  const packets = [];
  const doc = createDefaultCharacterDocument('hero_actor', 'Hero Actor');

  assert.equal(doc.schema, CHARACTER_SCHEMA);
  assert.equal(filterBoneTree(doc.skeleton.joints, 'arm').length, 2);
  assert.equal(groupMorphTargets(doc.morphTargets).expression.length, 2);
  assert.deepEqual(buildBoneOffsetOps('hips', [0, 0, 0], [0, 0, 0]), [{ remove: ['boneOffsets', 'hips'] }]);
  assert.equal(buildPresetApplyOps(doc, 'warm_smile').length, 2);

  const spec = evaluateCharacterPoseSpec(doc, {
    blendSpaceId: 'stance_blend_1d',
    blendParam: 0.5,
    timeS: 0.3,
  });
  assert.equal(spec.clips.length, 2);

  const handle = mountCharacterStudio(container, {
    getActiveDocument: () => ({ id: 'doc_hero', path: 'content/characters/hero.character.json', root: doc }),
    edit(id, ops, label) {
      edits.push({ id, ops, label });
    },
    getEngine: () => ({
      activeCharacterInstance: 11,
      poseModelClip(inst, pkt) {
        packets.push({ inst, pkt });
      },
    }),
  });

  assert.ok(packets.length >= 1);
  assert.equal(packets[0].inst, 11);

  const smileSlider = container.find((el) => el.dataset?.morph === 'smile');
  assert.ok(smileSlider, 'expected smile morph slider');
  smileSlider.value = '0.85';
  smileSlider.dispatch('input');
  assert.equal(edits.length, 1);
  assert.deepEqual(edits[0].ops, [{ set: ['morphWeights', 'smile'], value: 0.85 }]);

  const ikBtn = container.find((el) => el.dataset?.role === 'ik-toggle-0');
  assert.ok(ikBtn);
  ikBtn.dispatch('click');
  assert.equal(edits.length, 2);
  assert.deepEqual(edits[1].ops, [{ set: ['ikChains', '0', 'enabled'], value: true }]);

  handle.onDeactivate();
  handle.onActivate();
  handle.destroy();
  assert.equal(container.children.length, 0);
});
