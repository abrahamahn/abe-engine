// Ganbate Studio — Ecosystem Census & Project X-Ray Workbench (`editor/code-xray.js`)
// Visualizes every codebase & asset surface across `ganbate-main` + `ganbate-engine` with
// SHA-256 provenance, glTF mesh/morph stats, static consumer/dependency graphs, state-authority
// tags, dual Implementation vs Integration completeness %, live 3D/image/audio/sandbox previews,
// and non-destructive human-approved triage (`Keep`, `Merge`, `Wire`, `Refactor`, `Archive`, `Delete`).

import {
  drawGlbPreviewCanvas,
  drawWaveformCanvas,
  extractWavPeaks,
  parseGlbPreview,
  parseGlbPreviewAsync,
} from './asset-previews.js';

export const DEFAULT_CATALOG_URLS = Object.freeze([
  '/api/project/xray-catalog',
  '/ganbate-studio/code-surface-catalog.json',
  './code-surface-catalog.json',
]);

export const DECISIONS_STORAGE_KEY = 'ganbate.studio.codeXrayDecisions.v2';
export const DECISIONS_DOC_PATH = 'editor/data/code-xray-decisions.json';
export const DECISIONS_SCHEMA = 'ganbate.code-xray-decisions/2';

export const OFFICIAL_STATUSES = Object.freeze([
  'canonical',
  'active_alternative',
  'experimental',
  'legacy',
  'deprecated',
  'orphaned',
  'unresolved',
]);

export const SURFACE_BUCKETS = Object.freeze([
  'live',
  'hidden',
  'duplicate',
  'dev-studio-migration',
  'orphan',
]);

export const RUNTIME_EXPOSURES = Object.freeze([
  'live_production',
  'capability_gated',
  'editor_only',
  'unwired_code',
  'worktree_only',
]);

export const TRIAGE_ACTIONS = Object.freeze([
  'Keep',
  'Update',
  'Merge',
  'Wire',
  'Refactor',
  'Archive',
  'Delete',
]);

const TRIAGE_ACTION_META = Object.freeze({
  Keep: { icon: '✓', label: 'Keep' },
  Update: { icon: '✨', label: 'Update (Algo/Bug/Docs)' },
  Merge: { icon: '⇄', label: 'Merge' },
  Wire: { icon: '⚡', label: 'Wire' },
  Refactor: { icon: '🛠', label: 'Refactor' },
  Archive: { icon: '📦', label: 'Archive' },
  Delete: { icon: '🗑', label: 'Delete' },
});

const BUCKET_LABELS = Object.freeze({
  live: '🟢 Live',
  hidden: '🟡 Hidden',
  duplicate: '🟠 Duplicate',
  'dev-studio-migration': '🔵 DevStudio Mig.',
  orphan: '🔴 Orphan',
});

export function normalizeTriageAction(raw) {
  if (!raw || typeof raw !== 'string') return null;
  const trimmed = raw.trim();
  if (TRIAGE_ACTIONS.includes(trimmed)) return trimmed;
  const lower = trimmed.toLowerCase();
  if (lower === 'keep' || lower === 'keep-canonical') return 'Keep';
  if (lower === 'update' || lower.startsWith('update-') || lower === 'improve' || lower === 'fix') return 'Update';
  if (lower === 'merge' || lower === 'merge-into-studio') return 'Merge';
  if (lower === 'wire' || lower === 'wire-to-frontend') return 'Wire';
  if (lower === 'refactor' || lower === 're-architect') return 'Refactor';
  if (lower === 'archive') return 'Archive';
  if (lower === 'delete' || lower === 'mark-for-deletion') return 'Delete';
  return null;
}

export function formatBytes(bytes) {
  const n = Number(bytes || 0);
  if (n >= 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(2)} MB`;
  if (n >= 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${n} B`;
}

/**
 * Synthesizes a minimal live catalog from `studioApi.listAssets()` when running in an offline
 * scratch project before `code-surface-catalog.json` has been generated.
 */
export function buildFallbackCatalogFromStudioAssets(assets = []) {
  const items = (Array.isArray(assets) ? assets : []).map((a) => {
    const p = String(a.path || a.id || 'asset');
    const lower = p.toLowerCase();
    const category = lower.endsWith('.glb')
      ? '3d-model'
      : lower.endsWith('.seq.json')
        ? 'animation-sequence'
        : lower.endsWith('.vfx.json')
          ? 'vfx-emitter'
          : lower.endsWith('.character.json')
            ? 'character-model'
            : lower.endsWith('.soundbank.json') || /\.(?:ogg|mp3|wav)$/i.test(lower)
              ? 'audio-bank'
              : /\.(?:png|webp|avif|svg|jpg|jpeg)$/i.test(lower)
                ? 'image-asset'
                : 'ui-component';
    return {
      id: `studio-asset:${a.id || p}`,
      title: p.split('/').pop() || p,
      category,
      repo: 'ganbate-engine',
      sourcePath: p,
      repoPath: p,
      owningSubsystem: 'engine-studio-project',
      fileSize: Number(a.bytes || 0),
      loc: 0,
      sha256: a.sha256 || null,
      provenance: {
        sha256: a.sha256 || null,
        shortHash: a.sha256 ? String(a.sha256).slice(0, 12) : null,
        identicalHashPeers: [],
        forkGroup: null,
        existsOnMainBranch: true,
      },
      referenceCount: Number(a.usages || 0),
      referencedBy: [],
      consumers: [],
      dependencies: [],
      editorExposure: 'asset-library-only',
      authoritySource: { kind: 'json-manifest', ref: p },
      candidateCanonicalId: `studio-asset:${a.id || p}`,
      status: 'canonical',
      surfaceBucket: 'live',
      runtimeExposure: 'editor_only',
      statusReason: 'Discovered directly from active Studio host asset index.',
      gateFlag: null,
      routePath: null,
      implementationPct: 85,
      integrationPct: 75,
      metricsBreakdown: {
        implementationSignals: ['studio-asset-indexed'],
        integrationSignals: ['studio-host-mounted'],
      },
      recommendedAction: 'Keep',
      preview: {
        kind: lower.endsWith('.glb')
          ? 'glb'
          : /\.(?:ogg|mp3|wav)$/i.test(lower)
            ? 'audio'
            : /\.(?:png|webp|avif|svg|jpg|jpeg)$/i.test(lower)
              ? 'image'
              : 'code-summary',
        assetUrl: null,
        sandboxKey: p,
      },
      glbStats: null,
      docSummary: null,
      clusterIds: [],
    };
  });

  return {
    schema: 'ganbate.code-xray-catalog/2',
    generatedAt: new Date().toISOString(),
    repos: {
      mainRootLabel: 'ganbate-main',
      engineRootLabel: 'ganbate-engine',
      scannedNonTestSourceFiles: items.length,
    },
    summary: {
      totalItems: items.length,
      clusterCount: 0,
      byStatus: Object.fromEntries(OFFICIAL_STATUSES.map((s) => [s, s === 'canonical' ? items.length : 0])),
      bySurfaceBucket: Object.fromEntries(SURFACE_BUCKETS.map((b) => [b, b === 'live' ? items.length : 0])),
      byRuntimeExposure: Object.fromEntries(RUNTIME_EXPOSURES.map((r) => [r, r === 'editor_only' ? items.length : 0])),
      byCategory: {},
      totalDependencyEdges: 0,
      identicalDuplicateGroupCount: 0,
      avgImplementationPct: items.length ? 85 : 0,
      avgIntegrationPct: items.length ? 75 : 0,
      totalFileBytes: items.reduce((s, i) => s + i.fileSize, 0),
      totalMB: Number((items.reduce((s, i) => s + i.fileSize, 0) / (1024 * 1024)).toFixed(2)),
      totalLoc: 0,
      reclaimableBytes: 0,
      reclaimableMB: 0,
      reclaimableLoc: 0,
      hiddenFeatureCount: 0,
    },
    clusters: [],
    items,
  };
}

/**
 * Loads the Ecosystem Census & Project X-Ray catalog trying the canonical URL fallback order:
 * `['/api/project/xray-catalog', '/ganbate-studio/code-surface-catalog.json', './code-surface-catalog.json']`.
 */
export async function loadXrayCatalog({
  fetchImpl = globalThis.fetch,
  urls = DEFAULT_CATALOG_URLS,
  studioApi = null,
} = {}) {
  const tried = [];
  if (typeof fetchImpl === 'function') {
    for (const url of urls) {
      tried.push(url);
      try {
        const res = await fetchImpl(url, { headers: { Accept: 'application/json' } });
        if (!res || !res.ok) continue;
        const json = await res.json();
        if (json && Array.isArray(json.items)) {
          return { catalog: json, sourceUrl: url, triedUrls: tried };
        }
      } catch {
        // try next URL in chain
      }
    }
  }
  const studioAssets = studioApi?.listAssets?.() || [];
  return {
    catalog: buildFallbackCatalogFromStudioAssets(studioAssets),
    sourceUrl: 'studio-assets-fallback',
    triedUrls: tried,
  };
}

/**
 * Computes real-time KPI totals across official statuses, surface buckets, runtime exposure,
 * completeness averages, and human-approved triage progress.
 */
export function computeCatalogKpis(catalog, decisions = {}) {
  const items = Array.isArray(catalog?.items) ? catalog.items : [];
  const clusters = Array.isArray(catalog?.clusters) ? catalog.clusters : [];

  const byStatus = Object.fromEntries(OFFICIAL_STATUSES.map((s) => [s, 0]));
  const bySurfaceBucket = Object.fromEntries(SURFACE_BUCKETS.map((b) => [b, 0]));
  const byRuntimeExposure = Object.fromEntries(RUNTIME_EXPOSURES.map((r) => [r, 0]));
  const byTriage = Object.fromEntries(TRIAGE_ACTIONS.map((a) => [a, 0]));

  let totalBytes = 0;
  let totalLoc = 0;
  let reclaimableBytes = 0;
  let reclaimableLoc = 0;
  let implSum = 0;
  let integSum = 0;
  let decidedCount = 0;
  let dependencyEdges = 0;
  const identicalHashes = new Set();

  for (const item of items) {
    const st = item.status || 'canonical';
    const bucket = item.surfaceBucket || 'live';
    const exp = item.runtimeExposure || 'live_production';

    byStatus[st] = (byStatus[st] || 0) + 1;
    bySurfaceBucket[bucket] = (bySurfaceBucket[bucket] || 0) + 1;
    byRuntimeExposure[exp] = (byRuntimeExposure[exp] || 0) + 1;

    totalBytes += Number(item.fileSize || 0);
    totalLoc += Number(item.loc || 0);
    implSum += Number(item.implementationPct || 0);
    integSum += Number(item.integrationPct || 0);
    dependencyEdges += Array.isArray(item.dependencies) ? item.dependencies.length : 0;

    if (item.provenance?.identicalHashPeers?.length > 0 && item.sha256) {
      identicalHashes.add(item.sha256);
    }

    const decision = decisions?.[item.id];
    const triage = normalizeTriageAction(decision?.action);
    if (triage) {
      decidedCount += 1;
      byTriage[triage] = (byTriage[triage] || 0) + 1;
    }

    const markedReclaim =
      triage === 'Delete' ||
      triage === 'Archive' ||
      triage === 'Merge' ||
      (!triage &&
        (bucket === 'duplicate' || bucket === 'orphan' || bucket === 'dev-studio-migration'));
    if (markedReclaim) {
      reclaimableBytes += Number(item.fileSize || 0);
      reclaimableLoc += Number(item.loc || 0);
    }
  }

  return {
    totalItems: items.length,
    clusterCount: clusters.length,
    dependencyEdges,
    identicalHashGroups: identicalHashes.size,
    byStatus,
    bySurfaceBucket,
    byRuntimeExposure,
    byTriage,
    decidedCount,
    undecidedCount: Math.max(0, items.length - decidedCount),
    avgImplementationPct: items.length ? Math.round(implSum / items.length) : 0,
    avgIntegrationPct: items.length ? Math.round(integSum / items.length) : 0,
    totalBytes,
    totalMB: Number((totalBytes / (1024 * 1024)).toFixed(2)),
    totalLoc,
    reclaimableBytes,
    reclaimableMB: Number((reclaimableBytes / (1024 * 1024)).toFixed(2)),
    reclaimableLoc,
  };
}

/**
 * Filters catalog items deterministically by status, surfaceBucket, runtimeExposure, category,
 * subsystem, clusterId, human triage decision, and free-text query (including SHA-256 prefix & authority ref).
 */
export function filterCatalogItems(items = [], filters = {}, decisions = {}) {
  const {
    status = 'all',
    surfaceBucket = 'all',
    runtimeExposure = 'all',
    category = 'all',
    subsystem = 'all',
    clusterId = 'all',
    layer = 'all',
    riskSignal = 'all',
    triage = 'all',
    query = '',
  } = filters;

  const q = String(query || '').trim().toLowerCase();

  return items.filter((item) => {
    if (status !== 'all' && item.status !== status) return false;
    if (surfaceBucket !== 'all' && item.surfaceBucket !== surfaceBucket) return false;
    if (runtimeExposure !== 'all' && item.runtimeExposure !== runtimeExposure) return false;
    if (category !== 'all' && item.category !== category) return false;
    if (subsystem !== 'all' && item.owningSubsystem !== subsystem) return false;
    if (clusterId !== 'all' && !(item.clusterIds || []).includes(clusterId)) return false;
    if (layer !== 'all') {
      const itemLayer = item.codeIntel?.layer || null;
      if (layer === 'any-code') {
        if (!itemLayer && !isSourceCodePath(item.repoPath)) return false;
      } else if (itemLayer !== layer) {
        return false;
      }
    }
    if (riskSignal !== 'all') {
      const risks = item.codeIntel?.riskSignals || [];
      if (riskSignal === 'any-risk') {
        if (!risks.length) return false;
      } else if (!risks.includes(riskSignal)) {
        return false;
      }
    }

    if (triage !== 'all') {
      const itemTriage = normalizeTriageAction(decisions?.[item.id]?.action);
      if (triage === 'undecided') {
        if (itemTriage !== null) return false;
      } else if (itemTriage !== triage) {
        return false;
      }
    }

    if (q) {
      const hay = [
        item.id,
        item.title,
        item.repoPath,
        item.owningSubsystem,
        item.status,
        item.surfaceBucket,
        item.runtimeExposure,
        item.sha256 || '',
        item.gateFlag || '',
        item.routePath || '',
        item.authoritySource?.kind || '',
        item.authoritySource?.ref || '',
        item.statusReason || '',
        item.codeIntel?.layer || '',
        item.codeIntel?.lang || '',
        ...(item.codeIntel?.symbols || []),
        ...(item.codeIntel?.httpRoutes || []),
        ...(item.codeIntel?.sqlTables || []),
        ...(item.codeIntel?.riskSignals || []),
      ]
        .join(' ')
        .toLowerCase();
      if (!hay.includes(q)) return false;
    }

    return true;
  });
}

export function isSourceCodePath(repoPath = '') {
  return /\.(?:rs|sql|ts|tsx|js|mjs|cjs|wgsl|toml|sh|py|css|html)$/i.test(String(repoPath || ''));
}

/**
 * Builds the Visual Dependency & Consumer Graph neighborhood around a selected item:
 * `consumers (inbound)` -> `selected item` -> `dependencies (outbound)` + `canonical target` + `hash peers`.
 */
export function buildDependencyNeighborhood(catalog, itemId) {
  const items = Array.isArray(catalog?.items) ? catalog.items : [];
  const byId = new Map(items.map((i) => [i.id, i]));
  const center = byId.get(itemId) || null;
  if (!center) return null;

  const mapEdge = (edge) => {
    const linked = edge.itemId ? byId.get(edge.itemId) || null : null;
    return {
      itemId: linked ? linked.id : edge.itemId || null,
      label: linked ? linked.title : (edge.path || '').split('/').pop() || edge.path,
      repo: edge.repo || linked?.repo || 'ganbate-main',
      path: edge.path || linked?.repoPath || '',
      kind: edge.kind || 'ref',
      status: linked?.status || null,
    };
  };

  const consumers = (center.consumers || []).map(mapEdge);
  const dependencies = (center.dependencies || []).map(mapEdge);
  const canonicalTarget =
    center.candidateCanonicalId && center.candidateCanonicalId !== center.id
      ? byId.get(center.candidateCanonicalId) || null
      : null;
  const identicalPeers = (center.provenance?.identicalHashPeers || [])
    .map((pid) => byId.get(pid))
    .filter(Boolean);

  return {
    center,
    consumers,
    dependencies,
    canonicalTarget,
    identicalPeers,
  };
}

/**
 * Compares 3D `.glb` models in a cluster side-by-side, computing delta vs canonical and proving
 * byte-identical copies vs modified forks via SHA-256.
 */
export function compareGlbClusterItems(catalog, clusterIdOrItems) {
  const allItems = Array.isArray(catalog?.items) ? catalog.items : [];
  let members = [];
  let canonicalId = null;

  if (typeof clusterIdOrItems === 'string') {
    const cl = (catalog?.clusters || []).find((c) => c.id === clusterIdOrItems);
    canonicalId = cl?.canonicalItemId || null;
    const set = new Set(cl?.itemIds || []);
    members = allItems.filter((i) => set.has(i.id));
  } else if (Array.isArray(clusterIdOrItems)) {
    members = clusterIdOrItems;
  }

  const glbItems = members.filter((m) => m.glbStats || String(m.repoPath || '').endsWith('.glb'));
  const canonical =
    (canonicalId && glbItems.find((g) => g.id === canonicalId)) ||
    glbItems.find((g) => g.status === 'canonical') ||
    glbItems[0] ||
    null;

  return glbItems.map((it) => {
    const isCanonical = canonical && it.id === canonical.id;
    const isByteIdentical =
      !isCanonical &&
      Boolean(canonical?.sha256 && it.sha256 && canonical.sha256 === it.sha256);
    const provenanceRelation = isCanonical
      ? 'CANONICAL'
      : isByteIdentical
        ? 'BYTE-IDENTICAL COPY'
        : 'MODIFIED FORK';

    const tris = Number(it.glbStats?.triangles || 0);
    const baseTris = Number(canonical?.glbStats?.triangles || 0);
    return {
      id: it.id,
      title: it.title,
      repoPath: it.repoPath,
      status: it.status,
      fileSize: it.fileSize,
      deltaBytes: canonical ? it.fileSize - canonical.fileSize : 0,
      sha256: it.sha256 || null,
      shortHash: it.provenance?.shortHash || (it.sha256 ? it.sha256.slice(0, 12) : 'n/a'),
      provenanceRelation,
      meshes: Number(it.glbStats?.meshes || 0),
      vertices: Number(it.glbStats?.vertices || 0),
      triangles: tris,
      deltaTriangles: canonical ? tris - baseTris : 0,
      materials: Number(it.glbStats?.materials || 0),
      bones: Number(it.glbStats?.bones || 0),
      morphTargets: Number(it.glbStats?.morphTargets || 0),
      animations: Number(it.glbStats?.animations || 0),
    };
  });
}

// ---- Non-Destructive Human Triage Persistence & Exports ---------------------------------------

export function loadTriageDecisionsFromStorage(storage = globalThis.localStorage) {
  if (!storage || typeof storage.getItem !== 'function') return {};
  try {
    const raw = storage.getItem(DECISIONS_STORAGE_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw);
    const entries = parsed?.decisions && typeof parsed.decisions === 'object' ? parsed.decisions : parsed;
    const clean = {};
    for (const [id, val] of Object.entries(entries || {})) {
      if (!val || typeof val !== 'object') continue;
      const action = normalizeTriageAction(val.action);
      if (action || val.notes) {
        clean[id] = {
          action,
          notes: String(val.notes || ''),
          updatedAt: val.updatedAt || new Date().toISOString(),
        };
      }
    }
    return clean;
  } catch {
    return {};
  }
}

export function saveTriageDecisionsToStorage(decisions, storage = globalThis.localStorage) {
  const payload = {
    schema: DECISIONS_SCHEMA,
    zeroAutoDelete: true,
    updatedAt: new Date().toISOString(),
    decisions,
  };
  if (storage && typeof storage.setItem === 'function') {
    try {
      storage.setItem(DECISIONS_STORAGE_KEY, JSON.stringify(payload));
    } catch {
      // ignore quota errors
    }
  }
  return payload;
}

/**
 * Saves human-approved triage decisions non-destructively to the Studio host (`editor/data/code-xray-decisions.json`)
 * or `/api/project/xray-decisions` without ever deleting any file on disk.
 */
export async function persistTriageDecisionsToStudio(
  decisions,
  studioApi = null,
  fetchImpl = globalThis.fetch,
) {
  const payload = {
    schema: DECISIONS_SCHEMA,
    zeroAutoDelete: true,
    updatedAt: new Date().toISOString(),
    decisions,
  };

  if (studioApi) {
    try {
      const existing = studioApi.getDocument?.(DECISIONS_DOC_PATH);
      if (existing && typeof studioApi.edit === 'function') {
        await studioApi.edit(
          'Update Code X-Ray Triage Ledger',
          [{ set: ['decisions'], value: decisions }, { set: ['updatedAt'], value: payload.updatedAt }],
          'code-xray-triage',
          existing.id || DECISIONS_DOC_PATH,
        );
        await studioApi.save?.(false, existing.id || DECISIONS_DOC_PATH);
        return { saved: true, target: 'studio-edit', path: DECISIONS_DOC_PATH };
      }
      if (typeof studioApi.createDocument === 'function') {
        await studioApi.createDocument('data', DECISIONS_DOC_PATH, payload);
        await studioApi.save?.(false, DECISIONS_DOC_PATH);
        return { saved: true, target: 'studio-create', path: DECISIONS_DOC_PATH };
      }
    } catch {
      // fall through to optional HTTP endpoint
    }
  }

  if (typeof fetchImpl === 'function') {
    try {
      const res = await fetchImpl('/api/project/xray-decisions', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
      });
      if (res?.ok) {
        return { saved: true, target: 'http-endpoint', path: '/api/project/xray-decisions' };
      }
    } catch {
      // ignore offline
    }
  }

  return { saved: false, target: 'localStorage-only', path: DECISIONS_DOC_PATH };
}

export function exportTriageLedgerJson(catalog, decisions = {}) {
  const itemsById = new Map((catalog?.items || []).map((i) => [i.id, i]));
  const records = [];
  for (const [itemId, dec] of Object.entries(decisions)) {
    const action = normalizeTriageAction(dec?.action);
    if (!action && !dec?.notes) continue;
    const item = itemsById.get(itemId);
    records.push({
      itemId,
      action,
      notes: String(dec?.notes || ''),
      updatedAt: dec?.updatedAt || null,
      repo: item?.repo || null,
      sourcePath: item?.repoPath || null,
      sha256: item?.sha256 || null,
      fileSize: item?.fileSize ?? null,
      loc: item?.loc ?? null,
      status: item?.status || null,
      candidateCanonicalId: item?.candidateCanonicalId || null,
    });
  }
  return JSON.stringify(
    {
      schema: DECISIONS_SCHEMA,
      zeroAutoDelete: true,
      exportedAt: new Date().toISOString(),
      totalDecisions: records.length,
      records,
    },
    null,
    2,
  );
}

export function exportTriageActionPlanMarkdown(catalog, decisions = {}) {
  const kpis = computeCatalogKpis(catalog, decisions);
  const itemsById = new Map((catalog?.items || []).map((i) => [i.id, i]));
  const lines = [
    '# Ganbate Studio — Project X-Ray Human-Approved Action Plan',
    '',
    `> Non-destructive triage ledger (\`zeroAutoDelete: true\`). Scanned **${kpis.totalItems}** surfaces across \`ganbate-main\` + \`ganbate-engine\`.`,
    `> Reclaimable footprint: **${kpis.reclaimableMB} MB** / **${kpis.reclaimableLoc.toLocaleString('en-US')} LOC** | Human decisions recorded: **${kpis.decidedCount}**.`,
    '',
  ];

  for (const action of TRIAGE_ACTIONS) {
    const matching = Object.entries(decisions)
      .filter(([, d]) => normalizeTriageAction(d?.action) === action)
      .map(([id, d]) => ({ id, dec: d, item: itemsById.get(id) }));
    if (!matching.length) continue;

    const meta = TRIAGE_ACTION_META[action];
    lines.push(`## ${meta.icon} ${action} (${matching.length})`);
    lines.push('');
    for (const { id, dec, item } of matching) {
      const pathLabel = item ? `${item.repo}:${item.repoPath}` : id;
      const hashLabel = item?.provenance?.shortHash ? ` · sha256:\`${item.provenance.shortHash}\`` : '';
      const sizeLabel = item ? ` · ${formatBytes(item.fileSize)} / ${item.loc} LOC` : '';
      const canonLabel =
        item?.candidateCanonicalId && item.candidateCanonicalId !== item.id
          ? ` → canonical: \`${item.candidateCanonicalId}\``
          : '';
      const noteLabel = dec.notes ? ` — _${dec.notes}_` : '';
      lines.push(`- **${item?.title || id}** (\`${pathLabel}\`${hashLabel}${sizeLabel})${canonLabel}${noteLabel}`);
    }
    lines.push('');
  }

  return lines.join('\n');
}

// ---- DOM Workbench Mount (`mountCodeXray`) ----------------------------------------------------

function resolveAssetPreviewUrl(item, studioApi) {
  if (item?.preview?.assetUrl) return item.preview.assetUrl;
  const p = item?.repoPath || '';
  if (p.startsWith('main/apps/web/public/')) {
    return p.slice('main/apps/web/public'.length);
  }
  if (typeof studioApi?.assetUrl === 'function' && item?.repo === 'ganbate-engine') {
    return studioApi.assetUrl(p);
  }
  return `/api/project/file?repo=${encodeURIComponent(item?.repo || 'ganbate-main')}&path=${encodeURIComponent(p)}`;
}

function ensureStylesheet(doc) {
  if (!doc?.head || typeof doc.createElement !== 'function') return;
  try {
    const existing = doc.querySelector?.('link[data-ganbate-code-xray-css="1"]');
    if (existing) return;
    const link = doc.createElement('link');
    link.setAttribute('rel', 'stylesheet');
    link.setAttribute('data-ganbate-code-xray-css', '1');
    link.setAttribute('href', new URL('./code-xray.css', import.meta.url).href);
    doc.head.append(link);
  } catch {
    // ignore in synthetic test DOMs
  }
}

function triggerFileDownload(doc, filename, content, mimeType) {
  if (typeof Blob === 'undefined' || typeof URL?.createObjectURL !== 'function' || !doc?.createElement) {
    return false;
  }
  const blob = new Blob([content], { type: mimeType });
  const url = URL.createObjectURL(blob);
  const a = doc.createElement('a');
  a.setAttribute('href', url);
  a.setAttribute('download', filename);
  a.click?.();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
  return true;
}

export function mountCodeXray(container, studioApi = {}) {
  if (!container) throw new Error('mountCodeXray requires a container element');
  const doc = container.ownerDocument || globalThis.document;
  ensureStylesheet(doc);

  const h = (tag, attrs = {}, ...children) => {
    const node = doc.createElement(tag);
    for (const [k, v] of Object.entries(attrs)) {
      if (v === undefined || v === null || v === false) continue;
      if (k.startsWith('on') && typeof v === 'function') {
        node.addEventListener(k.slice(2).toLowerCase(), v);
      } else if (k === 'class') {
        node.className = v;
      } else if (k === 'value' && 'value' in node) {
        node.value = String(v);
      } else {
        node.setAttribute(k, v === true ? '' : String(v));
      }
    }
    for (const child of children.flat(Infinity)) {
      if (child === undefined || child === null || child === false) continue;
      if (typeof child === 'string' || typeof child === 'number') {
        node.append(doc.createTextNode ? doc.createTextNode(String(child)) : String(child));
      } else {
        node.append(child);
      }
    }
    return node;
  };

  const storage = studioApi.storage || globalThis.localStorage;
  const fetchImpl = studioApi.fetch || globalThis.fetch;

  const state = {
    catalog: studioApi.initialCatalog || studioApi.catalog || null,
    sourceUrl: studioApi.initialCatalog ? 'injected' : null,
    loading: !studioApi.initialCatalog && !studioApi.catalog,
    viewMode: 'clusters', // 'clusters' | 'bookshelf' | 'codetree' | 'audit'
    selectedClusterId: null,
    selectedItemId: null,
    sandboxLiveMounted: false,
    imageZoomed: false,
    orbitYaw: 0.42,
    orbitPitch: 0.18,
    orbitZoom: 1.0,
    orbitPanX: 0,
    orbitPanY: 0,
    glbShading: 'textured', // 'textured' | 'clay' | 'wireframe' | 'normals'
    glbFocus: 'body', // 'body' | 'bust'
    expandedDirs: new Set(['ganbate-main', 'ganbate-engine', 'ganbate-main/main', 'ganbate-main/main/backend', 'ganbate-main/main/database', 'ganbate-main/main/apps', 'ganbate-engine/src', 'ganbate-engine/crates']),
    auditSort: 'priority', // 'priority' | 'loc' | 'callers' | 'path'
    auditOnlyUndecided: false,
    filters: {
      status: 'all',
      surfaceBucket: 'all',
      runtimeExposure: 'all',
      category: 'all',
      subsystem: 'all',
      layer: 'all',
      riskSignal: 'all',
      triage: 'all',
      query: '',
    },
    decisions: loadTriageDecisionsFromStorage(storage),
    glbCache: new Map(),
    glbInflight: new Map(),
    glbQueue: [],
    glbActiveLoads: 0,
    bookshelfLimit: 120,
    sourceCache: new Map(),
    sourceInflight: new Set(),
    saveNotice: '',
    disposed: false,
  };

  const root = h('div', { class: 'cxr-root', 'data-testid': 'code-xray-workbench' });
  container.innerHTML = '';
  container.append(root);

  function ensureDefaultSelection() {
    if (!state.catalog) return;
    const clusters = state.catalog.clusters || [];
    if (!state.selectedClusterId && clusters.length > 0) {
      state.selectedClusterId = clusters[0].id;
    }
    if (!state.selectedItemId) {
      const activeCluster = clusters.find((c) => c.id === state.selectedClusterId);
      if (activeCluster?.itemIds?.length) {
        state.selectedItemId = activeCluster.canonicalItemId || activeCluster.itemIds[0];
      } else if (state.catalog.items?.length) {
        state.selectedItemId = state.catalog.items[0].id;
      }
    }
  }

  function currentGlbDrawOpts(width, height, isMini = false) {
    return {
      width,
      height,
      yaw: state.orbitYaw,
      pitch: state.orbitPitch,
      zoom: isMini ? 1.0 : state.orbitZoom,
      panX: isMini ? 0 : state.orbitPanX,
      panY: isMini ? 0 : state.orbitPanY,
      shading: state.glbShading,
      focus: isMini ? 'body' : state.glbFocus,
    };
  }

  function acquireGlbSlot() {
    if (state.glbActiveLoads < 2) {
      state.glbActiveLoads++;
      return Promise.resolve();
    }
    return new Promise((resolve) => state.glbQueue.push(resolve));
  }

  function releaseGlbSlot() {
    const next = state.glbQueue.shift();
    if (next) next();
    else state.glbActiveLoads = Math.max(0, state.glbActiveLoads - 1);
  }

  async function loadGlbGeometryForCanvas(item, canvas, width, height, isMini = false) {
    if (!item || !canvas || typeof fetchImpl !== 'function') return;
    const cacheKey = item.id;
    if (state.glbCache.has(cacheKey)) {
      drawGlbPreviewCanvas(canvas, state.glbCache.get(cacheKey), currentGlbDrawOpts(width, height, isMini));
      return;
    }
    drawGlbPreviewCanvas(canvas, null, currentGlbDrawOpts(width, height, isMini));

    let loadPromise = state.glbInflight.get(cacheKey);
    if (!loadPromise) {
      loadPromise = (async () => {
        await acquireGlbSlot();
        try {
          const primaryUrl = resolveAssetPreviewUrl(item, studioApi);
          const fallbackUrl = `/api/project/file?repo=${encodeURIComponent(item?.repo || 'ganbate-main')}&path=${encodeURIComponent(item?.repoPath || '')}`;
          let res = await fetchImpl(primaryUrl);
          if ((!res || !res.ok) && primaryUrl !== fallbackUrl) {
            res = await fetchImpl(fallbackUrl);
          }
          if (!res?.ok) return null;
          const buf = await res.arrayBuffer();
          await new Promise((r) => setTimeout(r, 0));
          const maxFullTris = isMini ? 85000 : 165000;
          const parsed =
            (await parseGlbPreviewAsync(buf, { fetchImpl, maxFullTris })) ||
            parseGlbPreview(buf, { maxFullTris });
          if (parsed && !state.disposed) {
            state.glbCache.set(cacheKey, parsed);
          }
          return parsed;
        } catch {
          return null;
        } finally {
          releaseGlbSlot();
          state.glbInflight.delete(cacheKey);
        }
      })();
      state.glbInflight.set(cacheKey, loadPromise);
    }

    const parsed = await loadPromise;
    if (parsed && !state.disposed && canvas.isConnected !== false) {
      drawGlbPreviewCanvas(canvas, parsed, currentGlbDrawOpts(width, height, isMini));
    }
  }

  function bindOrbitDrag(canvas, item, width, height, isMini = false) {
    let dragging = false;
    let panMode = false;
    let lastX = 0;
    let lastY = 0;
    canvas.addEventListener('contextmenu', (ev) => ev.preventDefault?.());
    canvas.addEventListener('pointerdown', (ev) => {
      dragging = true;
      panMode = !isMini && (ev.button === 2 || ev.button === 1 || Boolean(ev.shiftKey));
      lastX = ev.clientX || 0;
      lastY = ev.clientY || 0;
      canvas.setPointerCapture?.(ev.pointerId);
      ev.stopPropagation?.();
    });
    canvas.addEventListener('pointermove', (ev) => {
      if (!dragging) return;
      const dx = (ev.clientX || 0) - lastX;
      const dy = (ev.clientY || 0) - lastY;
      lastX = ev.clientX || 0;
      lastY = ev.clientY || 0;
      if (panMode) {
        state.orbitPanX += dx;
        state.orbitPanY += dy;
      } else {
        state.orbitYaw += dx * 0.012;
        state.orbitPitch = Math.max(-1.35, Math.min(1.35, state.orbitPitch + dy * 0.01));
      }
      const info = state.glbCache.get(item.id);
      if (info) {
        drawGlbPreviewCanvas(canvas, info, currentGlbDrawOpts(width, height, isMini));
      }
    });
    canvas.addEventListener('pointerup', () => {
      dragging = false;
    });
    if (!isMini) {
      canvas.addEventListener(
        'wheel',
        (ev) => {
          ev.preventDefault?.();
          const factor = (ev.deltaY || 0) < 0 ? 1.14 : 1 / 1.14;
          state.orbitZoom = Math.max(0.25, Math.min(10, state.orbitZoom * factor));
          const info = state.glbCache.get(item.id);
          if (info) {
            drawGlbPreviewCanvas(canvas, info, currentGlbDrawOpts(width, height, false));
          }
        },
        { passive: false },
      );
    }
  }

  function ensureSourceLoaded(item) {
    if (!item || typeof fetchImpl !== 'function') return;
    if (!isSourceCodePath(item.repoPath) && item.preview?.kind !== 'source-code' && !item.codeIntel) return;
    if (state.sourceCache.has(item.id) || state.sourceInflight.has(item.id)) return;
    state.sourceInflight.add(item.id);
    const url = `/api/project/file?repo=${encodeURIComponent(item.repo || 'ganbate-main')}&path=${encodeURIComponent(item.repoPath || '')}`;
    fetchImpl(url)
      .then(async (res) => {
        if (!res?.ok) throw new Error(`HTTP ${res?.status || 'err'}`);
        const text = await res.text();
        if (!state.disposed) {
          state.sourceCache.set(item.id, { ok: true, text });
          if (state.selectedItemId === item.id) render();
        }
      })
      .catch((err) => {
        if (!state.disposed) {
          state.sourceCache.set(item.id, { ok: false, text: `Unable to load live file preview (${err?.message || 'offline'}).` });
          if (state.selectedItemId === item.id) render();
        }
      })
      .finally(() => {
        state.sourceInflight.delete(item.id);
      });
  }

  async function setItemTriage(itemId, action, notesInput) {
    const prev = state.decisions[itemId] || { action: null, notes: '' };
    const nextAction = action === undefined ? prev.action : normalizeTriageAction(action);
    const nextNotes = notesInput === undefined ? prev.notes : String(notesInput);
    if (!nextAction && !nextNotes) {
      delete state.decisions[itemId];
    } else {
      state.decisions[itemId] = {
        action: nextAction,
        notes: nextNotes,
        updatedAt: new Date().toISOString(),
      };
    }
    saveTriageDecisionsToStorage(state.decisions, storage);
    const res = await persistTriageDecisionsToStudio(state.decisions, studioApi, fetchImpl);
    state.saveNotice = `Saved (${res.target})`;
    render();
  }

  function selectItem(itemId) {
    if (!itemId) return;
    state.selectedItemId = itemId;
    state.sandboxLiveMounted = false;
    state.imageZoomed = false;
    render();
  }

  function renderDualBars(implPct, integPct) {
    return h(
      'div',
      { class: 'cxr-dual-bars' },
      h(
        'div',
        { class: 'cxr-bar-row' },
        h('span', {}, 'Impl. %'),
        h('div', { class: 'cxr-bar-track' }, h('div', { class: 'cxr-bar-fill-impl', style: `width:${implPct}%` })),
        h('strong', {}, `${implPct}%`),
      ),
      h(
        'div',
        { class: 'cxr-bar-row' },
        h('span', {}, 'Integ. %'),
        h('div', { class: 'cxr-bar-track' }, h('div', { class: 'cxr-bar-fill-integ', style: `width:${integPct}%` })),
        h('strong', {}, `${integPct}%`),
      ),
    );
  }

  function renderBookCard(item, { showTurntable = false } = {}) {
    const isSelected = state.selectedItemId === item.id;
    const dec = state.decisions[item.id];
    const triage = normalizeTriageAction(dec?.action);
    const hasIdenticalPeer = (item.provenance?.identicalHashPeers?.length || 0) > 0;

    let turntableEl = null;
    if (showTurntable && (item.glbStats || String(item.repoPath || '').endsWith('.glb'))) {
      turntableEl = h('canvas', { class: 'cxr-mini-turntable', width: 256, height: 164 });
      bindOrbitDrag(turntableEl, item, 256, 164, true);
      void loadGlbGeometryForCanvas(item, turntableEl, 256, 164, true);
    }

    return h(
      'article',
      {
        class: `cxr-book-card${isSelected ? ' is-selected' : ''}`,
        'data-item-id': item.id,
        onClick: () => selectItem(item.id),
      },
      h(
        'div',
        { class: 'cxr-card-top' },
        h('span', { class: `cxr-badge cxr-status-${item.status}` }, item.status),
        h(
          'span',
          { class: `cxr-badge cxr-bucket-${item.surfaceBucket}` },
          BUCKET_LABELS[item.surfaceBucket] || item.surfaceBucket,
        ),
        item.runtimeExposure === 'worktree_only'
          ? h('span', { class: 'cxr-badge cxr-status-experimental' }, 'worktree_only')
          : null,
        triage
          ? h('span', { class: 'cxr-badge cxr-status-active_alternative' }, `${TRIAGE_ACTION_META[triage].icon} ${triage}`)
          : null,
      ),
      turntableEl,
      h('div', { class: 'cxr-card-title' }, item.title),
      h('div', { class: 'cxr-card-path' }, `${item.repo}:${item.repoPath}`),
      h(
        'div',
        { class: 'cxr-card-top' },
        item.provenance?.shortHash
          ? h(
              'span',
              { class: `cxr-hash-badge${hasIdenticalPeer ? ' is-identical' : ''}` },
              hasIdenticalPeer ? `≡ ${item.provenance.shortHash} (COPY)` : `#${item.provenance.shortHash}`,
            )
          : null,
        h(
          'span',
          { class: 'cxr-hash-badge' },
          `${item.authoritySource?.kind || 'source'}`,
        ),
      ),
      h(
        'div',
        { class: 'cxr-cluster-tab-meta' },
        h('span', {}, formatBytes(item.fileSize)),
        item.loc > 0 ? h('span', {}, `${item.loc} LOC`) : null,
        h('span', {}, `${item.referenceCount} refs`),
        item.glbStats ? h('span', {}, `${item.glbStats.triangles.toLocaleString('en-US')} tris`) : null,
      ),
      renderDualBars(item.implementationPct ?? 0, item.integrationPct ?? 0),
    );
  }

  function renderClusterShelf() {
    const clusters = state.catalog?.clusters || [];
    const itemsById = new Map((state.catalog?.items || []).map((i) => [i.id, i]));
    const activeCluster = clusters.find((c) => c.id === state.selectedClusterId) || clusters[0];

    const tabs = h(
      'div',
      { class: 'cxr-cluster-tabs' },
      clusters.map((cl) =>
        h(
          'button',
          {
            type: 'button',
            class: `cxr-cluster-tab${cl.id === activeCluster?.id ? ' is-selected' : ''}`,
            'data-cluster-id': cl.id,
            onClick: () => {
              state.selectedClusterId = cl.id;
              if (cl.itemIds?.length) {
                state.selectedItemId = cl.canonicalItemId || cl.itemIds[0];
              }
              render();
            },
          },
          h('div', { class: 'cxr-cluster-tab-title' }, cl.title),
          h(
            'div',
            { class: 'cxr-cluster-tab-meta' },
            h('span', {}, `${cl.itemCount} items`),
            h('span', {}, `Reclaim ${formatBytes(cl.reclaimableBytes)}`),
            cl.reclaimableLoc > 0 ? h('span', {}, `${cl.reclaimableLoc.toLocaleString('en-US')} LOC`) : null,
            h('span', {}, `Impl ${cl.implementationPct ?? 0}% / Integ ${cl.integrationPct ?? 0}%`),
          ),
        ),
      ),
    );

    if (!activeCluster) {
      return h('div', {}, tabs, h('p', {}, 'No clusters detected in current catalog.'));
    }

    const clusterItems = filterCatalogItems(
      activeCluster.itemIds.map((id) => itemsById.get(id)).filter(Boolean),
      { ...state.filters, clusterId: 'all' },
      state.decisions,
    );

    const is3dCluster =
      activeCluster.kind === '3d-turntable-compare' ||
      clusterItems.some((i) => i.glbStats);

    const pairsSection =
      Array.isArray(activeCluster.pairs) && activeCluster.pairs.length > 0
        ? h(
            'div',
            { class: 'cxr-pair-grid' },
            activeCluster.pairs.map((pair) =>
              h(
                'div',
                { class: 'cxr-pair-card' },
                h('div', { class: 'cxr-pair-role' }, pair.role),
                h(
                  'div',
                  { class: 'cxr-pair-columns' },
                  h(
                    'div',
                    { class: 'cxr-dep-col' },
                    pair.legacyItemIds.map((lid) => {
                      const it = itemsById.get(lid);
                      return it
                        ? h(
                            'button',
                            {
                              type: 'button',
                              class: 'cxr-dep-node',
                              onClick: () => selectItem(it.id),
                            },
                            `🔵 ${it.title}`,
                          )
                        : null;
                    }),
                  ),
                  h('span', { class: 'cxr-pair-arrow' }, '⇄'),
                  h(
                    'div',
                    { class: 'cxr-dep-col' },
                    pair.targetItemIds.map((tid) => {
                      const it = itemsById.get(tid);
                      return it
                        ? h(
                            'button',
                            {
                              type: 'button',
                              class: 'cxr-dep-node',
                              onClick: () => selectItem(it.id),
                            },
                            `🟢 ${it.title}`,
                          )
                        : null;
                    }),
                  ),
                ),
              ),
            ),
          )
        : null;

    return h(
      'div',
      { style: 'display:flex;flex-direction:column;gap:12px;' },
      tabs,
      h(
        'div',
        { class: 'cxr-cluster-banner' },
        h(
          'div',
          { class: 'cxr-card-top' },
          h('strong', { style: 'font-size:13px;color:#fff;' }, activeCluster.title),
          h(
            'span',
            { class: 'cxr-badge cxr-status-deprecated' },
            `Reclaimable: ${formatBytes(activeCluster.reclaimableBytes)} · ${activeCluster.reclaimableLoc.toLocaleString('en-US')} LOC`,
          ),
        ),
        h('p', { class: 'cxr-cluster-summary' }, activeCluster.summary),
        renderDualBars(activeCluster.implementationPct ?? 0, activeCluster.integrationPct ?? 0),
      ),
      pairsSection,
      h(
        'div',
        { class: 'cxr-card-grid' },
        clusterItems.map((it, idx) =>
          renderBookCard(it, {
            showTurntable:
              is3dCluster &&
              (state.glbCache.has(it.id) || (idx < 6 && (it.fileSize || 0) <= 20 * 1024 * 1024)),
          }),
        ),
      ),
    );
  }

  function renderInspector(item) {
    if (!item) {
      return h('div', { class: 'cxr-section' }, 'Select any surface or asset card to inspect.');
    }

    const neighborhood = buildDependencyNeighborhood(state.catalog, item.id);
    const dec = state.decisions[item.id] || { action: null, notes: '' };
    const currentTriage = normalizeTriageAction(dec.action);
    const previewUrl = resolveAssetPreviewUrl(item, studioApi);

    // 1. Live Preview Stage
    let previewContent = null;
    const pKind = item.preview?.kind || 'code-summary';
    if (pKind === 'glb' || item.glbStats) {
      const stageW = 480;
      const stageH = 310;
      const canvas = h('canvas', { class: 'cxr-preview-canvas', width: stageW, height: stageH });
      bindOrbitDrag(canvas, item, stageW, stageH, false);
      void loadGlbGeometryForCanvas(item, canvas, stageW, stageH, false);
      const cachedGlb = state.glbCache.get(item.id);
      const submeshList = cachedGlb?.submeshes?.length
        ? h(
            'details',
            { class: 'cxr-submesh-details' },
            h(
              'summary',
              {},
              `Mesh Parts & Materials (${cachedGlb.submeshes.length} submeshes · ${cachedGlb.triangles.toLocaleString('en-US')} tris${cachedGlb.usesDraco ? ' · Draco decoded' : ''})`,
            ),
            h(
              'div',
              { class: 'cxr-submesh-list' },
              cachedGlb.submeshes.slice(0, 48).map((sm) =>
                h(
                  'div',
                  { class: 'cxr-submesh-row' },
                  h('span', { class: 'cxr-submesh-name' }, sm.name),
                  h('span', { class: 'cxr-subtitle' }, `${sm.materialName} · ${sm.triCount.toLocaleString('en-US')} tris`),
                ),
              ),
            ),
          )
        : null;
      previewContent = h(
        'div',
        { class: 'cxr-preview-stage' },
        h(
          'div',
          { class: 'cxr-glb-toolbar' },
          ['textured', 'clay', 'wireframe', 'normals'].map((mode) =>
            h(
              'button',
              {
                type: 'button',
                class: `cxr-pill${state.glbShading === mode ? ' is-active' : ''}`,
                onClick: () => {
                  state.glbShading = mode;
                  render();
                },
              },
              mode === 'textured'
                ? 'Textured'
                : mode === 'clay'
                  ? 'Clay Sculpt'
                  : mode === 'wireframe'
                    ? 'Wire + Shaded'
                    : 'Normals',
            ),
          ),
          h('span', { style: 'width:6px;' }),
          ['body', 'bust'].map((foc) =>
            h(
              'button',
              {
                type: 'button',
                class: `cxr-pill${state.glbFocus === foc ? ' is-active' : ''}`,
                onClick: () => {
                  state.glbFocus = foc;
                  state.orbitPanX = 0;
                  state.orbitPanY = 0;
                  render();
                },
              },
              foc === 'body' ? 'Full Frame' : 'Bust / Detail',
            ),
          ),
          h(
            'button',
            {
              type: 'button',
              class: 'cxr-pill',
              onClick: () => {
                state.orbitYaw = 0.42;
                state.orbitPitch = 0.18;
                state.orbitZoom = 1.0;
                state.orbitPanX = 0;
                state.orbitPanY = 0;
                render();
              },
            },
            'Reset View',
          ),
        ),
        canvas,
        h(
          'div',
          { class: 'cxr-header-actions' },
          h(
            'span',
            { class: 'cxr-subtitle' },
            'Left-drag: Orbit · Right/Shift-drag: Pan · Scroll: Zoom · WebGL2 full-mesh + diffuse textures',
          ),
          studioApi.activateWorkbench
            ? h(
                'button',
                {
                  type: 'button',
                  class: 'cxr-btn',
                  onClick: () => studioApi.activateWorkbench('model'),
                },
                'Open in Model Studio',
              )
            : null,
        ),
        submeshList,
      );
    } else if (pKind === 'image') {
      const img = h('img', {
        class: `cxr-preview-img${state.imageZoomed ? ' is-zoomed' : ''}`,
        src: previewUrl,
        alt: item.title,
      });
      previewContent = h(
        'div',
        { class: 'cxr-preview-stage' },
        img,
        h(
          'button',
          {
            type: 'button',
            class: 'cxr-btn',
            onClick: () => {
              state.imageZoomed = !state.imageZoomed;
              render();
            },
          },
          state.imageZoomed ? 'Reset Zoom (1x)' : 'Zoom Sheet (1.85x)',
        ),
      );
    } else if (pKind === 'audio') {
      const waveCanvas = h('canvas', { width: 320, height: 76, style: 'width:100%;max-width:340px;height:76px;border-radius:4px;' });
      if (typeof fetchImpl === 'function' && String(previewUrl).endsWith('.wav')) {
        fetchImpl(previewUrl)
          .then((r) => (r?.ok ? r.arrayBuffer() : null))
          .then((buf) => {
            if (!buf || state.disposed) return;
            const wave = extractWavPeaks(buf, 44);
            if (wave) drawWaveformCanvas(waveCanvas, wave, { width: 320, height: 76 });
          })
          .catch(() => {});
      }
      previewContent = h(
        'div',
        { class: 'cxr-preview-stage' },
        waveCanvas,
        h('audio', { controls: true, src: previewUrl, style: 'width:100%;max-width:340px;' }),
      );
    } else if (pKind === 'sandbox-page' || pKind === 'sandbox-widget' || pKind === 'iframe-review') {
      const iframeSrc =
        pKind === 'iframe-review' && item.preview?.assetUrl
          ? item.preview.assetUrl
          : `/__studio-xray-preview?item=${encodeURIComponent(item.id)}`;
      previewContent = h(
        'div',
        { class: 'cxr-preview-stage' },
        state.sandboxLiveMounted
          ? h('iframe', {
              class: 'cxr-sandbox-iframe',
              src: iframeSrc,
              title: `Live Sandbox Preview: ${item.title}`,
            })
          : h(
              'div',
              { style: 'text-align:center;display:flex;flex-direction:column;gap:8px;align-items:center;' },
              h('div', { style: 'color:#c6d0e3;' }, `Live Sandbox Target: ${iframeSrc}`),
              h(
                'button',
                {
                  type: 'button',
                  class: 'cxr-btn cxr-btn-primary',
                  'data-testid': 'mount-live-sandbox-btn',
                  onClick: () => {
                    state.sandboxLiveMounted = true;
                    render();
                  },
                },
                '▶ Mount Live Interactive Preview',
              ),
            ),
      );
    } else if (pKind === 'json-doc' && item.docSummary) {
      previewContent = h(
        'div',
        { class: 'cxr-preview-stage' },
        h(
          'div',
          { class: 'cxr-kv-grid', style: 'width:100%;' },
          Object.entries(item.docSummary).map(([k, v]) =>
            h(
              'div',
              { class: 'cxr-kv-item' },
              h('div', { class: 'cxr-kv-key' }, k),
              h('div', { class: 'cxr-kv-val' }, Array.isArray(v) ? v.join(', ') : String(v)),
            ),
          ),
        ),
        studioApi.openDocument
          ? h(
              'button',
              {
                type: 'button',
                class: 'cxr-btn cxr-btn-primary',
                onClick: () => studioApi.openDocument(item.repoPath),
              },
              'Open Authored Document in Studio',
            )
          : null,
      );
    }

    // glTF 2.0 Mesh / Rig Breakdown if `.glb`
    const glbStatsSection = item.glbStats
      ? h(
          'div',
          { class: 'cxr-kv-grid' },
          h('div', { class: 'cxr-kv-item' }, h('div', { class: 'cxr-kv-key' }, 'Triangles'), h('div', { class: 'cxr-kv-val' }, item.glbStats.triangles.toLocaleString('en-US'))),
          h('div', { class: 'cxr-kv-item' }, h('div', { class: 'cxr-kv-key' }, 'Vertices'), h('div', { class: 'cxr-kv-val' }, item.glbStats.vertices.toLocaleString('en-US'))),
          h('div', { class: 'cxr-kv-item' }, h('div', { class: 'cxr-kv-key' }, 'Meshes / Prims'), h('div', { class: 'cxr-kv-val' }, `${item.glbStats.meshes} / ${item.glbStats.primitives}`)),
          h('div', { class: 'cxr-kv-item' }, h('div', { class: 'cxr-kv-key' }, 'Bones / Morphs'), h('div', { class: 'cxr-kv-val' }, `${item.glbStats.bones} / ${item.glbStats.morphTargets}`)),
          h('div', { class: 'cxr-kv-item' }, h('div', { class: 'cxr-kv-key' }, 'Materials / Tex'), h('div', { class: 'cxr-kv-val' }, `${item.glbStats.materials} / ${item.glbStats.textures}`)),
          h('div', { class: 'cxr-kv-item' }, h('div', { class: 'cxr-kv-key' }, 'Animations'), h('div', { class: 'cxr-kv-val' }, String(item.glbStats.animations))),
        )
      : null;

    // Provenance & State Authority Card
    const provenanceSection = h(
      'section',
      { class: 'cxr-section' },
      h('h3', { class: 'cxr-section-title' }, 'Provenance, SHA-256 & State Authority'),
      h(
        'div',
        { class: 'cxr-kv-grid' },
        h('div', { class: 'cxr-kv-item' }, h('div', { class: 'cxr-kv-key' }, 'Official Status'), h('div', { class: 'cxr-kv-val' }, item.status)),
        h('div', { class: 'cxr-kv-item' }, h('div', { class: 'cxr-kv-key' }, 'Runtime Exposure'), h('div', { class: 'cxr-kv-val' }, `${item.runtimeExposure} (${item.surfaceBucket})`)),
        h('div', { class: 'cxr-kv-item' }, h('div', { class: 'cxr-kv-key' }, 'Owning Subsystem'), h('div', { class: 'cxr-kv-val' }, item.owningSubsystem || 'n/a')),
        h('div', { class: 'cxr-kv-item' }, h('div', { class: 'cxr-kv-key' }, 'State Authority'), h('div', { class: 'cxr-kv-val' }, `${item.authoritySource?.kind || 'source'} · ${item.authoritySource?.ref || ''}`)),
        h('div', { class: 'cxr-kv-item' }, h('div', { class: 'cxr-kv-key' }, 'Footprint'), h('div', { class: 'cxr-kv-val' }, `${formatBytes(item.fileSize)} · ${item.loc} LOC`)),
        h('div', { class: 'cxr-kv-item' }, h('div', { class: 'cxr-kv-key' }, 'SHA-256 Fingerprint'), h('div', { class: 'cxr-kv-val', style: 'font-family:monospace;font-size:10px;' }, item.sha256 || 'n/a')),
      ),
      neighborhood?.identicalPeers?.length
        ? h(
            'div',
            { class: 'cxr-badge cxr-status-experimental' },
            `Byte-Identical SHA-256 Copy Of: ${neighborhood.identicalPeers.map((p) => p.title).join(', ')}`,
          )
        : null,
      h('div', { class: 'cxr-subtitle' }, item.statusReason),
      glbStatsSection,
    );

    // Dual Completeness Bars
    const completenessSection = h(
      'section',
      { class: 'cxr-section' },
      h('h3', { class: 'cxr-section-title' }, 'Implementation vs Integration Completeness'),
      renderDualBars(item.implementationPct ?? 0, item.integrationPct ?? 0),
      item.metricsBreakdown
        ? h(
            'div',
            { class: 'cxr-subtitle' },
            `Impl: ${(item.metricsBreakdown.implementationSignals || []).join(', ')} | Integ: ${(item.metricsBreakdown.integrationSignals || []).join(', ')}`,
          )
        : null,
    );

    // Visual Dependency & Consumer Graph
    const graphSection = h(
      'section',
      { class: 'cxr-section', 'data-testid': 'dependency-graph-inspector' },
      h('h3', { class: 'cxr-section-title' }, 'Visual Dependency & Consumer Graph'),
      h(
        'div',
        { class: 'cxr-dep-graph' },
        h(
          'div',
          { class: 'cxr-dep-col' },
          h('div', { class: 'cxr-dep-col-header' }, `Inbound Consumers (${neighborhood?.consumers.length || 0})`),
          (neighborhood?.consumers || []).length === 0
            ? h('span', { class: 'cxr-subtitle' }, '0 inbound non-test refs')
            : neighborhood.consumers.slice(0, 10).map((c) =>
                h(
                  'button',
                  {
                    type: 'button',
                    class: 'cxr-dep-node',
                    title: `${c.repo}:${c.path} (${c.kind})`,
                    onClick: () => c.itemId && selectItem(c.itemId),
                  },
                  `→ ${c.label}`,
                ),
              ),
        ),
        h(
          'div',
          { class: 'cxr-dep-center' },
          h('strong', { style: 'font-size:11px;color:#fff;' }, item.title),
          h('span', { class: `cxr-badge cxr-status-${item.status}` }, item.status),
          neighborhood?.canonicalTarget
            ? h(
                'button',
                {
                  type: 'button',
                  class: 'cxr-dep-node',
                  onClick: () => selectItem(neighborhood.canonicalTarget.id),
                },
                `★ Canonical: ${neighborhood.canonicalTarget.title}`,
              )
            : null,
        ),
        h(
          'div',
          { class: 'cxr-dep-col' },
          h('div', { class: 'cxr-dep-col-header' }, `Outbound Dependencies (${neighborhood?.dependencies.length || 0})`),
          (neighborhood?.dependencies || []).length === 0
            ? h('span', { class: 'cxr-subtitle' }, '0 resolved internal deps')
            : neighborhood.dependencies.slice(0, 10).map((d) =>
                h(
                  'button',
                  {
                    type: 'button',
                    class: 'cxr-dep-node',
                    title: `${d.repo}:${d.path} (${d.kind})`,
                    onClick: () => d.itemId && selectItem(d.itemId),
                  },
                  `⇢ ${d.label}`,
                ),
              ),
        ),
      ),
    );

    // Human-Approved Non-Destructive Triage Controls
    const notesBox = h('textarea', {
      class: 'cxr-notes',
      value: dec.notes || '',
      onChange: (ev) => {
        void setItemTriage(item.id, currentTriage, ev.target?.value ?? '');
      },
    });
    if ('value' in notesBox) notesBox.value = dec.notes || '';

    const triageSection = h(
      'section',
      { class: 'cxr-section', 'data-testid': 'triage-controls' },
      h(
        'div',
        { class: 'cxr-card-top' },
        h('h3', { class: 'cxr-section-title' }, 'Human-Approved Triage (Non-Destructive)'),
        state.saveNotice ? h('span', { class: 'cxr-subtitle' }, state.saveNotice) : null,
      ),
      h(
        'div',
        { class: 'cxr-triage-buttons' },
        TRIAGE_ACTIONS.map((act) => {
          const meta = TRIAGE_ACTION_META[act];
          return h(
            'button',
            {
              type: 'button',
              class: `cxr-triage-btn${currentTriage === act ? ' is-selected' : ''}`,
              'data-triage-action': act,
              onClick: () => {
                void setItemTriage(
                  item.id,
                  currentTriage === act ? null : act,
                  notesBox.value ?? dec.notes,
                );
              },
            },
            `${meta.icon} ${meta.label}`,
          );
        }),
      ),
      notesBox,
    );

    // Live Source Code & Code Intelligence Section (Backend Rust / SQL / Protocol / Frontend / Engine)
    const isCodeItem = Boolean(item.codeIntel || isSourceCodePath(item.repoPath) || pKind === 'source-code');
    let codeIntelSection = null;
    if (isCodeItem) {
      ensureSourceLoaded(item);
      const cachedSource = state.sourceCache.get(item.id);
      const ci = item.codeIntel || {};
      const lines = cachedSource?.ok && typeof cachedSource.text === 'string'
        ? cachedSource.text.split('\n')
        : null;
      const maxPreviewLines = 260;

      codeIntelSection = h(
        'section',
        { class: 'cxr-section', 'data-testid': 'source-code-inspector' },
        h(
          'div',
          { class: 'cxr-card-top' },
          h('h3', { class: 'cxr-section-title' }, 'Source Code & Static Audit Intelligence'),
          ci.layer ? h('span', { class: 'cxr-badge cxr-status-active_alternative' }, `${ci.layer} · ${ci.lang || 'code'}`) : null,
        ),
        (ci.riskSignals || []).length > 0
          ? h(
              'div',
              { class: 'cxr-pill-bar' },
              ci.riskSignals.map((sig) =>
                h('span', { class: 'cxr-badge cxr-status-deprecated' }, `⚠ ${sig}`),
              ),
            )
          : h('div', { class: 'cxr-subtitle' }, 'No static risk flags triggered on this file.'),
        (ci.httpRoutes?.length || ci.sqlTables?.length || ci.symbols?.length)
          ? h(
              'div',
              { class: 'cxr-kv-grid' },
              ci.httpRoutes?.length
                ? h('div', { class: 'cxr-kv-item' }, h('div', { class: 'cxr-kv-key' }, 'HTTP Endpoints'), h('div', { class: 'cxr-kv-val' }, ci.httpRoutes.slice(0, 6).join(', ')))
                : null,
              ci.sqlTables?.length
                ? h('div', { class: 'cxr-kv-item' }, h('div', { class: 'cxr-kv-key' }, 'SQL Tables'), h('div', { class: 'cxr-kv-val' }, ci.sqlTables.slice(0, 6).join(', ')))
                : null,
              ci.symbols?.length
                ? h('div', { class: 'cxr-kv-item' }, h('div', { class: 'cxr-kv-key' }, 'Key Symbols'), h('div', { class: 'cxr-kv-val' }, ci.symbols.slice(0, 10).join(', ')))
                : null,
            )
          : null,
        lines
          ? h(
              'pre',
              { class: 'cxr-code-viewer' },
              lines
                .slice(0, maxPreviewLines)
                .map((ln, idx) => `${String(idx + 1).padStart(4, ' ')} │ ${ln}`)
                .join('\n') +
                (lines.length > maxPreviewLines
                  ? `\n... (${lines.length - maxPreviewLines} more lines — ${lines.length} total lines)`
                  : ''),
            )
          : h(
              'div',
              { class: 'cxr-subtitle' },
              cachedSource ? cachedSource.text : 'Loading source file from workspace...',
            ),
      );
    }

    return h(
      'div',
      { style: 'display:flex;flex-direction:column;gap:10px;' },
      h(
        'section',
        { class: 'cxr-section' },
        h('div', { style: 'font-size:13px;font-weight:700;color:#fff;' }, item.title),
        h('div', { class: 'cxr-card-path' }, `${item.repo}:${item.repoPath}`),
        previewContent,
      ),
      triageSection,
      codeIntelSection,
      provenanceSection,
      completenessSection,
      graphSection,
    );
  }

  function renderCodeTreeShelf(filteredItems) {
    const codeItems = filteredItems.filter((i) => i.codeIntel || isSourceCodePath(i.repoPath));
    const layers = ['all', ...new Set(codeItems.map((i) => i.codeIntel?.layer).filter(Boolean))];
    const riskOptions = [
      ['all', 'All Code Files'],
      ['any-risk', '⚠ Any Risk Signal'],
      ['over-1000-loc', '> 1,000 LOC Monoliths'],
      ['zero-inbound-imports', '0 Inbound Callers'],
      ['uncalled-http-route', 'Uncalled Backend Route'],
      ['unused-sql-table', 'Unused SQL Table'],
      ['high-unwrap-or-any', 'High unwrap / any'],
      ['missing-module-docs', 'Missing Module Docs'],
    ];

    // Group into folder hierarchy (repo/top1/top2)
    const groups = new Map();
    for (const item of codeItems) {
      const parts = String(item.repoPath || '').split('/').filter(Boolean);
      const groupKey = `${item.repo}/${parts.slice(0, Math.min(3, Math.max(1, parts.length - 1))).join('/')}`;
      if (!groups.has(groupKey)) {
        groups.set(groupKey, { key: groupKey, items: [], totalLoc: 0 });
      }
      const g = groups.get(groupKey);
      g.items.push(item);
      g.totalLoc += Number(item.loc || 0);
    }
    const sortedGroups = [...groups.values()].sort((a, b) => b.items.length - a.items.length || a.key.localeCompare(b.key));

    return h(
      'div',
      { style: 'display:flex;flex-direction:column;gap:10px;', 'data-testid': 'code-tree-shelf' },
      h(
        'div',
        { class: 'cxr-cluster-banner' },
        h(
          'div',
          { class: 'cxr-card-top' },
          h('strong', { style: 'color:#fff;font-size:13px;' }, `Full Repository Code Explorer (${codeItems.length.toLocaleString('en-US')} files)`),
          h(
            'select',
            {
              class: 'cxr-select',
              value: state.filters.riskSignal,
              onChange: (ev) => {
                state.filters.riskSignal = ev.target?.value || 'all';
                render();
              },
            },
            riskOptions.map(([val, lbl]) => h('option', { value: val }, lbl)),
          ),
        ),
        h(
          'div',
          { class: 'cxr-pill-bar' },
          layers.map((lyr) =>
            h(
              'button',
              {
                type: 'button',
                class: `cxr-pill${state.filters.layer === lyr ? ' is-active' : ''}`,
                onClick: () => {
                  state.filters.layer = lyr;
                  render();
                },
              },
              lyr === 'all' ? 'All Architecture Layers' : lyr,
            ),
          ),
        ),
      ),
      sortedGroups.slice(0, 60).map((grp) => {
        const open = state.expandedDirs.has(grp.key) || sortedGroups.length <= 8 || Boolean(state.filters.query.trim());
        return h(
          'div',
          { class: 'cxr-tree-group' },
          h(
            'button',
            {
              type: 'button',
              class: 'cxr-tree-folder',
              onClick: () => {
                if (state.expandedDirs.has(grp.key)) state.expandedDirs.delete(grp.key);
                else state.expandedDirs.add(grp.key);
                render();
              },
            },
            h('span', {}, `${open ? '▾' : '▸'} 📁 ${grp.key}`),
            h('span', { class: 'cxr-subtitle' }, `${grp.items.length} files · ${grp.totalLoc.toLocaleString('en-US')} LOC`),
          ),
          open
            ? h(
                'div',
                { class: 'cxr-tree-files' },
                grp.items.slice(0, 120).map((it) => {
                  const tri = normalizeTriageAction(state.decisions[it.id]?.action);
                  const risks = it.codeIntel?.riskSignals || [];
                  return h(
                    'button',
                    {
                      type: 'button',
                      class: `cxr-tree-file${state.selectedItemId === it.id ? ' is-selected' : ''}`,
                      onClick: () => selectItem(it.id),
                    },
                    h('span', { class: 'cxr-tree-filename' }, it.repoPath.split('/').slice(3).join('/') || it.title),
                    h(
                      'span',
                      { class: 'cxr-cluster-tab-meta' },
                      risks.length > 0 ? h('span', { class: 'cxr-badge cxr-status-deprecated' }, `⚠ ${risks.length}`) : null,
                      tri ? h('span', { class: 'cxr-badge cxr-status-active_alternative' }, `${TRIAGE_ACTION_META[tri].icon} ${tri}`) : null,
                      h('span', {}, `${it.loc} LOC`),
                      h('span', {}, `${it.referenceCount} refs`),
                    ),
                  );
                }),
              )
            : null,
        );
      }),
    );
  }

  function renderAuditQueueShelf(filteredItems) {
    let queue = filteredItems.filter((i) => {
      if (!state.auditOnlyUndecided) return true;
      return !normalizeTriageAction(state.decisions[i.id]?.action);
    });

    const riskScore = (it) => {
      const risks = it.codeIntel?.riskSignals || [];
      let s = risks.length * 25;
      if (it.surfaceBucket === 'orphan') s += 40;
      if (it.surfaceBucket === 'duplicate') s += 30;
      if (Number(it.loc || 0) > 1000) s += 35;
      return s + Math.min(20, Math.floor(Number(it.loc || 0) / 100));
    };

    queue = [...queue].sort((a, b) => {
      if (state.auditSort === 'loc') return Number(b.loc || 0) - Number(a.loc || 0);
      if (state.auditSort === 'callers') return Number(a.referenceCount || 0) - Number(b.referenceCount || 0);
      if (state.auditSort === 'path') return String(a.repoPath).localeCompare(String(b.repoPath));
      return riskScore(b) - riskScore(a);
    });

    const curIdx = queue.findIndex((i) => i.id === state.selectedItemId);
    const stepBy = (delta) => {
      if (!queue.length) return;
      const nextIdx = Math.max(0, Math.min(queue.length - 1, (curIdx < 0 ? 0 : curIdx) + delta));
      selectItem(queue[nextIdx].id);
    };

    return h(
      'div',
      { style: 'display:flex;flex-direction:column;gap:10px;', 'data-testid': 'audit-queue-shelf' },
      h(
        'div',
        { class: 'cxr-cluster-banner' },
        h(
          'div',
          { class: 'cxr-card-top' },
          h('strong', { style: 'color:#fff;font-size:13px;' }, `File-by-File AI & Founder Audit Queue (${queue.length.toLocaleString('en-US')} items)`),
          h(
            'div',
            { class: 'cxr-header-actions' },
            h('button', { type: 'button', class: 'cxr-btn', onClick: () => stepBy(-1) }, '← Prev File'),
            h('span', { class: 'cxr-subtitle' }, curIdx >= 0 ? `${curIdx + 1} / ${queue.length}` : `0 / ${queue.length}`),
            h('button', { type: 'button', class: 'cxr-btn cxr-btn-primary', onClick: () => stepBy(1) }, 'Next File →'),
          ),
        ),
        h(
          'div',
          { class: 'cxr-card-top' },
          h(
            'label',
            { class: 'cxr-subtitle', style: 'display:inline-flex;align-items:center;gap:5px;cursor:pointer;' },
            h('input', {
              type: 'checkbox',
              checked: state.auditOnlyUndecided ? 'checked' : null,
              onChange: (ev) => {
                state.auditOnlyUndecided = Boolean(ev.target?.checked);
                render();
              },
            }),
            'Show undecided files only',
          ),
          h(
            'select',
            {
              class: 'cxr-select',
              value: state.auditSort,
              onChange: (ev) => {
                state.auditSort = ev.target?.value || 'priority';
                render();
              },
            },
            h('option', { value: 'priority' }, 'Sort: Highest Audit Priority First'),
            h('option', { value: 'loc' }, 'Sort: Largest LOC First (>1,000 LOC Monoliths)'),
            h('option', { value: 'callers' }, 'Sort: 0 Inbound Callers First'),
            h('option', { value: 'path' }, 'Sort: Alphabetical Path'),
          ),
        ),
      ),
      h(
        'div',
        { class: 'cxr-audit-list' },
        queue.slice(0, 160).map((it) => {
          const curAct = normalizeTriageAction(state.decisions[it.id]?.action);
          const risks = it.codeIntel?.riskSignals || [];
          return h(
            'div',
            {
              class: `cxr-audit-row${state.selectedItemId === it.id ? ' is-selected' : ''}`,
              onClick: () => selectItem(it.id),
            },
            h(
              'div',
              { class: 'cxr-audit-main' },
              h('div', { class: 'cxr-card-title' }, it.title),
              h('div', { class: 'cxr-card-path' }, `${it.repo}:${it.repoPath}`),
              h(
                'div',
                { class: 'cxr-cluster-tab-meta' },
                it.codeIntel?.layer ? h('span', { class: 'cxr-hash-badge' }, it.codeIntel.layer) : null,
                h('span', {}, `${it.loc || 0} LOC`),
                h('span', {}, `${it.referenceCount || 0} callers`),
                ...risks.slice(0, 3).map((r) => h('span', { class: 'cxr-badge cxr-status-deprecated' }, r)),
              ),
            ),
            h(
              'div',
              { class: 'cxr-audit-quick-actions' },
              ['Keep', 'Update', 'Merge', 'Delete'].map((act) =>
                h(
                  'button',
                  {
                    type: 'button',
                    class: `cxr-pill${curAct === act ? ' is-active' : ''}`,
                    onClick: (ev) => {
                      ev.stopPropagation?.();
                      void setItemTriage(it.id, curAct === act ? null : act);
                    },
                  },
                  `${TRIAGE_ACTION_META[act].icon} ${act}`,
                ),
              ),
            ),
          );
        }),
      ),
    );
  }

  function render() {
    if (state.disposed) return;
    root.innerHTML = '';

    if (state.loading || !state.catalog) {
      root.append(
        h(
          'div',
          { style: 'padding:24px;color:#9aa5ba;' },
          'Scanning Ecosystem Census & Project X-Ray catalog...',
        ),
      );
      return;
    }

    ensureDefaultSelection();
    const kpis = computeCatalogKpis(state.catalog, state.decisions);
    const allItems = state.catalog.items || [];
    const filteredItems = filterCatalogItems(allItems, state.filters, state.decisions);
    const selectedItem =
      allItems.find((i) => i.id === state.selectedItemId) || filteredItems[0] || allItems[0] || null;

    const header = h(
      'header',
      { class: 'cxr-header' },
      h(
        'div',
        { class: 'cxr-title-row' },
        h(
          'div',
          { class: 'cxr-title-group' },
          h('h2', { class: 'cxr-title' }, 'Project X-Ray & Ecosystem Census'),
          h(
            'span',
            { class: 'cxr-subtitle' },
            `${kpis.totalItems} surfaces · ${kpis.dependencyEdges} graph edges · ${kpis.identicalHashGroups} SHA-256 identical groups · source: ${state.sourceUrl || 'catalog'}`,
          ),
        ),
        h(
          'div',
          { class: 'cxr-header-actions' },
          h(
            'button',
            {
              type: 'button',
              class: 'cxr-btn',
              'data-testid': 'export-json-ledger-btn',
              onClick: () => {
                const json = exportTriageLedgerJson(state.catalog, state.decisions);
                triggerFileDownload(doc, 'code-xray-decisions.json', json, 'application/json');
              },
            },
            '⬇ Export JSON Ledger',
          ),
          h(
            'button',
            {
              type: 'button',
              class: 'cxr-btn cxr-btn-primary',
              'data-testid': 'export-markdown-plan-btn',
              onClick: () => {
                const md = exportTriageActionPlanMarkdown(state.catalog, state.decisions);
                triggerFileDownload(doc, 'code-xray-action-plan.md', md, 'text/markdown');
              },
            },
            '📋 Export Markdown Cut/Wire Plan',
          ),
        ),
      ),
      h(
        'div',
        { class: 'cxr-kpi-strip' },
        h('div', { class: 'cxr-kpi-card' }, h('span', { class: 'cxr-kpi-label' }, 'Total Scanned'), h('span', { class: 'cxr-kpi-value' }, String(kpis.totalItems)), h('span', { class: 'cxr-kpi-sub' }, `${kpis.totalMB} MB · ${kpis.totalLoc.toLocaleString('en-US')} LOC`)),
        h('div', { class: 'cxr-kpi-card' }, h('span', { class: 'cxr-kpi-label' }, '🟢 Live / Canonical'), h('span', { class: 'cxr-kpi-value' }, `${kpis.bySurfaceBucket.live} / ${kpis.byStatus.canonical}`), h('span', { class: 'cxr-kpi-sub' }, `${kpis.byStatus.active_alternative} active alt`)),
        h('div', { class: 'cxr-kpi-card' }, h('span', { class: 'cxr-kpi-label' }, '🟡 Hidden / Gated'), h('span', { class: 'cxr-kpi-value' }, String(kpis.bySurfaceBucket.hidden)), h('span', { class: 'cxr-kpi-sub' }, `${kpis.byRuntimeExposure.worktree_only} worktree_only`)),
        h('div', { class: 'cxr-kpi-card' }, h('span', { class: 'cxr-kpi-label' }, '🟠 Duplicate / Legacy'), h('span', { class: 'cxr-kpi-value' }, `${kpis.bySurfaceBucket.duplicate} (${kpis.clusterCount} clusters)`), h('span', { class: 'cxr-kpi-sub' }, `${kpis.byStatus.deprecated} deprecated`)),
        h('div', { class: 'cxr-kpi-card' }, h('span', { class: 'cxr-kpi-label' }, '🔵 DevStudio → Studio'), h('span', { class: 'cxr-kpi-value' }, String(kpis.bySurfaceBucket['dev-studio-migration'])), h('span', { class: 'cxr-kpi-sub' }, `${kpis.byStatus.orphaned} 🔴 orphaned`)),
        h('div', { class: 'cxr-kpi-card' }, h('span', { class: 'cxr-kpi-label' }, 'Reclaimable Footprint'), h('span', { class: 'cxr-kpi-value' }, `${kpis.reclaimableMB} MB`), h('span', { class: 'cxr-kpi-sub' }, `${kpis.reclaimableLoc.toLocaleString('en-US')} LOC · ${kpis.decidedCount} triaged`)),
        h('div', { class: 'cxr-kpi-card' }, h('span', { class: 'cxr-kpi-label' }, 'Avg Completeness'), h('span', { class: 'cxr-kpi-value' }, `${kpis.avgImplementationPct}% Impl`), h('span', { class: 'cxr-kpi-sub' }, `${kpis.avgIntegrationPct}% Canonical Integ`)),
      ),
    );

    const categories = [...new Set(allItems.map((i) => i.category))].sort();
    const toolbar = h(
      'div',
      { class: 'cxr-toolbar' },
      h(
        'div',
        { class: 'cxr-mode-tabs' },
        h(
          'button',
          {
            type: 'button',
            class: `cxr-mode-tab${state.viewMode === 'clusters' ? ' is-active' : ''}`,
            onClick: () => {
              state.viewMode = 'clusters';
              render();
            },
          },
          `Overlap Clusters (${kpis.clusterCount})`,
        ),
        h(
          'button',
          {
            type: 'button',
            class: `cxr-mode-tab${state.viewMode === 'codetree' ? ' is-active' : ''}`,
            'data-testid': 'mode-codetree-btn',
            onClick: () => {
              state.viewMode = 'codetree';
              render();
            },
          },
          'Code Tree & Backend',
        ),
        h(
          'button',
          {
            type: 'button',
            class: `cxr-mode-tab${state.viewMode === 'audit' ? ' is-active' : ''}`,
            'data-testid': 'mode-audit-btn',
            onClick: () => {
              state.viewMode = 'audit';
              render();
            },
          },
          'File-by-File Audit Queue',
        ),
        h(
          'button',
          {
            type: 'button',
            class: `cxr-mode-tab${state.viewMode === 'bookshelf' ? ' is-active' : ''}`,
            onClick: () => {
              state.viewMode = 'bookshelf';
              render();
            },
          },
          `Every Surface (${filteredItems.length})`,
        ),
      ),
      h('input', {
        type: 'search',
        class: 'cxr-search',
        placeholder: 'Search name, path, SHA-256, localStorage/DB authority...',
        value: state.filters.query,
        onInput: (ev) => {
          state.filters.query = ev.target?.value ?? '';
          if (state.viewMode === 'clusters' && state.filters.query.trim()) {
            state.viewMode = 'bookshelf';
          }
          render();
        },
      }),
      h(
        'select',
        {
          class: 'cxr-select',
          value: state.filters.category,
          onChange: (ev) => {
            state.filters.category = ev.target?.value || 'all';
            render();
          },
        },
        h('option', { value: 'all' }, 'All Categories'),
        categories.map((cat) => h('option', { value: cat }, cat)),
      ),
      h(
        'select',
        {
          class: 'cxr-select',
          value: state.filters.status,
          onChange: (ev) => {
            state.filters.status = ev.target?.value || 'all';
            render();
          },
        },
        h('option', { value: 'all' }, 'All Official Statuses'),
        OFFICIAL_STATUSES.map((st) => h('option', { value: st }, `${st} (${kpis.byStatus[st] || 0})`)),
      ),
      h(
        'div',
        { class: 'cxr-pill-bar' },
        ['all', ...SURFACE_BUCKETS].map((b) =>
          h(
            'button',
            {
              type: 'button',
              class: `cxr-pill${state.filters.surfaceBucket === b ? ' is-active' : ''}`,
              onClick: () => {
                state.filters.surfaceBucket = b;
                render();
              },
            },
            b === 'all' ? 'All Buckets' : `${BUCKET_LABELS[b]} (${kpis.bySurfaceBucket[b] || 0})`,
          ),
        ),
      ),
    );

    const shelfPane = h(
      'div',
      { class: 'cxr-shelf-pane' },
      state.viewMode === 'clusters'
        ? renderClusterShelf()
        : state.viewMode === 'codetree'
          ? renderCodeTreeShelf(filteredItems)
          : state.viewMode === 'audit'
            ? renderAuditQueueShelf(filteredItems)
            : (() => {
                let auto3dCount = 0;
                const limit = state.bookshelfLimit || 120;
                const visible = filteredItems.slice(0, limit);
                return h(
                  'div',
                  { style: 'display:flex;flex-direction:column;gap:12px;' },
                  h(
                    'div',
                    { class: 'cxr-card-grid' },
                    visible.map((it) => {
                      const canAuto =
                        Boolean(it.glbStats) &&
                        (state.glbCache.has(it.id) ||
                          (auto3dCount < 6 && (it.fileSize || 0) <= 20 * 1024 * 1024 && (++auto3dCount, true)));
                      return renderBookCard(it, { showTurntable: canAuto });
                    }),
                  ),
                  filteredItems.length > limit
                    ? h(
                        'button',
                        {
                          type: 'button',
                          class: 'cxr-btn',
                          onClick: () => {
                            state.bookshelfLimit = limit + 120;
                            render();
                          },
                        },
                        `Load More (+120 · showing ${visible.length} of ${filteredItems.length})`,
                      )
                    : null,
                );
              })(),
    );

    const inspectorPane = h('aside', { class: 'cxr-inspector-pane' }, renderInspector(selectedItem));

    root.append(header, toolbar, h('div', { class: 'cxr-workspace' }, shelfPane, inspectorPane));
  }

  render();

  if (!state.catalog) {
    loadXrayCatalog({ fetchImpl, studioApi }).then(({ catalog, sourceUrl }) => {
      if (state.disposed) return;
      state.catalog = catalog;
      state.sourceUrl = sourceUrl;
      state.loading = false;
      render();
    });
  }

  return {
    getState: () => state,
    selectItem,
    setItemTriage,
    exportJson: () => exportTriageLedgerJson(state.catalog, state.decisions),
    exportMarkdown: () => exportTriageActionPlanMarkdown(state.catalog, state.decisions),
    refresh: async () => {
      const { catalog, sourceUrl } = await loadXrayCatalog({ fetchImpl, studioApi });
      if (!state.disposed) {
        state.catalog = catalog;
        state.sourceUrl = sourceUrl;
        state.loading = false;
        render();
      }
    },
    destroy: () => {
      state.disposed = true;
      root.remove?.();
    },
  };
}

export const mountCodeXrayWorkbench = mountCodeXray;
export const mount = mountCodeXray;
export default mountCodeXray;

// Self-register with Ganbate Studio workbench discovery + standalone fallback
if (typeof window !== 'undefined') {
  const registration = {
    id: 'code-xray',
    label: 'Code & Asset X-Ray',
    mount: mountCodeXray,
  };
  window.__ganbateRegisterWorkbench?.(registration);
  window.__ganbateStudioApi?.registerWorkbench?.(registration);

  if (typeof document !== 'undefined') {
    const standaloneRoot =
      document.getElementById?.('code-xray-standalone-root') ||
      (document.body?.dataset?.standaloneWorkbench === 'code-xray' ? document.body : null);
    if (standaloneRoot) {
      mountCodeXray(standaloneRoot, window.__ganbateStudioApi || {});
    }
  }
}
