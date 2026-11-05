import assert from 'node:assert/strict';
import test from 'node:test';
import { setLang, t } from '../i18n.js';
import {
  BUILTIN_GAMEPLAY_PRESETS,
  createPieViewport,
  resolveGameWorkspace,
} from '../pie-viewport.js';
import {
  createGameplaySwitcher,
  resolveGameplayTools,
  resolveProjectTools,
} from '../project-tools.js';

function createMockStudioDom() {
  class MockClassList {
    constructor(owner) {
      this.owner = owner;
    }

    _tokens() {
      return new Set((this.owner.className || '').split(/\s+/).filter(Boolean));
    }

    _sync(set) {
      this.owner.className = [...set].join(' ');
    }

    add(...names) {
      const set = this._tokens();
      for (const n of names) if (n) set.add(n);
      this._sync(set);
    }

    remove(...names) {
      const set = this._tokens();
      for (const n of names) set.delete(n);
      this._sync(set);
    }

    toggle(name, force) {
      const set = this._tokens();
      const shouldAdd = force !== undefined ? Boolean(force) : !set.has(name);
      if (shouldAdd) set.add(name);
      else set.delete(name);
      this._sync(set);
      return shouldAdd;
    }

    contains(name) {
      return this._tokens().has(name);
    }
  }

  class MockNode {
    constructor(tag) {
      this.tagName = String(tag).toUpperCase();
      this.className = '';
      this.classList = new MockClassList(this);
      this.id = '';
      this.hidden = false;
      this.value = '';
      this.src = '';
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
      else if (name === 'src') this.src = str;
      else if (name === 'value') this.value = str;
      else if (name === 'hidden') this.hidden = true;
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

    dispatch(type, ev = {}) {
      for (const fn of this.listeners.get(type) || []) {
        fn({ stopPropagation() {}, preventDefault() {}, target: this, ...ev });
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
      return { left: 0, top: 0, width: 960, height: 720 };
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

  const byId = new Map();
  const register = (id, tag = 'div') => {
    const node = new MockNode(tag);
    node.id = id;
    byId.set(id, node);
    return node;
  };

  const body = new MockNode('body');
  const workbenchDeck = register('workbench-deck', 'section');
  workbenchDeck.hidden = true;
  const switcherHost = register('gameplay-workbench-switcher', 'div');
  switcherHost.hidden = true;
  workbenchDeck.append(switcherHost);

  const pieToolbar = register('pie-toolbar', 'span');
  pieToolbar.hidden = true;
  const viewModeScene = register('view-mode-scene', 'button');
  const viewModePie = register('view-mode-pie', 'button');
  const viewModeMatrix = register('view-mode-matrix', 'button');
  const pieSplitToggle = register('pie-split-toggle', 'button');
  pieSplitToggle.setAttribute('data-role', 'pie-split-toggle');
  const piePreset = register('pie-preset', 'select');
  const pieDevice = register('pie-device', 'select');
  const pieInspectUi = register('pie-inspect-ui', 'button');
  pieToolbar.append(
    viewModeScene,
    viewModePie,
    viewModeMatrix,
    pieSplitToggle,
    piePreset,
    pieDevice,
    pieInspectUi,
  );

  const previewFrame = register('preview-frame', 'div');
  const viewport = register('viewport', 'section');
  const pieStage = register('pie-stage', 'section');
  pieStage.hidden = true;
  const pieMatrix = register('pie-matrix', 'section');
  pieMatrix.hidden = true;
  previewFrame.append(viewport, pieStage, pieMatrix);

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
    body,
    workbenchDeck,
    switcherHost,
    pieToolbar,
    pieSplitToggle,
    piePreset,
    pieDevice,
    viewport,
    pieStage,
    pieMatrix,
    previewFrame,
    el,
    documentStub: {
      body,
      documentElement: new MockNode('html'),
      getElementById: (id) => byId.get(id) || null,
      querySelectorAll: () => [],
    },
  };
}

test('Sequencer and Audio gameplay switcher pills render EN + KO labels and dispatch 1-click switches', () => {
  const prevDoc = globalThis.document;
  const dom = createMockStudioDom();
  globalThis.document = dom.documentStub;

  try {
    setLang('en');
    const tools = resolveProjectTools(
      {
        version: 1,
        origin: 'http://localhost:5173',
        tools: [
          { label: 'Table layouts', path: '/ganbate-editor/layout-tuner' },
          { label: 'Sound & Foley', path: '/ganbate-editor/audio-mixer' },
        ],
      },
      'http://localhost:7480',
    );
    const gameplayTools = resolveGameplayTools(tools);
    const selections = [];
    let activeWorkbench = 'world';

    const switcher = createGameplaySwitcher({
      el: dom.el,
      t,
      getContext: () => ({ activeWorkbench, gameplayTools }),
      onSelectTarget: (targetKey) => selections.push(targetKey),
    });

    // Hidden in `world` workbench
    switcher.draw();
    assert.equal(dom.switcherHost.hidden, true);

    // Native `sequencer` workbench renders `[data-role="gameplay-sequencer-switcher"]`
    activeWorkbench = 'sequencer';
    switcher.draw();
    assert.equal(dom.switcherHost.hidden, false);
    const seqBars = dom.switcherHost.queryAll((n) => n.dataset.role === 'gameplay-sequencer-switcher');
    assert.equal(seqBars.length, 1);

    const seqPills = dom.switcherHost.queryAll((n) => Boolean(n.dataset.gameplayTool));
    assert.deepEqual(
      seqPills.map((p) => [p.dataset.gameplayTool, p.innerText, p.getAttribute('aria-pressed')]),
      [
        ['action-sequencer', '🎬 Hand Choreography', 'false'],
        ['motion-tuner', '🃏 Card & Chip Motion', 'false'],
        ['god-mode', '🧪 Crowded-Hand Lab', 'false'],
        ['sequencer', '🎞️ Engine Sequencer', 'true'],
      ],
    );

    seqPills[0].dispatch('click');
    seqPills[1].dispatch('click');
    seqPills[2].dispatch('click');
    assert.deepEqual(selections, ['action-sequencer', 'motion-tuner', 'god-mode']);

    // Docked `motion-tuner` highlights `🃏 Card & Chip Motion` and localizes cleanly in Korean
    activeWorkbench = 'project:motion-tuner';
    setLang('ko');
    switcher.draw();
    const koSeqPills = dom.switcherHost.queryAll((n) => Boolean(n.dataset.gameplayTool));
    assert.deepEqual(
      koSeqPills.map((p) => [p.dataset.gameplayTool, p.innerText, p.getAttribute('aria-pressed')]),
      [
        ['action-sequencer', '🎬 핸드 시퀀스 연출', 'false'],
        ['motion-tuner', '🃏 카드 · 칩 모션', 'true'],
        ['god-mode', '🧪 다인 핸드 테스트랩', 'false'],
        ['sequencer', '🎞️ 엔진 시퀀서', 'false'],
      ],
    );

    // Docked `sound-foley` (`/ganbate-editor/audio-mixer`) and native `audio` render `[data-role="gameplay-audio-switcher"]`
    setLang('en');
    activeWorkbench = 'project:sound-foley';
    switcher.draw();
    const audioBars = dom.switcherHost.queryAll((n) => n.dataset.role === 'gameplay-audio-switcher');
    assert.equal(audioBars.length, 1);
    const audioPills = dom.switcherHost.queryAll((n) => Boolean(n.dataset.gameplayTool));
    assert.deepEqual(
      audioPills.map((p) => [p.dataset.gameplayTool, p.innerText, p.getAttribute('aria-pressed')]),
      [
        ['audio-mixer', '🎚️ Game Foley Mixer', 'true'],
        ['audio', '🎹 Engine Audio DAW', 'false'],
      ],
    );
    audioPills[1].dispatch('click');
    assert.equal(selections.at(-1), 'audio');
  } finally {
    setLang('en');
    globalThis.document = prevDoc;
  }
});

test('resolveGameWorkspace surfaces built-in Holdem, Blackjack, and Baccarat presets and Split-Play docks #pie-stage 50/50', async () => {
  const prevDoc = globalThis.document;
  const prevLoc = globalThis.location;
  const dom = createMockStudioDom();
  globalThis.document = dom.documentStub;
  globalThis.location = new URL('http://127.0.0.1:7480/');

  try {
    // 1. Manifest with custom preset preserves index 0 while appending Hold'em, Blackjack, and Baccarat,
    //    and normalizes `127.0.0.1:5173` -> `localhost:5173` when Studio runs on `127.0.0.1:7480`.
    const ws = resolveGameWorkspace(
      {
        version: 1,
        origin: 'http://127.0.0.1:5173',
        entry: '/play?studioPie=1',
        presets: [
          { id: 'poker-play-living', label: "Ganbate Hold'em · Live 3D", path: '/play?living=1&studioPie=1' },
        ],
      },
      'http://127.0.0.1:7480',
    );
    assert.equal(ws.origin, 'http://localhost:5173');
    assert.equal(ws.presets[0].id, 'poker-play-living');
    const presetLabels = ws.presets.map((p) => p.label);
    assert.ok(presetLabels.includes("Hold'em (/play)"));
    assert.ok(presetLabels.includes('Blackjack (/play/blackjack)'));
    assert.ok(presetLabels.includes('Baccarat (/play/baccarat)'));
    assert.equal(BUILTIN_GAMEPLAY_PRESETS.length, 4);

    // 2. Split-Play toggle (`[data-role="pie-split-toggle"]`) docks `#pie-stage` alongside World or Gameplay Editor!
    const pie = createPieViewport({
      el: dom.el,
      t: (k) => k,
      getContext: () => ({
        assets: [],
        datas: new Map(),
        looks: new Map(),
        kinds: {},
        doc: null,
        time: 17.5,
        weather: 'clear',
        previewLook: () => ({}),
        allowSameOrigin: false,
      }),
    });
    pie.setupEvents();
    pie.setFallbackOrigin('http://127.0.0.1:5173');
    assert.equal(dom.pieToolbar.hidden, false);
    assert.equal(pie.isSplitPlay(), false);

    // Click `[data-role="pie-split-toggle"]` while in World (`!workbench-active`) -> both `#viewport` and `#pie-stage` visible!
    dom.pieSplitToggle.dispatch('click');
    assert.equal(pie.isSplitPlay(), true);
    assert.equal(dom.body.classList.contains('pie-split-active'), true);
    assert.equal(dom.pieSplitToggle.getAttribute('aria-pressed'), 'true');
    assert.equal(dom.viewport.hidden, false);
    assert.equal(dom.pieStage.hidden, false);
    assert.equal(dom.piePreset.hidden, false);
    assert.equal(dom.pieDevice.hidden, false);

    const iframes = dom.pieStage.queryAll((n) => n.tagName === 'IFRAME');
    assert.equal(iframes.length, 1);
    assert.equal(iframes[0].src, 'http://localhost:5173/play');

    // Switch live game preset to Blackjack (`/play/blackjack`) and Baccarat (`/play/baccarat`)
    assert.equal(pie.setPreset('blackjack-play'), true);
    assert.equal(iframes[0].src, 'http://localhost:5173/play/blackjack');
    assert.equal(pie.setPreset('baccarat-play'), true);
    assert.equal(iframes[0].src, 'http://localhost:5173/play/baccarat');

    // Simulate entering a docked Gameplay Editor (`body.workbench-active`) -> `#viewport` hides, `#pie-stage` stays live!
    dom.body.classList.add('workbench-active');
    pie.syncStageVisibility();
    assert.equal(dom.viewport.hidden, true);
    assert.equal(dom.pieStage.hidden, false);
    assert.equal(dom.body.classList.contains('pie-split-active'), true);

    // Toggle Split-Play OFF -> removes `pie-split-active` and hides `#pie-stage` cleanly
    dom.pieSplitToggle.dispatch('click');
    assert.equal(pie.isSplitPlay(), false);
    assert.equal(dom.body.classList.contains('pie-split-active'), false);
    assert.equal(dom.pieSplitToggle.getAttribute('aria-pressed'), 'false');
    assert.equal(dom.pieStage.hidden, true);
  } finally {
    globalThis.document = prevDoc;
    globalThis.location = prevLoc;
  }
});
