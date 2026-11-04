import assert from 'node:assert/strict';
import test from 'node:test';
import {
  SEQUENCE_SCHEMA,
  collectSequenceTriggers,
  createDefaultSequenceDocument,
  evaluateSequenceAt,
  mountSequencerStudio,
  msToFrameLabel,
  sampleSequenceCurve,
  snapMsToFrame,
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

  const mid = evaluateSequenceAt(doc, 750);
  assert.ok(mid.camera);
  assert.equal(mid.skeletal.length, 1);
  assert.equal(mid.transforms.length, 1);
  // Parabolic arc lift raises Y above base 0.9 at midpoint (750ms is halfway between 300ms and 1200ms)
  assert.ok(mid.transforms[0].position[1] > 1.05);

  const cues = collectSequenceTriggers(doc, 1000, 1300);
  assert.equal(cues.length, 2);
  assert.deepEqual(
    cues.map((c) => c.kind).sort(),
    ['audio', 'vfx'],
  );
});

test('mountSequencerStudio scrubs playhead, broadcasts PIE clock & cues, and emits edit ops', () => {
  const container = createMiniDom();
  const doc = createDefaultSequenceDocument('stage_seq');
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

  const scrubber = container.find((el) => el.dataset?.role === 'seq-scrubber');
  assert.ok(scrubber);
  scrubber.value = '1250';
  scrubber.dispatch('input');
  assert.equal(cues.length, 2);

  const muteBtn = container.find((el) => el.dataset?.role === 'mute-track-0');
  assert.ok(muteBtn);
  muteBtn.dispatch('click');
  assert.equal(edits.length, 1);
  assert.deepEqual(edits[0].ops, [{ set: ['tracks', '0', 'muted'], value: true }]);

  handle.unmount();
  assert.equal(container.children.length, 0);
});
