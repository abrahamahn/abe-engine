import assert from 'node:assert/strict';
import test from 'node:test';
import {
  DEFAULT_ACADEMIC_SEQUENCER_PRESET,
  SEQUENCE_SCHEMA,
  SEQUENCER_DOMAIN_PRESETS,
  collectSequenceTriggers,
  createDefaultSequenceDocument,
  createPresetSequenceDocument,
  evaluateSequenceAt,
  inferTemporalBeatRelations,
  mountSequencerStudio,
  msToFrameLabel,
  projectTrajectorySection,
  resolveTemporalAgentConflict,
  sampleSequenceCurve,
  snapMsToFrame,
  solveTwoBoneIkChain,
} from '../sequencer-studio.js';

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
      this.attrs = {};
      this.children = [];
      this.listeners = new Map();
    }
    set innerHTML(val) {
      if (val === '') this.children = [];
    }
    get innerHTML() {
      return '';
    }
    setAttribute(k, v) {
      this.attrs[k] = String(v);
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
    createElementNS(_ns, tag) {
      return new MiniElement(tag, doc);
    },
  };
  return new MiniElement('div', doc);
}

test('sequencer curves, frame snapping, multi-track evaluation, and cue collection', () => {
  assert.equal(sampleSequenceCurve({ type: 'linear' }, 0.4, 1), 0.4);
  assert.equal(sampleSequenceCurve({ type: 'cubicBezier', x1: 0.22, y1: 1, x2: 0.36, y2: 1 }, 0, 1), 0);
  assert.equal(sampleSequenceCurve({ type: 'cubicBezier', x1: 0.22, y1: 1, x2: 0.36, y2: 1 }, 1, 1), 1);
  assert.ok(sampleSequenceCurve({ type: 'cubicBezier', x1: 0.22, y1: 1, x2: 0.36, y2: 1 }, 0.3, 1) > 0.5);
  assert.equal(sampleSequenceCurve({ type: 'spring', stiffness: 260, damping: 18, mass: 1 }, 1, 0.8), 1);

  assert.equal(snapMsToFrame(50, 60), 50);
  assert.equal(msToFrameLabel(500, 60), '500ms · f30');

  const doc = createDefaultSequenceDocument('arena_reveal');
  assert.equal(doc.schema, SEQUENCE_SCHEMA);
  assert.equal(doc.presetId, DEFAULT_ACADEMIC_SEQUENCER_PRESET);
  assert.equal(DEFAULT_ACADEMIC_SEQUENCER_PRESET, 'character-ik-rig');

  const mid = evaluateSequenceAt(doc, 750);
  assert.ok(mid.camera);
  assert.equal(mid.skeletal.length, 1);
  assert.equal(mid.transforms.length, 1);
  assert.ok(mid.transforms[0].position[1] > 1.05);
  assert.ok(mid.transforms[0].ikPose);

  const cues = collectSequenceTriggers(doc, 1000, 1300);
  assert.equal(cues.length, 2);
  assert.deepEqual(
    cues.map((c) => c.kind).sort(),
    ['audio', 'vfx'],
  );
});

test('Articulated Character Rigging 2-Bone IK, Inverse Temporal Lifting, and Trajectory Projection', () => {
  // 1. Reachable target inside kinematic sphere
  const ikValid = solveTwoBoneIkChain([0, 1.32, 0], [0.25, 1.02, 0.38], {
    upperLen: 0.36,
    lowerLen: 0.36,
    maxReachRadius: 0.71,
    surfaceMinY: 0.84,
  });
  assert.equal(ikValid.reachable, true);
  assert.equal(ikValid.l2Perturbation, 0);
  assert.ok(ikValid.elbowBendAngleDeg > 10);

  // 2. Unreachable target & below desk surface -> projects onto kinematic manifold C_IK
  const ikClamped = solveTwoBoneIkChain([0, 1.32, 0], [1.25, 0.52, 0.95], {
    upperLen: 0.36,
    lowerLen: 0.36,
    maxReachRadius: 0.71,
    surfaceMinY: 0.84,
  });
  assert.equal(ikClamped.reachable, false);
  assert.equal(ikClamped.reachClamped, true);
  assert.equal(ikClamped.surfaceClamped, true);
  assert.ok(ikClamped.l2Perturbation > 0.5);
  assert.ok(ikClamped.wristPos[1] >= 0.84);

  // 3. Inverse Spatio-Temporal Lifting infers reaching_for, on_top_of, gaze_at, and contact_lock
  const obstacles = SEQUENCER_DOMAIN_PRESETS['character-ik-rig'].obstacles;
  const lifted = inferTemporalBeatRelations('wrist_hand_R', [0.28, 0.92, 0.44], obstacles, {
    trackKind: 'skeletal',
    gazeTargetId: 'artifact_orb',
  });
  const rels = lifted.edges.map((e) => e.rel);
  assert.ok(rels.includes('reaching_for'));
  assert.ok(rels.includes('on_top_of'));
  assert.ok(rels.includes('gaze_at'));

  // 4. Trajectory projection lifts low arc over desk obstacle and emits ghost arc points
  const lowSection = {
    id: 'sec_low_reach',
    target: 'wrist_hand_R',
    startMs: 300,
    endMs: 1200,
    fromPos: [-0.35, 0.78, 0.48],
    toPos: [0.35, 0.78, 0.48],
    arcLift: 0.01,
    ikChain: {
      shoulderPos: [0, 1.32, 0],
      upperLen: 0.38,
      lowerLen: 0.38,
      maxReachRadius: 0.75,
      surfaceMinY: 0.84,
    },
  };
  const proj = projectTrajectorySection(lowSection, obstacles);
  assert.equal(proj.violated, true);
  assert.ok(proj.projectedSection.toPos[1] >= 0.84);
  assert.ok(proj.projectedSection.arcLift > lowSection.arcLift);
  assert.equal(proj.ghostArcPoints.length, proj.projectedArcPoints.length);

  // 5. 2-Agent Temporal Conflict Resolution merges anchor shift + projects IK trajectory
  const conflict = resolveTemporalAgentConflict(
    { startMs: 300, endMs: 1200, fromPos: [0.28, 0.86, 0.44], toPos: [0.42, 0.65, 0.52] },
    lowSection,
    obstacles,
  );
  assert.equal(conflict.temporalOverlapMs, 900);
  assert.ok(conflict.resolvedSection.toPos[1] >= 0.84);
});

test('mountSequencerStudio scrubs playhead, switches domain presets, resolves 2-agent rig conflict, and emits edit ops', () => {
  const container = createMiniDom();
  const doc = createPresetSequenceDocument('character-ik-rig');
  const clocks = [];
  const cues = [];
  const edits = [];
  const cameraPoses = [];

  const handle = mountSequencerStudio(container, {
    getActiveDocument: () => ({ id: 'seq_1', path: 'content/sequences/stage.seq.json', root: doc }),
    sendPieClock: (clk) => clocks.push(clk),
    sendPieCue: (cue) => cues.push(cue),
    getViewport: () => ({
      setCameraPose: (pose) => cameraPoses.push(pose),
    }),
    edit: (id, ops, label) => edits.push({ id, ops, label }),
  });

  assert.ok(clocks.length >= 1);
  assert.ok(cameraPoses.length >= 1);

  const semanticBox = container.find((el) => el.dataset?.role === 'seq-semantic-edges');
  assert.ok(semanticBox);
  assert.match(semanticBox.textContent, /reaching_for|on_top_of/);

  const scrubber = container.find((el) => el.dataset?.role === 'seq-scrubber');
  assert.ok(scrubber);
  scrubber.value = '1250';
  scrubber.dispatch('input');
  assert.equal(cues.length, 2);

  const conflictBtn = container.find((el) => el.dataset?.role === 'seq-sim-conflict');
  assert.ok(conflictBtn);
  conflictBtn.dispatch('click');
  assert.equal(edits.length, 1);

  const muteBtn = container.find((el) => el.dataset?.role === 'mute-track-0');
  assert.ok(muteBtn);
  muteBtn.dispatch('click');
  assert.equal(edits.length, 2);
  assert.deepEqual(edits[1].ops, [{ set: ['tracks', '0', 'muted'], value: true }]);

  handle.unmount();
  assert.equal(container.children.length, 0);
});
