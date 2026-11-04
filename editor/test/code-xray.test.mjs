// editor/test/code-xray.test.mjs
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

import {
  DECISIONS_DOC_PATH,
  DECISIONS_SCHEMA,
  DEFAULT_CATALOG_URLS,
  OFFICIAL_STATUSES,
  RUNTIME_EXPOSURES,
  SURFACE_BUCKETS,
  TRIAGE_ACTIONS,
  buildDependencyNeighborhood,
  compareGlbClusterItems,
  computeCatalogKpis,
  exportTriageActionPlanMarkdown,
  exportTriageLedgerJson,
  filterCatalogItems,
  loadTriageDecisionsFromStorage,
  loadXrayCatalog,
  mountCodeXray,
  mountCodeXrayWorkbench,
  normalizeTriageAction,
  persistTriageDecisionsToStudio,
  saveTriageDecisionsToStorage,
} from '../code-xray.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

class MiniElement {
  constructor(tag, ownerDocument) {
    this.tagName = String(tag).toUpperCase();
    this.ownerDocument = ownerDocument;
    this.className = '';
    this.value = '';
    this.attributes = new Map();
    this.listeners = new Map();
    this.children = [];
    this.parentNode = null;
    this._innerHTML = '';
  }

  get innerHTML() {
    return this._innerHTML;
  }

  set innerHTML(val) {
    this._innerHTML = String(val);
    if (val === '') this.children = [];
  }

  setAttribute(k, v) {
    this.attributes.set(k, String(v));
  }

  getAttribute(k) {
    return this.attributes.get(k) ?? null;
  }

  addEventListener(type, fn) {
    const arr = this.listeners.get(type) || [];
    arr.push(fn);
    this.listeners.set(type, arr);
  }

  dispatch(type, eventInit = {}) {
    const ev = { target: this, currentTarget: this, ...eventInit };
    for (const fn of this.listeners.get(type) || []) {
      fn(ev);
    }
  }

  append(...nodes) {
    for (const n of nodes) {
      if (n === null || n === undefined) continue;
      if (typeof n === 'object') n.parentNode = this;
      this.children.push(n);
    }
  }

  remove() {
    if (!this.parentNode) return;
    this.parentNode.children = this.parentNode.children.filter((c) => c !== this);
    this.parentNode = null;
  }

  textContentDeep() {
    let out = '';
    for (const c of this.children) {
      if (typeof c === 'string') out += c;
      else if (c?.nodeType === 3) out += c.text;
      else if (c?.textContentDeep) out += c.textContentDeep();
    }
    return out;
  }

  findAll(predicate, acc = []) {
    if (predicate(this)) acc.push(this);
    for (const c of this.children) {
      if (c && typeof c.findAll === 'function') c.findAll(predicate, acc);
    }
    return acc;
  }
}

function createMiniDom() {
  const doc = {
    head: null,
    createElement(tag) {
      return new MiniElement(tag, doc);
    },
    createTextNode(text) {
      return { nodeType: 3, text: String(text) };
    },
    querySelector() {
      return null;
    },
  };
  doc.head = new MiniElement('head', doc);
  return doc;
}

function createMemoryStorage() {
  const map = new Map();
  return {
    getItem: (k) => (map.has(k) ? map.get(k) : null),
    setItem: (k, v) => map.set(k, String(v)),
    removeItem: (k) => map.delete(k),
  };
}

function createSampleCatalog() {
  const realCandidate = path.resolve(
    __dirname,
    '../../../lane-code-xray-web/main/apps/web/public/ganbate-studio/code-surface-catalog.json',
  );
  if (fs.existsSync(realCandidate)) {
    return JSON.parse(fs.readFileSync(realCandidate, 'utf8'));
  }

  // Standalone engine-only CI fixture matching ganbate.code-xray-catalog/2 schema
  const sharedImgHash = 'f01938938f63aa11223344556677889900aabbccddeeff001122334455667788';
  return {
    schema: 'ganbate.code-xray-catalog/2',
    generatedAt: '2026-11-04T09:45:00.000Z',
    clusters: [
      {
        id: 'cluster:dealer-rei-models',
        title: 'Dealer Rei 3D Models & Workshop Iterations',
        severity: 'high',
        kind: '3d-turntable-compare',
        canonicalItemId: 'character-model:rei-v9.glb',
        summary: 'Compare Dealer Rei glTF iterations.',
        itemCount: 3,
        itemIds: [
          'character-model:rei-v9.glb',
          'character-model:rei-v8.glb',
          'character-model:rei-v9-copy.glb',
        ],
        totalBytes: 18_000_000,
        reclaimableBytes: 12_000_000,
        totalLoc: 0,
        reclaimableLoc: 0,
        implementationPct: 92,
        integrationPct: 45,
        pairs: [],
      },
      {
        id: 'cluster:editors-workbenches-overlap',
        title: 'Editors & Workbenches Overlap',
        severity: 'high',
        kind: 'workbench-absorption-pairs',
        canonicalItemId: 'editor-tool:engine-audio-daw',
        summary: 'DevStudio vs Ganbate Studio.',
        itemCount: 2,
        itemIds: ['editor-tool:AudioCueMixer', 'editor-tool:engine-audio-daw'],
        totalBytes: 160_000,
        reclaimableBytes: 40_000,
        totalLoc: 2400,
        reclaimableLoc: 620,
        implementationPct: 88,
        integrationPct: 55,
        pairs: [
          {
            role: 'Audio & Foley DAW',
            legacyItemIds: ['editor-tool:AudioCueMixer'],
            targetItemIds: ['editor-tool:engine-audio-daw'],
          },
        ],
      },
    ],
    items: [
      {
        id: 'character-model:rei-v9.glb',
        title: 'rei-v9.glb',
        category: 'character-model',
        repo: 'ganbate-main',
        sourcePath: 'main/apps/web/public/games/characters/rei-v9.glb',
        repoPath: 'main/apps/web/public/games/characters/rei-v9.glb',
        owningSubsystem: 'web-character-assets',
        fileSize: 6_000_000,
        loc: 0,
        sha256: '1111111111111111111111111111111111111111111111111111111111111111',
        provenance: {
          sha256: '1111111111111111111111111111111111111111111111111111111111111111',
          shortHash: '111111111111',
          identicalHashPeers: ['character-model:rei-v9-copy.glb'],
          forkGroup: 'fork-group:dealer-rei',
          existsOnMainBranch: true,
        },
        referenceCount: 4,
        consumers: [
          { itemId: 'editor-tool:engine-audio-daw', repo: 'ganbate-main', path: 'GanbatePokerStage.tsx', kind: 'asset-ref' },
        ],
        dependencies: [],
        editorExposure: 'asset-library-only',
        authoritySource: { kind: 'binary-asset', ref: 'rei-v9.glb' },
        candidateCanonicalId: 'character-model:rei-v9.glb',
        status: 'canonical',
        surfaceBucket: 'live',
        runtimeExposure: 'live_production',
        statusReason: 'Canonical Dealer Rei.',
        implementationPct: 100,
        integrationPct: 100,
        recommendedAction: 'Keep',
        preview: { kind: 'glb', assetUrl: '/games/characters/rei-v9.glb', sandboxKey: 'glb:rei-v9' },
        glbStats: { meshes: 6, primitives: 8, vertices: 24000, triangles: 42000, materials: 4, textures: 4, bones: 65, morphTargets: 24, animations: 8 },
        clusterIds: ['cluster:dealer-rei-models'],
      },
      {
        id: 'character-model:rei-v8.glb',
        title: 'rei-v8.glb',
        category: 'character-model',
        repo: 'ganbate-main',
        sourcePath: 'main/apps/web/public/games/characters/rei-v8.glb',
        repoPath: 'main/apps/web/public/games/characters/rei-v8.glb',
        owningSubsystem: 'web-character-assets',
        fileSize: 6_200_000,
        loc: 0,
        sha256: '2222222222222222222222222222222222222222222222222222222222222222',
        provenance: {
          sha256: '2222222222222222222222222222222222222222222222222222222222222222',
          shortHash: '222222222222',
          identicalHashPeers: [],
          forkGroup: 'fork-group:dealer-rei',
          existsOnMainBranch: true,
        },
        referenceCount: 1,
        consumers: [],
        dependencies: [],
        editorExposure: 'asset-library-only',
        authoritySource: { kind: 'binary-asset', ref: 'rei-v8.glb' },
        candidateCanonicalId: 'character-model:rei-v9.glb',
        status: 'deprecated',
        surfaceBucket: 'duplicate',
        runtimeExposure: 'editor_only',
        statusReason: 'Superseded fork.',
        implementationPct: 95,
        integrationPct: 18,
        recommendedAction: 'Refactor',
        preview: { kind: 'glb', assetUrl: '/games/characters/rei-v8.glb', sandboxKey: 'glb:rei-v8' },
        glbStats: { meshes: 5, primitives: 6, vertices: 21000, triangles: 38000, materials: 3, textures: 3, bones: 65, morphTargets: 12, animations: 5 },
        clusterIds: ['cluster:dealer-rei-models'],
      },
      {
        id: 'character-model:rei-v9-copy.glb',
        title: 'rei-v9-copy.glb',
        category: 'character-model',
        repo: 'ganbate-main',
        sourcePath: 'main/tools/scripts/art/sources/rei-v9-copy.glb',
        repoPath: 'main/tools/scripts/art/sources/rei-v9-copy.glb',
        owningSubsystem: 'art-workshop-sources',
        fileSize: 6_000_000,
        loc: 0,
        sha256: '1111111111111111111111111111111111111111111111111111111111111111',
        provenance: {
          sha256: '1111111111111111111111111111111111111111111111111111111111111111',
          shortHash: '111111111111',
          identicalHashPeers: ['character-model:rei-v9.glb'],
          forkGroup: 'fork-group:dealer-rei',
          existsOnMainBranch: true,
        },
        referenceCount: 0,
        consumers: [],
        dependencies: [],
        editorExposure: 'none',
        authoritySource: { kind: 'binary-asset', ref: 'rei-v9-copy.glb' },
        candidateCanonicalId: 'character-model:rei-v9.glb',
        status: 'orphaned',
        surfaceBucket: 'orphan',
        runtimeExposure: 'unwired_code',
        statusReason: 'Exact copy.',
        implementationPct: 100,
        integrationPct: 0,
        recommendedAction: 'Delete',
        preview: { kind: 'glb', assetUrl: null, sandboxKey: 'glb:rei-v9-copy' },
        glbStats: { meshes: 6, primitives: 8, vertices: 24000, triangles: 42000, materials: 4, textures: 4, bones: 65, morphTargets: 24, animations: 8 },
        clusterIds: ['cluster:dealer-rei-models'],
      },
      {
        id: 'editor-tool:AudioCueMixer',
        title: 'AudioCueMixer.tsx (DevStudio Audio & Foley DAW)',
        category: 'editor-tool',
        repo: 'ganbate-main',
        sourcePath: 'main/apps/web/src/games/dev-studio/AudioCueMixer.tsx',
        repoPath: 'main/apps/web/src/games/dev-studio/AudioCueMixer.tsx',
        owningSubsystem: 'web-dev-studio',
        fileSize: 40_000,
        loc: 620,
        sha256: sharedImgHash,
        provenance: {
          sha256: sharedImgHash,
          shortHash: sharedImgHash.slice(0, 12),
          identicalHashPeers: [],
          forkGroup: null,
          existsOnMainBranch: true,
        },
        referenceCount: 2,
        consumers: [
          { itemId: null, repo: 'ganbate-main', path: 'DevStudioPage.tsx', kind: 'es-import' },
        ],
        dependencies: [
          { itemId: 'character-model:rei-v9.glb', repo: 'ganbate-main', path: 'roomAudio.ts', kind: 'es-import' },
        ],
        editorExposure: 'dev-studio-panel',
        authoritySource: { kind: 'localStorage', ref: 'localStorage:ganbate.dev.audioMixer.v1' },
        candidateCanonicalId: 'editor-tool:engine-audio-daw',
        status: 'legacy',
        surfaceBucket: 'dev-studio-migration',
        runtimeExposure: 'editor_only',
        statusReason: 'Isolated DevStudio panel.',
        implementationPct: 90,
        integrationPct: 15,
        recommendedAction: 'Merge',
        preview: { kind: 'sandbox-widget', assetUrl: null, sandboxKey: 'tool:AudioCueMixer' },
        glbStats: null,
        clusterIds: ['cluster:editors-workbenches-overlap'],
      },
      {
        id: 'editor-tool:engine-audio-daw',
        title: 'Ganbate Studio · editor/audio-daw.js',
        category: 'editor-tool',
        repo: 'ganbate-engine',
        sourcePath: 'editor/audio-daw.js',
        repoPath: 'editor/audio-daw.js',
        owningSubsystem: 'engine-studio-editor',
        fileSize: 120_000,
        loc: 1780,
        sha256: '3333333333333333333333333333333333333333333333333333333333333333',
        provenance: {
          sha256: '3333333333333333333333333333333333333333333333333333333333333333',
          shortHash: '333333333333',
          identicalHashPeers: [],
          forkGroup: null,
          existsOnMainBranch: true,
        },
        referenceCount: 2,
        consumers: [],
        dependencies: [],
        editorExposure: 'studio-workbench',
        authoritySource: { kind: 'json-manifest', ref: 'ganbate-studio-host-protocol' },
        candidateCanonicalId: 'editor-tool:engine-audio-daw',
        status: 'canonical',
        surfaceBucket: 'live',
        runtimeExposure: 'editor_only',
        statusReason: 'Canonical Studio Audio DAW.',
        implementationPct: 98,
        integrationPct: 95,
        recommendedAction: 'Keep',
        preview: { kind: 'code-summary', assetUrl: null, sandboxKey: 'engine-editor:audio-daw' },
        glbStats: null,
        clusterIds: ['cluster:editors-workbenches-overlap'],
      },
    ],
  };
}

test('loadXrayCatalog probes exact URL fallback chain and falls back to studioApi.listAssets()', async () => {
  assert.deepEqual(DEFAULT_CATALOG_URLS, [
    '/api/project/xray-catalog',
    '/ganbate-studio/code-surface-catalog.json',
    './code-surface-catalog.json',
  ]);

  const seenUrls = [];
  const sample = createSampleCatalog();
  const resBrowser = await loadXrayCatalog({
    fetchImpl: async (url) => {
      seenUrls.push(url);
      if (url === '/ganbate-studio/code-surface-catalog.json') {
        return { ok: true, json: async () => sample };
      }
      return { ok: false, status: 404 };
    },
  });
  assert.equal(resBrowser.sourceUrl, '/ganbate-studio/code-surface-catalog.json');
  assert.deepEqual(seenUrls, [
    '/api/project/xray-catalog',
    '/ganbate-studio/code-surface-catalog.json',
  ]);

  const resOffline = await loadXrayCatalog({
    fetchImpl: async () => ({ ok: false, status: 503 }),
    studioApi: {
      listAssets: () => [{ id: 'a1', path: 'content/rei.glb', bytes: 4096, usages: 2 }],
    },
  });
  assert.equal(resOffline.sourceUrl, 'studio-assets-fallback');
  assert.equal(resOffline.catalog.items.length, 1);
});

test('KPIs, filters, dependency graph, and SHA-256 glTF fork vs copy comparison work deterministically', () => {
  const catalog = createSampleCatalog();
  assert.ok(OFFICIAL_STATUSES.includes('canonical'));
  assert.ok(SURFACE_BUCKETS.includes('dev-studio-migration'));
  assert.ok(RUNTIME_EXPOSURES.includes('worktree_only'));
  assert.deepEqual(TRIAGE_ACTIONS, ['Keep', 'Merge', 'Wire', 'Refactor', 'Archive', 'Delete']);

  const kpis = computeCatalogKpis(catalog, {});
  assert.equal(kpis.totalItems, catalog.items.length);
  assert.ok(kpis.reclaimableBytes > 0);
  assert.ok(kpis.avgImplementationPct >= kpis.avgIntegrationPct);

  const devStudioOnly = filterCatalogItems(catalog.items, {
    surfaceBucket: 'dev-studio-migration',
  });
  assert.ok(devStudioOnly.some((i) => i.id === 'editor-tool:AudioCueMixer'));

  const byLocalStorage = filterCatalogItems(catalog.items, { query: 'localStorage' });
  assert.ok(byLocalStorage.length >= 1);

  const hood = buildDependencyNeighborhood(catalog, 'editor-tool:AudioCueMixer');
  assert.ok(hood);
  assert.equal(hood.canonicalTarget?.id, 'editor-tool:engine-audio-daw');
  assert.ok(hood.consumers.length >= 1);

  const glbRows = compareGlbClusterItems(catalog, 'cluster:dealer-rei-models');
  assert.ok(glbRows.length >= 2);
  const canonRow = glbRows.find((r) => r.id === 'character-model:rei-v9.glb');
  const forkRow = glbRows.find((r) => r.id === 'character-model:rei-v8.glb');
  assert.equal(canonRow.provenanceRelation, 'CANONICAL');
  assert.equal(forkRow.provenanceRelation, 'MODIFIED FORK');
});

test('non-destructive human triage persists to localStorage and Studio host and exports JSON/Markdown', async () => {
  const catalog = createSampleCatalog();
  const storage = createMemoryStorage();
  assert.equal(normalizeTriageAction('merge-into-studio'), 'Merge');
  assert.equal(normalizeTriageAction('mark-for-deletion'), 'Delete');

  const decisions = {
    'editor-tool:AudioCueMixer': {
      action: 'Merge',
      notes: 'Absorb foley buses into editor/audio-daw.js',
      updatedAt: '2026-11-04T09:50:00.000Z',
    },
    'character-model:rei-v8.glb': {
      action: 'Archive',
      notes: 'Keep rei-v9.glb canonical in public/',
      updatedAt: '2026-11-04T09:50:00.000Z',
    },
  };

  saveTriageDecisionsToStorage(decisions, storage);
  const loaded = loadTriageDecisionsFromStorage(storage);
  assert.equal(loaded['editor-tool:AudioCueMixer'].action, 'Merge');
  assert.equal(loaded['character-model:rei-v8.glb'].action, 'Archive');

  const createdDocs = [];
  const saveRes = await persistTriageDecisionsToStudio(
    loaded,
    {
      getDocument: () => null,
      createDocument: async (kind, docPath, payload) => {
        createdDocs.push({ kind, docPath, payload });
      },
      save: async () => {},
    },
    null,
  );
  assert.equal(saveRes.saved, true);
  assert.equal(saveRes.target, 'studio-create');
  assert.equal(createdDocs[0].docPath, DECISIONS_DOC_PATH);
  assert.equal(createdDocs[0].payload.zeroAutoDelete, true);

  const jsonLedger = JSON.parse(exportTriageLedgerJson(catalog, loaded));
  assert.equal(jsonLedger.schema, DECISIONS_SCHEMA);
  assert.equal(jsonLedger.zeroAutoDelete, true);
  assert.equal(jsonLedger.totalDecisions, 2);

  const markdownPlan = exportTriageActionPlanMarkdown(catalog, loaded);
  assert.match(markdownPlan, /## ⇄ Merge/);
  assert.match(markdownPlan, /AudioCueMixer/);
  assert.match(markdownPlan, /## 📦 Archive/);
});

test('mountCodeXray renders workbench UI, dependency graph, live sandbox preview, and triage buttons', async () => {
  assert.equal(mountCodeXray, mountCodeXrayWorkbench);
  const doc = createMiniDom();
  const container = doc.createElement('div');
  const storage = createMemoryStorage();
  const catalog = createSampleCatalog();

  const api = mountCodeXray(container, {
    initialCatalog: catalog,
    storage,
    fetch: async () => ({ ok: false }),
  });

  const text = container.textContentDeep();
  assert.match(text, /Project X-Ray & Ecosystem Census/);
  assert.match(text, /Visual Dependency & Consumer Graph/);
  assert.match(text, /Human-Approved Triage/);

  // Select DevStudio tool and mount live sandbox iframe
  api.selectItem('editor-tool:AudioCueMixer');
  const mountBtns = container.findAll((el) => el.getAttribute('data-testid') === 'mount-live-sandbox-btn');
  assert.equal(mountBtns.length, 1);
  mountBtns[0].dispatch('click');

  const iframes = container.findAll((el) => el.tagName === 'IFRAME');
  assert.equal(iframes.length, 1);
  assert.match(iframes[0].getAttribute('src'), /\/__studio-xray-preview\?item=editor-tool%3AAudioCueMixer/);

  // Click Merge triage button
  const mergeBtn = container.findAll((el) => el.getAttribute('data-triage-action') === 'Merge')[0];
  assert.ok(mergeBtn);
  await api.setItemTriage('editor-tool:AudioCueMixer', 'Merge', 'Migrate to Studio audio-daw');

  const exportedMd = api.exportMarkdown();
  assert.match(exportedMd, /Migrate to Studio audio-daw/);

  api.destroy();
});
