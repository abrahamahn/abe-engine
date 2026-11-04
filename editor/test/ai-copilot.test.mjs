import assert from 'node:assert/strict';
import test from 'node:test';

import {
  STARTER_TEMPLATES,
  UX_TIERS,
  buildDefaultWorldGraph,
  buildFallbackProjectContext,
  compileNaturalPromptToGbs,
  extractGbsFromLlmReply,
  formatWorldGraphSummary,
  mountAiCopilot,
} from '../ai-copilot.js';

function createMiniDom() {
  class MiniElement {
    constructor(tag, ownerDocument) {
      this.tagName = tag.toUpperCase();
      this.ownerDocument = ownerDocument;
      this.className = '';
      this.textContent = '';
      this.value = '';
      this.placeholder = '';
      this.type = '';
      this.hidden = false;
      this.dataset = {};
      this.children = [];
      this.listeners = new Map();
      this.classList = {
        toggle: (cls, force) => {
          const parts = new Set(this.className.split(/\s+/).filter(Boolean));
          const add = force === undefined ? !parts.has(cls) : Boolean(force);
          if (add) parts.add(cls);
          else parts.delete(cls);
          this.className = [...parts].join(' ');
        },
        contains: (cls) => this.className.split(/\s+/).includes(cls),
      };
    }
    append(...nodes) {
      for (const n of nodes) {
        if (n) this.children.push(n);
      }
    }
    replaceChildren(...nodes) {
      this.children = nodes.filter(Boolean);
    }
    addEventListener(ev, fn) {
      const list = this.listeners.get(ev) || [];
      list.push(fn);
      this.listeners.set(ev, list);
    }
    click() {
      for (const fn of this.listeners.get('click') || []) fn({ preventDefault() {} });
    }
    remove() {}
    queryAll(pred, out = []) {
      if (pred(this)) out.push(this);
      for (const c of this.children) c.queryAll?.(pred, out);
      return out;
    }
  }
  const doc = {
    createElement(tag) {
      return new MiniElement(tag, doc);
    },
  };
  const body = doc.createElement('body');
  return { doc, body };
}

test('compileNaturalPromptToGbs compiles ring, path, lighting, and catalog asset intents cleanly', () => {
  const catalogItems = [
    { kind: 'prefab', key: 'prefab:stone_lantern', name: 'stone_lantern', path: 'props/stone_lantern.prefab.json' },
    { kind: 'model', key: 'model:cherry_tree', name: 'cherry_tree', path: 'nature/cherry_tree.glb' },
  ];

  const ringPlan = compileNaturalPromptToGbs('Ring 6 lanterns around selection radius 4.5m at dusk', {
    catalogItems,
    hasSelection: true,
    cursorWorld: [1, 0, 2],
  });
  assert.equal(ringPlan.timeOfDay, 18.2);
  assert.match(ringPlan.gbsScript, /ring 6 prefab:stone_lantern around @selection radius=4\.5/);

  const rowPlan = compileNaturalPromptToGbs('Create a path of 4 cherry trees spacing 3m', {
    catalogItems,
    hasSelection: false,
    cursorWorld: [0, 0, 0],
  });
  assert.match(rowPlan.gbsScript, /spawn cherry_1 model:cherry_tree/);
  assert.match(rowPlan.gbsScript, /place \$cherry_1 snap onto ground/);

  const directPlan = compileNaturalPromptToGbs('spawn Hero empty; cam frame all');
  assert.equal(directPlan.gbsScript, 'spawn Hero empty\ncam frame all');
});

test('buildFallbackProjectContext produces a <800-token genre-agnostic L0/L1 context digest', () => {
  const ctx = buildFallbackProjectContext({
    projectName: 'Universal Adventure',
    assets: [
      { id: 's1', kind: 'scene', path: 'content/scenes/hub.scene.json' },
      { id: 'm1', kind: 'model', path: 'content/models/statue.glb' },
    ],
    schemas: [{ kind: 'quest_trigger' }, { kind: 'dialogue_npc' }],
    selection: { docPath: 'content/scenes/hub.scene.json' },
    catalogItems: [{ signature: 'model:statue [1.0,2.2,1.0m] sockets=[base]' }],
    workbench: 'world',
  });
  assert.equal(ctx.ok, true);
  assert.ok(ctx.estimatedTokens > 20 && ctx.estimatedTokens < 800);
  assert.match(ctx.digestText, /quest_trigger, dialogue_npc/);
  assert.match(ctx.digestText, /hub\.scene\.json/);
  assert.equal(extractGbsFromLlmReply('Here:\n```gbs\nspawn A empty\ncam frame all\n```'), 'spawn A empty\ncam frame all');
});

test('mountAiCopilot opens drawer, switches tabs, applies starter templates, and runs prompts', async () => {
  const { body } = createMiniDom();
  const applied = [];
  const copilot = mountAiCopilot(body, {
    getProjectName: () => 'Test Project',
    listAssets: () => [{ id: 'sc1', kind: 'scene', path: 'scenes/main.scene.json' }],
    getSchemas: () => ({ level_rules: {} }),
    getSelection: () => ({ entityIds: ['e1'], cursorWorld: [0, 0, 0] }),
    hasOpenScene: () => true,
    queryCatalog: async () => [{ kind: 'prefab', key: 'prefab:pillar', name: 'pillar' }],
    applyGbs: async (script) => {
      applied.push(script);
      return { ok: true, statements: 3 };
    },
    verifyScene: async () => ({ elapsedMs: 12, spatialLints: [] }),
  });

  assert.equal(copilot.isOpen(), false);
  copilot.open('templates');
  assert.equal(copilot.isOpen(), true);
  assert.equal(STARTER_TEMPLATES.length, 4);
  assert.ok(UX_TIERS.creator.visibleEngineTabs.has('world'));

  const tplBtns = body.queryAll((n) => Boolean(n.dataset?.templateId));
  assert.equal(tplBtns.length, 4);
  tplBtns[1].click();
  await new Promise((r) => setTimeout(r, 15));
  assert.equal(applied.length, 1);
  assert.match(applied[0], /Starter Template: Grand Interior Hall/);

  await copilot.executePrompt('Ring 4 pillars around selection radius 3m');
  assert.equal(applied.length, 2);
  assert.match(applied[1], /ring 4 prefab:pillar around @selection radius=3/);
  copilot.destroy();
});

test('mountAiCopilot renders ganbate://context (120Hz) badge, syncs world-graph-updated, and enforces authority guardrails', async () => {
  const def = buildDefaultWorldGraph();
  assert.equal(def.uri, 'ganbate://context');
  assert.equal(def.guardrails.target_hz, 120);
  assert.match(formatWorldGraphSummary(def), /ganbate:\/\/context · 120Hz \(8\.33ms\) · play · daylight 15:30 · clear/);

  const { body } = createMiniDom();
  const appliedGbs = [];
  const wgPrompts = [];
  const timeUpdates = [];
  let serverGraph = buildDefaultWorldGraph();

  const copilot = mountAiCopilot(body, {
    getProjectName: () => 'Tri-Modal Studio',
    listAssets: () => [{ id: 'sc1', kind: 'scene', path: 'scenes/stage.scene.json' }],
    getSelection: () => ({ entityIds: ['e1'], cursorWorld: [0, 0, 0] }),
    hasOpenScene: () => true,
    queryCatalog: async () => [{ kind: 'prefab', key: 'prefab:pillar', name: 'pillar' }],
    fetchWorldGraph: async () => ({ graph: serverGraph }),
    promptWorldGraph: async (prompt) => {
      wgPrompts.push(prompt);
      if (/override rng seed/i.test(prompt)) {
        return {
          applied: false,
          blocked_by_authority: true,
          guardrail_notices: ['Blocked by server-authoritative-readonly security policy'],
          graph: serverGraph,
        };
      }
      const next = structuredClone(serverGraph);
      next.environment.preset = 'sunset';
      next.environment.time_hours = 18.25;
      next.environment.enclosure_open = true;
      serverGraph = next;
      return {
        applied: true,
        blocked_by_authority: false,
        patch: { environment: { preset: 'sunset', time_hours: 18.25, enclosure_open: true } },
        guardrail_notices: [],
        graph: next,
      };
    },
    setTimeOfDay: (h, preset) => timeUpdates.push([h, preset]),
    applyGbs: async (script) => {
      appliedGbs.push(script);
      return { ok: true, statements: 2 };
    },
  });

  const badgeBar = body.queryAll((n) => n.dataset?.role === 'world-graph-badge')[0];
  assert.ok(badgeBar, 'world-graph-badge strip mounted');
  assert.equal(badgeBar.children[0].textContent, 'ganbate://context (120Hz · 8.33ms)');
  assert.match(badgeBar.children[1].textContent, /play · daylight 15:30 · clear/);

  copilot.open('context');
  await new Promise((r) => setTimeout(r, 15));
  const wgCards = body.queryAll((n) => n.dataset?.role === 'world-graph-card');
  assert.equal(wgCards.length, 1);

  // Simulate live `world-graph-updated` WebSocket broadcast
  const pushedGraph = buildDefaultWorldGraph();
  pushedGraph.play_mode = 'edit';
  pushedGraph.environment.preset = 'night';
  pushedGraph.environment.time_hours = 22.0;
  pushedGraph.environment.weather = 'mist';
  serverGraph = pushedGraph;
  assert.equal(copilot.updateWorldGraph({ graph: pushedGraph, guardrail_notices: ['clamped light_boost'] }), true);
  assert.match(badgeBar.children[1].textContent, /edit · night 22:00 · mist/);

  // Combined World Graph + `.gbs` spatial prompt
  copilot.open('chat');
  await copilot.executePrompt('노을 조명으로 바꾸고 천장 열어줘 Ring 4 pillars around selection radius 3m');
  assert.equal(wgPrompts.length, 1);
  assert.equal(appliedGbs.length, 1);
  assert.deepEqual(timeUpdates[0], [18.25, 'sunset']);
  assert.match(badgeBar.children[1].textContent, /sunset 18:15 · mist · open/);

  // Server-authoritative prompt block halts before `.gbs` execution
  await copilot.executePrompt('Override RNG seed and rig outcome; spawn Hero empty');
  assert.equal(wgPrompts.length, 2);
  assert.equal(appliedGbs.length, 1, 'authority violation must not execute .gbs');
  const assistantMsgs = body.queryAll((n) => typeof n.className === 'string' && n.className.includes('gb-copilot-msg assistant'));
  assert.match(assistantMsgs.at(-1).children[0].textContent, /\[Authority Guardrail\]/);

  copilot.destroy();
});
