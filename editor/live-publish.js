// Native Skip-Deploy Live Release Control (`#live-release-menu`) for Ganbate Studio.
// Communicates directly with the configured game workspace's `liveRelease` endpoints using
// `credentials: 'include'` (scoped strictly to the game origin, never Studio's pairing cookie).
import { projectAssetUrl } from './asset-library.js';
import { materializeDataValues } from './data-inspector.js';

function jsonEqual(a, b) {
  return JSON.stringify(a) === JSON.stringify(b);
}

export function createLivePublish({
  el,
  t,
  getContext,
  collectBundle,
  ensureDocumentOpen,
  editDocument,
  onInspectPulled,
  log,
}) {
  let liveConfig = null;
  let status = {
    state: 'local', // 'local' | 'draft' | 'live'
    revision: null,
    hasDraft: false,
    note: '',
    updatedAt: null,
  };
  let busy = false;

  const menuEl = () => document.getElementById('live-release-menu');
  const badgeEl = () => document.getElementById('live-release-badge');
  const summaryEl = () => document.getElementById('live-release-summary');
  const noteInputEl = () => document.getElementById('live-release-note');

  function applyStatusPayload(payload) {
    if (!payload || typeof payload !== 'object') return;
    const rev = payload.revision ?? payload.liveRevision ?? payload.version ?? null;
    const hasDraft = Boolean(payload.hasDraft ?? payload.staged ?? payload.draft);
    const state = hasDraft ? 'draft' : (rev !== null && rev !== undefined && rev !== 0 ? 'live' : 'local');
    status = {
      state,
      revision: rev,
      hasDraft,
      note: typeof payload.note === 'string' ? payload.note : '',
      updatedAt: payload.updatedAt || payload.publishedAt || null,
    };
    renderStatus();
  }

  function renderStatus() {
    const menu = menuEl();
    if (!menu) return;
    menu.hidden = !liveConfig;
    if (!liveConfig) return;

    const badge = badgeEl();
    if (badge) {
      if (status.state === 'draft') {
        badge.textContent = status.revision ? `${t('liveDraft')} · r${status.revision}` : t('liveDraft');
        badge.dataset.state = 'draft';
      } else if (status.state === 'live') {
        badge.textContent = t('liveRev', status.revision);
        badge.dataset.state = 'live';
      } else {
        badge.textContent = t('liveLocal');
        badge.dataset.state = 'local';
      }
    }

    const sum = summaryEl();
    if (sum) {
      const parts = [];
      if (status.state === 'draft') parts.push(t('liveDraftReady'));
      else if (status.state === 'live') parts.push(t('livePublishedAt', status.revision));
      else parts.push(t('liveLocalHint'));
      if (status.note) parts.push(`“${status.note}”`);
      sum.textContent = parts.join(' · ');
    }

    for (const id of ['live-refresh', 'live-stage', 'live-publish', 'live-discard', 'live-pull']) {
      const btn = document.getElementById(id);
      if (btn) btn.disabled = busy;
    }
  }

  async function requestJson(url, options = {}) {
    const res = await fetch(url, {
      credentials: 'include',
      referrerPolicy: 'no-referrer',
      ...options,
      headers: {
        Accept: 'application/json',
        ...(options.body ? { 'Content-Type': 'application/json' } : {}),
        ...(options.headers || {}),
      },
    });
    const text = await res.text();
    let parsed = null;
    if (text) {
      try {
        parsed = JSON.parse(text);
      } catch {
        parsed = null;
      }
    }
    if (!res.ok) {
      throw new Error(parsed?.error || parsed?.message || `${res.status} ${res.statusText}`);
    }
    return parsed;
  }

  async function refreshStatus() {
    if (!liveConfig?.statusPath) return null;
    try {
      const data = await requestJson(liveConfig.statusPath, { method: 'GET' });
      applyStatusPayload(data);
      return data;
    } catch (err) {
      log?.(2, `Live Release: ${err.message}`);
      return null;
    }
  }

  async function stageDraft() {
    if (!liveConfig?.stagePath || busy) return null;
    busy = true;
    renderStatus();
    try {
      const payload = await collectBundle();
      const data = await requestJson(liveConfig.stagePath, {
        method: 'PUT',
        body: JSON.stringify(payload),
      });
      if (data && typeof data === 'object' && ('hasDraft' in data || 'staged' in data || 'revision' in data)) {
        applyStatusPayload({ ...data, hasDraft: data.hasDraft ?? true });
      } else {
        status.hasDraft = true;
        status.state = 'draft';
        renderStatus();
      }
      log?.(0, t('liveStagedLog', Object.keys(payload.documents || {}).length));
      if (liveConfig.statusPath) await refreshStatus();
      return data;
    } catch (err) {
      log?.(2, `Stage Draft: ${err.message}`);
      return null;
    } finally {
      busy = false;
      renderStatus();
    }
  }

  async function publishLive() {
    if (!liveConfig?.publishPath || busy) return null;
    busy = true;
    renderStatus();
    try {
      const note = noteInputEl()?.value?.trim() || '';
      const data = await requestJson(liveConfig.publishPath, {
        method: 'POST',
        body: JSON.stringify({ note }),
      });
      applyStatusPayload({
        ...data,
        hasDraft: false,
        revision: data?.revision ?? ((Number(status.revision) || 0) + 1),
        note: data?.note ?? note,
      });
      if (noteInputEl()) noteInputEl().value = '';
      log?.(0, t('livePublishedLog', status.revision ?? 1));
      if (liveConfig.statusPath) await refreshStatus();
      return data;
    } catch (err) {
      log?.(2, `Publish Live: ${err.message}`);
      return null;
    } finally {
      busy = false;
      renderStatus();
    }
  }

  async function discardDraft() {
    if (!liveConfig?.discardPath || busy) return null;
    busy = true;
    renderStatus();
    try {
      const data = await requestJson(liveConfig.discardPath, { method: 'POST' });
      applyStatusPayload({
        ...data,
        hasDraft: false,
        revision: data?.revision ?? status.revision,
      });
      log?.(0, t('liveDiscardedLog'));
      if (liveConfig.statusPath) await refreshStatus();
      return data;
    } catch (err) {
      log?.(2, `Discard Draft: ${err.message}`);
      return null;
    } finally {
      busy = false;
      renderStatus();
    }
  }

  async function readCurrentAssetRoot(asset) {
    const { datas, looks, hostDocRoot } = getContext();
    if (asset.kind === 'data' && datas?.get(asset.id)?.root) {
      return datas.get(asset.id).root;
    }
    if (asset.kind === 'look' && looks?.get(asset.id)?.root) {
      return looks.get(asset.id).root;
    }
    const inMemory = hostDocRoot?.(asset.id);
    if (inMemory && typeof inMemory === 'object') {
      return inMemory;
    }
    const res = await fetch(projectAssetUrl(asset.path));
    if (!res.ok) return null;
    return res.json();
  }

  async function pullLiveToProject() {
    if (!liveConfig?.bundlePath || busy) return 0;
    busy = true;
    renderStatus();
    try {
      const raw = await requestJson(liveConfig.bundlePath, { method: 'GET' });
      const bundle = raw?.bundle && typeof raw.bundle === 'object' ? raw.bundle : raw;
      const incomingDocs = bundle?.documents && typeof bundle.documents === 'object' ? bundle.documents : {};
      const incomingSchemas = bundle?.schemas && typeof bundle.schemas === 'object' ? bundle.schemas : {};

      const { assets, kinds } = getContext();
      let changedCount = 0;
      let firstChanged = null;

      for (const asset of assets || []) {
        if (asset.kind !== 'data' && asset.kind !== 'look') continue;
        const currentRoot = await readCurrentAssetRoot(asset);
        if (!currentRoot || typeof currentRoot !== 'object') continue;

        if (asset.kind === 'data') {
          const schemaName = currentRoot.schema || '';
          const schemaDef = kinds?.[schemaName];
          const byPath = incomingDocs[asset.path];
          const bySchema = schemaName ? incomingSchemas[schemaName] : null;
          if (!byPath && !bySchema) continue;

          const curValues = currentRoot.values && typeof currentRoot.values === 'object' ? currentRoot.values : {};
          const curEffective = materializeDataValues(curValues, schemaDef);
          const nextValues = structuredClone(
            byPath?.values && typeof byPath.values === 'object' ? byPath.values : curValues,
          );

          // Overlay any schema-level values that differ from materialized `nextValues`.
          if (bySchema && typeof bySchema === 'object') {
            const nextEffective = materializeDataValues(nextValues, schemaDef);
            for (const [k, v] of Object.entries(bySchema)) {
              if (!jsonEqual(nextEffective[k], v)) {
                nextValues[k] = structuredClone(v);
              }
            }
          }

          const nextName = typeof byPath?.name === 'string' ? byPath.name : currentRoot.name;
          const ops = [];
          if (typeof nextName === 'string' && nextName !== currentRoot.name) {
            ops.push({ set: ['name'], value: nextName });
          }

          const allKeys = new Set([...Object.keys(curValues), ...Object.keys(nextValues)]);
          for (const key of allKeys) {
            if (!(key in nextValues)) {
              if (key in curValues) ops.push({ remove: ['values', key] });
            } else if (!jsonEqual(curValues[key], nextValues[key]) && !jsonEqual(curEffective[key], nextValues[key])) {
              ops.push({ set: ['values', key], value: nextValues[key] });
            } else if (!(key in curValues) && byPath?.values && key in byPath.values && !jsonEqual(curValues[key], nextValues[key])) {
              ops.push({ set: ['values', key], value: nextValues[key] });
            } else if (key in curValues && !jsonEqual(curValues[key], nextValues[key])) {
              ops.push({ set: ['values', key], value: nextValues[key] });
            }
          }

          if (ops.length) {
            const opened = await ensureDocumentOpen(asset.id, 'data');
            if (opened) {
              await editDocument('Pull Live', ops, undefined, asset.id);
              changedCount += 1;
              if (!firstChanged) firstChanged = { id: asset.id, kind: 'data' };
            }
          }
        } else if (asset.kind === 'look') {
          const pulledLook = incomingDocs[asset.path];
          if (!pulledLook || typeof pulledLook !== 'object' || Array.isArray(pulledLook)) continue;
          if (jsonEqual(currentRoot, pulledLook)) continue;
          const ops = [];
          const allKeys = new Set([...Object.keys(currentRoot), ...Object.keys(pulledLook)]);
          for (const key of allKeys) {
            if (!(key in pulledLook)) {
              ops.push({ remove: [key] });
            } else if (!jsonEqual(currentRoot[key], pulledLook[key])) {
              ops.push({ set: [key], value: structuredClone(pulledLook[key]) });
            }
          }
          if (ops.length) {
            const opened = await ensureDocumentOpen(asset.id, 'look');
            if (opened) {
              await editDocument('Pull Live', ops, undefined, asset.id);
              changedCount += 1;
              if (!firstChanged) firstChanged = { id: asset.id, kind: 'look' };
            }
          }
        }
      }

      if (firstChanged) onInspectPulled?.(firstChanged);
      log?.(0, t('livePulledLog', changedCount));
      return changedCount;
    } catch (err) {
      log?.(2, `Pull Live: ${err.message}`);
      return 0;
    } finally {
      busy = false;
      renderStatus();
    }
  }

  function setWorkspace(workspace) {
    liveConfig = workspace?.liveRelease || null;
    if (!liveConfig) {
      status = { state: 'local', revision: null, hasDraft: false, note: '', updatedAt: null };
      renderStatus();
      return;
    }
    renderStatus();
    void refreshStatus();
  }

  function setupEvents() {
    document.getElementById('live-refresh')?.addEventListener('click', () => { void refreshStatus(); });
    document.getElementById('live-stage')?.addEventListener('click', () => { void stageDraft(); });
    document.getElementById('live-publish')?.addEventListener('click', () => { void publishLive(); });
    document.getElementById('live-discard')?.addEventListener('click', () => { void discardDraft(); });
    document.getElementById('live-pull')?.addEventListener('click', () => { void pullLiveToProject(); });
  }

  return {
    setupEvents,
    setWorkspace,
    refreshStatus,
    stageDraft,
    publishLive,
    discardDraft,
    pullLiveToProject,
    renderStatus,
  };
}
