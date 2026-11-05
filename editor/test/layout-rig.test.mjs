import assert from 'node:assert/strict';
import test from 'node:test';
import { createLayoutRig } from '../layout-rig.js';

function createMockDom() {
  class MockNode {
    constructor(tag) {
      this.tagName = String(tag).toUpperCase();
      this.className = '';
      this.id = '';
      this.hidden = false;
      this.style = {};
      this.dataset = {};
      this.attributes = new Map();
      this.children = [];
      this.listeners = new Map();
      this.textContent = '';
    }

    setAttribute(name, value) {
      const str = String(value);
      this.attributes.set(name, str);
      if (name === 'id') this.id = str;
      else if (name === 'class') this.className = str;
      else if (name.startsWith('data-')) {
        const camel = name.slice(5).replace(/-([a-z])/g, (_, ch) => ch.toUpperCase());
        this.dataset[camel] = str;
      }
    }

    getAttribute(name) {
      return this.attributes.get(name) ?? null;
    }

    addEventListener(type, fn) {
      const list = this.listeners.get(type) || [];
      list.push(fn);
      this.listeners.set(type, list);
    }

    removeEventListener(type, fn) {
      const list = this.listeners.get(type) || [];
      this.listeners.set(type, list.filter((item) => item !== fn));
    }

    dispatch(type, ev = {}) {
      for (const fn of this.listeners.get(type) || []) {
        fn({ stopPropagation() {}, preventDefault() {}, button: 0, ...ev });
      }
    }

    append(...items) {
      for (const item of items) {
        if (item === null || item === undefined) continue;
        if (typeof item === 'string') {
          const textNode = new MockNode('#text');
          textNode.textContent = item;
          this.children.push(textNode);
        } else {
          this.children.push(item);
        }
      }
    }

    replaceChildren(...items) {
      this.children = [];
      this.append(...items);
    }

    getBoundingClientRect() {
      return { left: 0, top: 0, width: 1280, height: 720 };
    }

    get innerText() {
      if (this.tagName === '#TEXT') return this.textContent;
      return this.children.map((c) => c.innerText).join('');
    }

    queryAll(predicate, out = []) {
      for (const child of this.children) {
        if (predicate(child)) out.push(child);
        child.queryAll(predicate, out);
      }
      return out;
    }
  }

  const overlay = new MockNode('div');
  overlay.id = 'layout-rig-overlay';
  overlay.hidden = true;
  const previewFrame = new MockNode('div');
  previewFrame.id = 'preview-frame';
  const target = new MockNode('canvas');
  target.id = 'viewport';

  const byId = new Map([
    ['layout-rig-overlay', overlay],
    ['preview-frame', previewFrame],
    ['viewport', target],
  ]);

  const el = (tag, attrs = {}, ...children) => {
    const node = new MockNode(tag);
    for (const [k, v] of Object.entries(attrs)) {
      if (k.startsWith('on')) {
        if (v) node.addEventListener(k.slice(2), v);
      } else if (k === 'class') {
        node.className = v;
      } else if (v !== undefined && v !== null && v !== false) {
        node.setAttribute(k, v === true ? '' : v);
      }
    }
    for (const c of children.flat()) {
      if (c !== null && c !== undefined) node.append(c);
    }
    if (node.id) byId.set(node.id, node);
    return node;
  };

  return {
    overlay,
    previewFrame,
    target,
    el,
    documentStub: {
      getElementById: (id) => byId.get(id) || null,
    },
  };
}

const LABELS = {
  layoutRig: 'Layout Rig',
  mirrorX: 'Mirror X',
  snapShiftHint: 'Shift: snap',
  layoutRig25dTable: '🃏 2.5D Table Layout',
  layoutRig25dTableTip: 'Open 2.5D Table Layout',
  layoutRig3dTable: '🎲 3D Table Layout',
  layoutRig3dTableTip: 'Open 3D Table Layout',
  layoutRigAnchorMode: '⚓ Engine Anchor Rig',
  layoutRigAnchorModeTip: 'Stay on Engine Anchor Rig',
  layoutRigSelectSchemaHint: 'Select a layout-rig .data.json asset to drag vec2 handles',
};

test('Layout Rig stays hidden in world workbench without schema doc, and surfaces 1-click Table Layout pills in layout-rig workbench', () => {
  const prevDoc = globalThis.document;
  const { overlay, target, el, documentStub } = createMockDom();
  globalThis.document = documentStub;

  try {
    const openedDocked = [];
    let anchorActivated = 0;
    let context = {
      inspectedDataDoc: null,
      kinds: {},
      pieMode: 'scene',
      activeWorkbench: 'world',
      tableLayoutTools: {
        layoutTuner: {
          id: 'layout-tuner',
          embedHref: 'http://127.0.0.1:5173/ganbate-editor/layout-tuner?embed=studio',
        },
        graphicsLayoutTuner: {
          id: 'graphics-layout-tuner',
          embedHref: 'http://127.0.0.1:5173/ganbate-editor/graphics-layout-tuner?embed=studio',
        },
      },
    };

    const rig = createLayoutRig({
      el,
      t: (k) => LABELS[k] || k,
      getContext: () => context,
      getTargetElement: () => target,
      onOpenDockedTool: (toolId) => openedDocked.push(toolId),
      onActivateAnchorRig: () => { anchorActivated += 1; },
    });

    // 1. World workbench with no Kind::Data schema selected -> #layout-rig-overlay stays hidden.
    rig.draw();
    assert.equal(overlay.hidden, true);
    assert.equal(overlay.children.length, 0);

    // 2. Switching to `layout-rig` workbench with no schema selected -> shows floating pill bar!
    context = { ...context, activeWorkbench: 'layout-rig' };
    rig.draw();
    assert.equal(overlay.hidden, false);

    const pills = overlay.queryAll((n) => Boolean(n.dataset.layoutTool));
    assert.deepEqual(
      pills.map((p) => [p.dataset.layoutTool, p.innerText]),
      [
        ['layout-tuner', '🃏 2.5D Table Layout'],
        ['graphics-layout-tuner', '🎲 3D Table Layout'],
        ['anchor-rig', '⚓ Engine Anchor Rig'],
      ],
    );

    pills[0].dispatch('click');
    pills[1].dispatch('click');
    pills[2].dispatch('click');
    assert.deepEqual(openedDocked, ['layout-tuner', 'graphics-layout-tuner']);
    assert.equal(anchorActivated, 1);

    // 3. Selecting a Kind::Data layout-rig document preserves `.layout-rig-handle` & `#layout-rig-mirror`.
    const hudSchema = {
      editor: 'layout-rig',
      fields: {
        compass_left: { type: 'vec2', unit: 'norm', min: 0, max: 1, default: [0.2, 0.18] },
        compass_right: { type: 'vec2', unit: 'norm', min: 0, max: 1, default: [0.8, 0.18] },
      },
    };
    context = {
      ...context,
      activeWorkbench: 'world',
      kinds: { 'hud.anchors': hudSchema },
      inspectedDataDoc: {
        id: 'doc-hud',
        root: { schema: 'hud.anchors', values: {} },
      },
    };
    rig.draw();
    assert.equal(overlay.hidden, false);
    const handles = overlay.queryAll((n) => n.className.split(/\s+/).includes('layout-rig-handle'));
    assert.deepEqual(handles.map((h) => h.dataset.anchorPath), ['compass_left', 'compass_right']);
    const mirrorBtn = overlay.queryAll((n) => n.id === 'layout-rig-mirror');
    assert.equal(mirrorBtn.length, 1);
  } finally {
    globalThis.document = prevDoc;
  }
});
