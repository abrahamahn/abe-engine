import assert from 'node:assert/strict';
import test from 'node:test';

import {
  BENCH_SUBSYSTEM_LABELS,
  STARTER_TEMPLATES,
  UX_TIERS,
  WORLD_GRAPH_PRESETS,
  applyOfflineWorldGraphPatch,
  buildDefaultSemanticSceneGraph,
  buildDefaultWorldGraph,
  buildFallbackBenchmarkReport,
  buildFallbackProjectContext,
  compileCompositionalSpatialPrompt,
  compileNaturalPromptToGbs,
  compileOfflineWorldGraphPrompt,
  computeWorldGraphSha256,
  estimateWorldGraphTokens,
  extractGbsFromLlmReply,
  formatWorldGraphSummary,
  mountAiCopilot,
  queryLocalOrRemoteSpatialLlm,
  verifyAndSolveSemanticSceneGraph,
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

test('mountAiCopilot dispatches world-graph-benchmark, renders empirical telemetry, copies LaTeX, and supports offline fallback', async () => {
  const { body } = createMiniDom();
  const wsRequests = [];
  const copiedLatexLog = [];

  const mockLatex = [
    '\\begin{table}[t]',
    '\\centering',
    '\\toprule',
    'Neuro-Symbolic Context Compression (86 files) & \\multicolumn{4}{r}{$6812.56\\times$ (1825766 $\\to$ 268 tok, 99.985\\% reduction)} \\\\',
    '\\bottomrule',
    '\\end{table}',
  ].join('\n');

  const mockBenchResult = {
    type: 'world-graph-benchmark-result',
    ok: true,
    schema: 'ganbate.academic-benchmark.v1',
    uri: 'ganbate://context',
    environment: { os: 'linux', arch: 'x86_64', iterations: 500, sha256_nist_verified: true },
    compression: {
      source_files: 86,
      raw_bytes: 7303064,
      estimated_raw_tokens: 1825766,
      graph_bytes: 1072,
      estimated_graph_tokens: 268,
      compression_ratio: 6812.56,
      token_reduction_pct: 99.9853,
    },
    isomorphism: {
      isomorphism_verified: true,
      invertibility_verified: true,
      byte_drift: 0,
    },
    guardrails: {
      authority_total_cases: 16,
      authority_rejected_cases: 16,
      budget_total_cases: 8,
      budget_clamped_cases: 8,
    },
    paper1_spatial_suite: {
      verified: true,
      total_tasks: 30,
      tasks_passed: 30,
      op_ctrl_z_sha256_recovered: 30,
      gbs_snapped_lint_free_tasks: 30,
    },
    paper2_ecosystem_governance: {
      verified: true,
      workspace_crates: 14,
      external_crates_io_dependencies: 0,
      xray_available: true,
      indexed_surfaces: 3518,
    },
    paper3_polyglot_systems: {
      verified: true,
      shader_targets: [
        { id: 'spirv_1_0', verified: true },
        { id: 'msl_2_1', verified: true },
        { id: 'webgpu_wgsl', verified: true },
        { id: 'webgl2_glsl_es_300', verified: true },
      ],
      gbew_cooker: { version: 4, bytes_per_triangle: 6.9, roundtrip_verified: true },
      physics_determinism: { steps: 10000, bit_exact_verified: true, zero_nan_verified: true },
    },
    latencies_us: [
      { id: 'world_graph_roundtrip', label: 'SemanticWorldGraph Round-Trip', p50_us: 10.62, p95_us: 13.71 },
      { id: 'tri_modal_prompt_compile_and_patch', label: 'Tri-Modal Prompt Compile & Patch', p50_us: 8.63, p95_us: 10.89 },
      { id: 'stereo_xr_frustum_pair', label: 'Omni Stereo XR Frustum Pair (63mm IPD)', p50_us: 0.14, p95_us: 0.17 },
      { id: 'upper_body_ik_sanitize', label: 'Upper-Body 6-DoF IK Pose Sanitization', p50_us: 0.05, p95_us: 0.06 },
      { id: 'governor_closed_loop_step', label: '120Hz Frame-Budget Governor Closed-Loop Tick', p50_us: 0.05, p95_us: 0.06 },
    ],
    latex: mockLatex,
  };

  const copilot = mountAiCopilot(body, {
    getProjectName: () => 'Empirical Studio',
    fetchWorldGraph: async () => ({ graph: buildDefaultWorldGraph() }),
    benchmarkWorldGraph: async (iterations) => {
      wsRequests.push({ type: 'world-graph-benchmark', iterations });
      return mockBenchResult;
    },
    onCopyLatex: (tex) => copiedLatexLog.push(tex),
  });

  copilot.open('context');
  await new Promise((r) => setTimeout(r, 15));

  const wgCard = body.queryAll((n) => n.dataset?.role === 'world-graph-card')[0];
  assert.ok(wgCard, 'world-graph-card mounted');

  const runBtn = wgCard.queryAll((n) => n.dataset?.action === 'run-world-graph-bench')[0];
  assert.ok(runBtn, 'run-world-graph-bench button mounted');
  assert.equal(runBtn.textContent, '🎓 Run Empirical Benchmark');

  runBtn.click();
  await new Promise((r) => setTimeout(r, 15));

  assert.deepEqual(wsRequests, [{ type: 'world-graph-benchmark', iterations: 500 }]);

  // Re-query inside re-rendered context tab
  const liveCard = body.queryAll((n) => n.dataset?.role === 'world-graph-card')[0];
  const benchOut = liveCard.queryAll((n) => n.dataset?.role === 'world-graph-bench-output')[0];
  assert.ok(benchOut, 'world-graph-bench-output rendered');

  const allText = benchOut.queryAll(() => true).map((n) => n.textContent).join(' | ');
  assert.match(allText, /6,812\.56x · 268 tok/);
  assert.match(allText, /Code == UI == EN == KO: PASS · 0B drift/);
  assert.match(allText, /16\/16 · 8\/8/);
  assert.match(allText, /Paper 1: 30\/30 \.gbs/);
  assert.match(allText, /Paper 2: 14 crates · 0 ext deps · 3518 surf/);
  assert.match(allText, /Paper 3: 4 RHI · ~6\.9 B\/tri · 10k-step bit-exact/);
  for (const label of Object.values(BENCH_SUBSYSTEM_LABELS)) {
    assert.ok(allText.includes(label), `expected subsystem row ${label} in latency table`);
  }
  assert.match(allText, /10\.62/);
  assert.match(allText, /13\.71/);

  const copyLatexBtn = liveCard.queryAll((n) => n.dataset?.action === 'copy-world-graph-latex')[0];
  assert.ok(copyLatexBtn, 'copy-world-graph-latex button mounted');
  copyLatexBtn.click();

  assert.equal(copilot.getLastCopiedLatex(), mockLatex);
  assert.deepEqual(copiedLatexLog, [mockLatex]);
  assert.equal(copyLatexBtn.textContent, 'Copied LaTeX!');
  copilot.destroy();

  // Verify deterministic offline fallback when disconnected (`benchmarkWorldGraph` returns null)
  const { body: offlineBody } = createMiniDom();
  const offlineCopilot = mountAiCopilot(offlineBody, {
    listAssets: () => [{ id: 'a1', kind: 'scene', path: 'scenes/stage.scene.json' }],
    benchmarkWorldGraph: async () => null,
  });
  offlineCopilot.open('context');
  await new Promise((r) => setTimeout(r, 15));
  const fallbackRes = await offlineCopilot.runWorldGraphBenchmark(120);
  assert.equal(fallbackRes.ok, true);
  assert.equal(fallbackRes.offline_fallback, true);
  assert.match(fallbackRes.latex, /\\begin\{table\}\[t\]/);
  assert.equal(buildFallbackBenchmarkReport({ iterations: 32 }).latencies_us.length, 5);
  const offlineOut = offlineBody.queryAll((n) => n.dataset?.role === 'world-graph-bench-output')[0];
  const offlineText = offlineOut.queryAll(() => true).map((n) => n.textContent).join(' | ');
  assert.match(offlineText, /Offline Fallback · N=120/);
  assert.match(offlineText, /Code == UI == EN == KO: PASS · 0B drift/);
  offlineCopilot.destroy();
});

test('Live ganbate://context Stage Director supports online RPC and offline mode, presets, enclosure, EN/KO prompt, clamps, authority rejection, and undo/redo', async () => {
  assert.equal(WORLD_GRAPH_PRESETS.map((p) => p.id).join(','), 'daylight,sunset,dusk,night,dawn');

  // 1. Online WebSocket RPC round-trip
  const { body } = createMiniDom();
  const rpcCalls = [];
  let hostGraph = buildDefaultWorldGraph();
  const undoStack = [];
  const redoStack = [];
  const stateReply = (type = 'world-graph-state', extra = {}) => ({
    ok: !extra.blocked_by_authority,
    type,
    graph: structuredClone(hostGraph),
    sha256: computeWorldGraphSha256(hostGraph),
    estimated_tokens: estimateWorldGraphTokens(hostGraph),
    undo_count: undoStack.length,
    redo_count: redoStack.length,
    can_undo: undoStack.length > 0,
    can_redo: redoStack.length > 0,
    guardrail_notices: [],
    ...extra,
  });

  const copilot = mountAiCopilot(body, {
    fetchWorldGraph: async () => {
      rpcCalls.push('fetch');
      return stateReply('world-graph-state');
    },
    patchWorldGraph: async (patch) => {
      rpcCalls.push(['patch', patch]);
      const res = applyOfflineWorldGraphPatch(hostGraph, patch);
      if (!res.blocked_by_authority && res.applied) {
        undoStack.push(structuredClone(hostGraph));
        redoStack.length = 0;
        hostGraph = res.graph;
      }
      return stateReply(res.blocked_by_authority ? 'world-graph-rejected' : 'world-graph-patched', {
        applied: res.applied,
        blocked_by_authority: res.blocked_by_authority,
        guardrail_notices: res.guardrail_notices,
        patch,
      });
    },
    promptWorldGraph: async (prompt) => {
      rpcCalls.push(['prompt', prompt]);
      const res = compileOfflineWorldGraphPrompt(hostGraph, prompt);
      if (!res.blocked_by_authority && res.applied) {
        undoStack.push(structuredClone(hostGraph));
        redoStack.length = 0;
        hostGraph = res.graph;
      }
      return stateReply(res.blocked_by_authority ? 'world-graph-rejected' : 'world-graph-prompted', {
        applied: res.applied,
        blocked_by_authority: res.blocked_by_authority,
        guardrail_notices: res.guardrail_notices,
        patch: res.patch,
      });
    },
    undoWorldGraph: async () => {
      rpcCalls.push('undo');
      const prev = undoStack.pop();
      redoStack.push(structuredClone(hostGraph));
      hostGraph = prev;
      return stateReply('world-graph-state', {
        applied: true,
        guardrail_notices: ['undid world graph change'],
      });
    },
    redoWorldGraph: async () => {
      rpcCalls.push('redo');
      const next = redoStack.pop();
      undoStack.push(structuredClone(hostGraph));
      hostGraph = next;
      return stateReply('world-graph-state', {
        applied: true,
        guardrail_notices: ['redid world graph change'],
      });
    },
  });

  copilot.open('chat');
  await new Promise((r) => setTimeout(r, 15));

  const getDirector = () => body.queryAll((n) => n.dataset?.role === 'world-graph-director')[0];
  assert.ok(getDirector(), 'world-graph-director mounted');

  const initialBadge = getDirector().queryAll((n) => n.dataset?.role === 'world-graph-digest-badge')[0];
  assert.ok(initialBadge);
  const sha0 = initialBadge.dataset.sha256;
  assert.equal(sha0.length, 64);
  assert.ok(Number(initialBadge.dataset.tokens) < 300);

  // Toggle Play -> Edit
  const modeBtn = getDirector().queryAll((n) => n.dataset?.action === 'world-graph-toggle-mode')[0];
  modeBtn.click();
  await new Promise((r) => setTimeout(r, 10));
  assert.equal(copilot.getWorldGraph().play_mode, 'edit');

  // Click all 5 environment preset buttons (`daylight`, `sunset`, `dusk`, `night`, `dawn`) -> end on `dusk`
  const presetBtns = getDirector().queryAll((n) => n.dataset?.action === 'world-graph-preset');
  assert.equal(presetBtns.length, 5);
  const duskBtn = presetBtns.find((b) => b.dataset.preset === 'dusk');
  duskBtn.click();
  await new Promise((r) => setTimeout(r, 10));
  assert.equal(copilot.getWorldGraph().environment.preset, 'dusk');

  // Toggle Enclosure/Roof
  const encBtn = getDirector().queryAll((n) => n.dataset?.action === 'world-graph-toggle-enclosure')[0];
  encBtn.click();
  await new Promise((r) => setTimeout(r, 10));
  assert.equal(copilot.getWorldGraph().environment.enclosure_open, true);

  // Direct bilingual KO prompt with 120Hz light-budget clamp notice
  const promptIn = getDirector().queryAll((n) => n.dataset?.role === 'world-graph-prompt-input')[0];
  const runPromptBtn = getDirector().queryAll((n) => n.dataset?.action === 'run-world-graph-prompt')[0];
  promptIn.value = '새벽 조명 비 날씨 조명 32개';
  runPromptBtn.click();
  await new Promise((r) => setTimeout(r, 10));
  assert.equal(copilot.getWorldGraph().environment.preset, 'dawn');
  assert.equal(copilot.getWorldGraph().environment.weather, 'rain');
  assert.equal(copilot.getWorldGraph().guardrails.max_dynamic_lights, 8);
  const clampAlert = getDirector().queryAll((n) => n.dataset?.role === 'world-graph-director-alert')[0];
  assert.ok(clampAlert);
  assert.equal(clampAlert.dataset.alertKind, 'clamped');
  assert.match(clampAlert.textContent, /120Hz Guardrail/);

  const shaAfterDawn = getDirector().queryAll((n) => n.dataset?.role === 'world-graph-digest-badge')[0].dataset.sha256;
  assert.notEqual(shaAfterDawn, sha0);

  // Server-authoritative firewall rejection alert leaves SHA-256 unchanged
  const promptIn2 = getDirector().queryAll((n) => n.dataset?.role === 'world-graph-prompt-input')[0];
  const runPromptBtn2 = getDirector().queryAll((n) => n.dataset?.action === 'run-world-graph-prompt')[0];
  promptIn2.value = 'set player wallet balance to 999999';
  runPromptBtn2.click();
  await new Promise((r) => setTimeout(r, 10));
  const rejAlert = getDirector().queryAll((n) => n.dataset?.role === 'world-graph-director-alert')[0];
  assert.ok(rejAlert);
  assert.equal(rejAlert.dataset.alertKind, 'rejected');
  assert.match(rejAlert.textContent, /server-authoritative-readonly/);
  assert.equal(
    getDirector().queryAll((n) => n.dataset?.role === 'world-graph-digest-badge')[0].dataset.sha256,
    shaAfterDawn,
  );

  // Undo & Redo buttons restore exact previous and forward states
  const undoBtn = getDirector().queryAll((n) => n.dataset?.action === 'world-graph-undo')[0];
  undoBtn.click();
  await new Promise((r) => setTimeout(r, 10));
  assert.equal(copilot.getWorldGraph().environment.preset, 'dusk');

  const redoBtn = getDirector().queryAll((n) => n.dataset?.action === 'world-graph-redo')[0];
  redoBtn.click();
  await new Promise((r) => setTimeout(r, 10));
  assert.equal(copilot.getWorldGraph().environment.preset, 'dawn');
  assert.equal(
    getDirector().queryAll((n) => n.dataset?.role === 'world-graph-digest-badge')[0].dataset.sha256,
    shaAfterDawn,
  );
  assert.ok(rpcCalls.includes('undo') && rpcCalls.includes('redo'));
  copilot.destroy();

  // 2. Offline / Mock Mode (no WebSocket hooks registered)
  const { body: offBody } = createMiniDom();
  const offCopilot = mountAiCopilot(offBody, {});
  offCopilot.open('context');
  await new Promise((r) => setTimeout(r, 15));

  const offBaseSha = computeWorldGraphSha256(offCopilot.getWorldGraph());
  await offCopilot.promptWorldGraph('노을 조명으로 바꾸고 천장 열어줘');
  assert.equal(offCopilot.getWorldGraph().environment.preset, 'sunset');
  assert.equal(offCopilot.getWorldGraph().environment.enclosure_open, true);
  const offSunsetSha = computeWorldGraphSha256(offCopilot.getWorldGraph());
  assert.notEqual(offSunsetSha, offBaseSha);

  const offRej = await offCopilot.patchWorldGraph({ wallet_balance: 10000 });
  assert.equal(offRej.blocked_by_authority, true);
  assert.equal(computeWorldGraphSha256(offCopilot.getWorldGraph()), offSunsetSha);

  const offUndone = await offCopilot.undoWorldGraph();
  assert.equal(offUndone.applied, true);
  assert.equal(offUndone.sha256, offBaseSha);

  const offRedone = await offCopilot.redoWorldGraph();
  assert.equal(offRedone.applied, true);
  assert.equal(offRedone.sha256, offSunsetSha);
  offCopilot.destroy();
});

test('Node+Edge Semantic Scene Graph (v2), Pre-Execution Spatial Constraint Verifier, and Local Ollama / Compositional Compiler work end-to-end', async () => {
  const sg = buildDefaultSemanticSceneGraph();
  assert.equal(sg.schema, 'ganbate.semantic.scene-graph.v2');
  assert.equal(sg.nodes.length, 3);
  assert.equal(sg.relations.length, 3);

  const comp = compileCompositionalSpatialPrompt(
    'Put 2 chip_stacks on top of poker_table_main and place dealer_rei behind poker_table_main facing the table',
    sg,
  );
  assert.equal(comp.ok, true);
  const stack1 = comp.graph.nodes.find((n) => n.id === 'chip_stack_1');
  const stack2 = comp.graph.nodes.find((n) => n.id === 'chip_stack_2');
  assert.ok(stack1 && stack2, 'spawned 2 chip_stack nodes');
  assert.equal(stack1.transform.pos[1], 0.78, 'snapped onto poker_table_main top surface y=0.78m');
  assert.equal(stack2.transform.pos[1], 0.78, 'snapped onto poker_table_main top surface y=0.78m');
  assert.ok(Math.abs(stack1.transform.pos[0] - stack2.transform.pos[0]) >= 0.36, 'non-overlapping XZ separation');
  assert.match(comp.gbsScript, /spawn chip_stack_1/);

  // Test Local Ollama JSON-schema mocked response + pre-execution AABB & floor constraint verifier
  const ollamaOut = await queryLocalOrRemoteSpatialLlm(
    'Add a lantern on top of poker_table_main',
    sg,
    { enableLocalOllama: true, model: 'qwen2.5-coder:1.5b' },
    async () => ({
      ok: true,
      json: async () => ({
        message: {
          content: JSON.stringify({
            nodes: [
              ...sg.nodes,
              {
                id: 'table_lantern',
                category: 'prop.light',
                prefab: 'prefab:lantern',
                transform: { pos: [0, -0.5, 0], yaw_deg: 0, scale: [1, 1, 1] },
                bounds: { type: 'aabb', size: [0.3, 0.4, 0.3] },
              },
            ],
            relations: [
              ...sg.relations,
              { subject: 'table_lantern', predicate: 'on_top_of', object: 'poker_table_main' },
            ],
          }),
        },
      }),
    }),
  );
  assert.equal(ollamaOut.source, 'local-ollama-llm');
  const lantern = ollamaOut.graph.nodes.find((n) => n.id === 'table_lantern');
  assert.equal(lantern.transform.pos[1], 0.78, 'verifier corrected y=-0.5 to table top y=0.78 before 3D execution');
  assert.ok(verifyAndSolveSemanticSceneGraph(ollamaOut.graph).verified);
});
