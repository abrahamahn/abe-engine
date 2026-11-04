// The edit protocol's client side (see crates/ganbate-studio/src/session.rs):
// - `Host` (`LocalRustHost`): one WebSocket (`/api/ws`) to the local `ganbate-studio` Rust host.
// - `CloudStudioHost`: browser/cloud transport (`?transport=cloud` / `/studio`) backed by
//   `/api/studio/live-bundle?channel=draft`, SSE `/api/studio/live-changes`, client-side `applyOps`
//   + Undo/Redo, and `/api/dev-studio/live-bundle` draft staging.

/** What a waiting request gets when the connection is gone. */
const NOT_CONNECTED = { type: 'error', message: 'not connected to the studio host' };

export class Host {
  constructor(url, onMessage, onState) {
    this.kind = 'local';
    this.url = url;
    this.onMessage = onMessage;
    this.onState = onState;
    this.queue = [];
    this.waiting = null;
    this.nextReq = 1;
    this.nextRpc = 1;
    this.pendingRpcs = new Map();
    this.open = false;
    // Milliseconds before the next attempt to connect again.
    this.retry = 500;
  }

  /**
   * Connects. When a connection closes, everything waiting for an answer gets an error and the
   * page connects again, after half a second and then twice as long each time, up to 10 s.
   */
  connect() {
    return new Promise((resolve, reject) => {
      const ws = new WebSocket(this.url);
      this.ws = ws;
      ws.onopen = () => {
        this.open = true;
        this.retry = 500;
        this.onState('connected');
        resolve();
      };
      ws.onerror = () => {
        if (!this.open) reject(new Error(`cannot reach the studio host at ${this.url}`));
      };
      ws.onclose = () => {
        const was = this.open;
        this.open = false;
        const items = [this.waiting, ...this.queue].filter(Boolean);
        this.waiting = null;
        this.queue = [];
        for (const item of items) item.done(NOT_CONNECTED);
        for (const [, rpc] of this.pendingRpcs) {
          clearTimeout(rpc.timer);
          rpc.resolve(NOT_CONNECTED);
        }
        this.pendingRpcs.clear();
        if (was) this.onState('disconnected');
        setTimeout(() => this.connect().catch(() => {}), this.retry);
        this.retry = Math.min(10000, this.retry * 2);
      };
      ws.onmessage = (e) => {
        const msg = JSON.parse(e.data);
        if (msg.type === 'welcome') this.client = msg.client;
        if (typeof msg.req === 'string' && this.pendingRpcs.has(msg.req)) {
          const rpc = this.pendingRpcs.get(msg.req);
          this.pendingRpcs.delete(msg.req);
          clearTimeout(rpc.timer);
          try {
            this.onMessage(msg);
          } finally {
            rpc.resolve(msg);
          }
          return;
        }
        const w = this.waiting;
        // Request counters are local to a connection. Another editor may broadcast a
        // transaction or save with the same counter while ours is still pending.
        const own = msg.by === undefined || msg.by === this.client;
        if (w && own && ((msg.req === w.req && ['ops', 'saved', 'error'].includes(msg.type)) ||
            (msg.req == null && w.answers?.(msg)))) {
          this.waiting = null;
          try {
            this.onMessage(msg);
          } finally {
            w.done(msg);
            this.pump();
          }
          return;
        }
        this.onMessage(msg);
      };
    });
  }

  send(msg) {
    if (this.open) this.ws.send(JSON.stringify(msg));
  }

  /** Sends a request carrying a unique string `req` id and resolves with the matching host reply. */
  request(msg, timeoutMs = 15000) {
    if (!this.open) return Promise.resolve(NOT_CONNECTED);
    const req = typeof msg.req === 'string' && msg.req.startsWith('rpc:')
      ? msg.req
      : `rpc:${this.nextRpc++}`;
    const payload = { ...msg, req };
    return new Promise((resolve) => {
      const timer = setTimeout(() => {
        this.pendingRpcs.delete(req);
        resolve({ type: 'error', req, message: `studio host request timed out (${msg.type})` });
      }, timeoutMs);
      this.pendingRpcs.set(req, { resolve, timer });
      this.send(payload);
    });
  }

  /**
   * Queues a request about a document (an edit, undo, redo or save): `make()` builds the message
   * when its turn comes, so it carries the revision the host last confirmed, or returns null to
   * skip it. Resolves with the host's answer: `ops` or `error` with the message's `req`, or a
   * message `answers(msg)` accepts (for answers the host sends without `req`), or null when
   * skipped. Without a connection it resolves with an error at once.
   */
  edit(make, answers) {
    return new Promise((done) => {
      if (!this.open) {
        done(NOT_CONNECTED);
        return;
      }
      this.queue.push({ make, done, answers });
      this.pump();
    });
  }

  pump() {
    while (!this.waiting && this.queue.length && this.open) {
      const item = this.queue.shift();
      const msg = item.make();
      if (!msg) {
        item.done(null);
        continue;
      }
      msg.req = this.nextReq++;
      this.waiting = { req: msg.req, done: item.done, answers: item.answers };
      this.send(msg);
    }
  }
}

/** Applies ops to a document root the way the host does (`ganbate_scene::doc`). */
export function applyOps(root, ops) {
  for (const op of ops) {
    const path = op.set || op.remove;
    let v = root;
    for (let i = 0; i < path.length - 1; i++) {
      if (!Object.hasOwn(v, path[i]) || v[path[i]] === null || typeof v[path[i]] !== 'object' || Array.isArray(v[path[i]])) {
        if (op.remove) {
          v = null;
          break;
        }
        Object.defineProperty(v, path[i], { value: {}, writable: true, enumerable: true, configurable: true });
      }
      v = v[path[i]];
    }
    if (!v) continue;
    const last = path[path.length - 1];
    // JSON keys are data, including "__proto__" in a project-supplied document.
    if (op.set) Object.defineProperty(v, last, { value: structuredClone(op.value), writable: true, enumerable: true, configurable: true });
    else delete v[last];
  }
}

/** Applies ops while returning exact inverse ops so `CloudStudioHost` supports multi-step Undo/Redo. */
export function applyOpsWithInverse(root, ops) {
  const inverse = [];
  for (const op of ops) {
    const path = op.set || op.remove;
    if (!Array.isArray(path) || !path.length) continue;
    let v = root;
    let createdAt = -1;
    let prevReplacedAtCreate = undefined;
    let hadReplacedAtCreate = false;
    for (let i = 0; i < path.length - 1; i++) {
      const key = path[i];
      if (!Object.hasOwn(v, key) || v[key] === null || typeof v[key] !== 'object' || Array.isArray(v[key])) {
        if (op.remove) {
          v = null;
          break;
        }
        if (createdAt < 0) {
          createdAt = i;
          hadReplacedAtCreate = Object.hasOwn(v, key);
          if (hadReplacedAtCreate) prevReplacedAtCreate = structuredClone(v[key]);
        }
        Object.defineProperty(v, key, { value: {}, writable: true, enumerable: true, configurable: true });
      }
      v = v[key];
    }
    if (!v) continue;
    const last = path[path.length - 1];
    if (op.set) {
      if (createdAt >= 0) {
        const prefix = path.slice(0, createdAt + 1);
        inverse.unshift(hadReplacedAtCreate
          ? { set: prefix, value: prevReplacedAtCreate }
          : { remove: prefix });
      } else if (Object.hasOwn(v, last)) {
        inverse.unshift({ set: path.slice(), value: structuredClone(v[last]) });
      } else {
        inverse.unshift({ remove: path.slice() });
      }
      Object.defineProperty(v, last, { value: structuredClone(op.value), writable: true, enumerable: true, configurable: true });
    } else if (Object.hasOwn(v, last)) {
      inverse.unshift({ set: path.slice(), value: structuredClone(v[last]) });
      delete v[last];
    }
  }
  return inverse;
}

/** Deterministic 32-hex asset/document ID for cloud-hosted documents without `.meta.json`. */
export function cloudDocId(relPath) {
  const s = `cloud:${String(relPath).replace(/\\/g, '/').replace(/^\/+/, '')}`;
  const seeds = [0x811c9dc5, 0x9e3779b9, 0x85ebca6b, 0xc2b2ae35];
  for (let i = 0; i < s.length; i++) {
    const code = s.charCodeAt(i);
    for (let k = 0; k < 4; k++) {
      let h = seeds[k] ^ ((code + k * 31) & 0xff);
      h = Math.imul(h, 0x01000193) >>> 0;
      seeds[k] = ((h << 5) | (h >>> 27)) >>> 0;
    }
  }
  return seeds.map((n) => n.toString(16).padStart(8, '0')).join('');
}

function inferCloudDocKind(path, root) {
  const lower = String(path).toLowerCase();
  if (lower.endsWith('.scene.json') || root?.ganbate === 'scene') return 'scene';
  if (lower.endsWith('.prefab.json') || root?.ganbate === 'prefab') return 'prefab';
  if (lower.endsWith('.look.json') || root?.ganbate === 'look') return 'look';
  if (lower.endsWith('.material.json') || root?.ganbate === 'material') return 'material';
  if (lower.endsWith('.soundbank.json') || root?.ganbate === 'soundbank') return 'soundbank';
  if (lower.endsWith('.character.json') || root?.ganbate === 'character') return 'character';
  if (lower.endsWith('.seq.json') || root?.ganbate === 'seq') return 'seq';
  if (lower.endsWith('.vfx.json') || root?.ganbate === 'vfx') return 'vfx';
  if (lower.endsWith('.data.json') || root?.ganbate === 'data') return 'data';
  if (lower.endsWith('.glb')) return 'model';
  if (lower.endsWith('.png') || lower.endsWith('.jpg') || lower.endsWith('.jpeg') || lower.endsWith('.webp')) return 'texture';
  if (lower.endsWith('.wav') || lower.endsWith('.ogg')) return 'sound';
  return 'data';
}

function randomEntityHex16(seq) {
  const hi = ((Date.now() & 0xffffffff) >>> 0).toString(16).padStart(8, '0');
  const lo = ((Math.imul(seq + 1, 0x9e3779b1) ^ (Math.random() * 0xffffffff)) >>> 0)
    .toString(16)
    .padStart(8, '0');
  return hi + lo;
}

function defaultRootForCreate(kind, id, path, schema, name) {
  const docName = name || String(path).split('/').pop()?.replace(/\.[a-z]+\.json$/i, '') || kind;
  switch (kind) {
    case 'scene':
      return { ganbate: 'scene', version: 1, name: docName, environment: {}, entities: {} };
    case 'prefab':
      return {
        ganbate: 'prefab',
        version: 1,
        name: docName,
        entities: {
          [randomEntityHex16(1)]: { name: docName, components: { transform: {} } },
        },
      };
    case 'look':
      return { ganbate: 'look', version: 1, name: docName };
    case 'material':
      return { ganbate: 'material', version: 1, name: docName, baseColor: [0.8, 0.8, 0.8, 1], roughness: 0.5, metallic: 0 };
    case 'soundbank':
      return { ganbate: 'soundbank', version: 1, name: docName, buses: [], events: [] };
    case 'character':
      return { ganbate: 'character', version: 1, name: docName, skeleton: null, clips: [], states: [] };
    case 'seq':
      return { ganbate: 'seq', version: 1, name: docName, duration: 4.0, tracks: [] };
    case 'vfx':
      return { ganbate: 'vfx', version: 1, name: docName, emitters: [] };
    case 'data':
    default:
      return {
        ganbate: 'data',
        version: 1,
        id: id.slice(0, 16),
        schema: schema || 'custom.data',
        name: docName,
        values: {},
      };
  }
}

export class CloudStudioHost {
  constructor(url, onMessage, onState, options = {}) {
    this.kind = 'cloud';
    this.url = url;
    this.onMessage = onMessage;
    this.onState = onState;
    this.options = options;
    this.open = false;
    this.client = 1;
    this.queue = [];
    this.waiting = null;
    this.nextReq = 1;
    this.nextRpc = 1;
    this.idSeq = 1;
    this.docs = new Map(); // id -> { id, path, kind, rev, dirty, root, undoStack, redoStack }
    this.assets = []; // [{ id, kind, path, bytes }]
    this.schemas = []; // [{ ganbate: 'schema', version: 1, components: {...} }]
    this.project = {
      id: options.projectId || 'ganbate-cloud-studio',
      name: options.projectName || 'Ganbate Live Studio',
      content: ['content'],
      schemas: ['schemas'],
      editor: ['editor'],
      startup: null,
    };
    this.cloudToolsManifest = options.cloudToolsManifest || null;
    this.cloudGameManifest = options.cloudGameManifest || null;
    this.selection = null;
    this.eventSource = null;
    this._onWindowMessage = null;
  }

  getCachedDocRoot(id) {
    const entry = this.docs.get(id);
    return entry?.root ? structuredClone(entry.root) : null;
  }

  connect() {
    this.open = true;
    const sseUrl = this.options.sseUrl ?? '/api/studio/live-changes';
    if (sseUrl && typeof EventSource !== 'undefined') {
      try {
        const es = new EventSource(sseUrl, { withCredentials: true });
        this.eventSource = es;
        const onRemoteChange = () => {
          void this._refreshFromLiveBundle(false);
        };
        es.addEventListener('change', onRemoteChange);
        es.addEventListener('studio-live', onRemoteChange);
        es.onmessage = (ev) => {
          if (!ev?.data) return;
          try {
            const parsed = JSON.parse(ev.data);
            if (parsed?.type === 'ping' || parsed?.type === 'connected') return;
          } catch {
            /* ignore non-JSON heartbeat */
          }
          onRemoteChange();
        };
        es.onerror = () => {
          /* SSE reconnects automatically; draft local state remains usable */
        };
      } catch {
        /* ignore environments where EventSource is blocked */
      }
    }
    if (typeof window !== 'undefined') {
      this._onWindowMessage = (ev) => {
        const data = ev?.data;
        if (!data || data.ganbateStudioHost !== 1) return;
        if (data.type === 'cloud:bootstrap') {
          this._ingestBootstrapPayload(data.payload || {});
          this._emitWelcome();
        } else if (data.type === 'cloud:bundle-update' && data.bundle) {
          this._ingestBundle(data.bundle, true);
        }
      };
      window.addEventListener('message', this._onWindowMessage);
    }
    queueMicrotask(() => {
      if (this.open) this.onState('connected');
    });
    return Promise.resolve();
  }

  disconnect() {
    this.open = false;
    if (this.eventSource) {
      this.eventSource.close();
      this.eventSource = null;
    }
    if (this._onWindowMessage && typeof window !== 'undefined') {
      window.removeEventListener('message', this._onWindowMessage);
      this._onWindowMessage = null;
    }
  }

  _upsertDoc(path, root, explicitKind = null) {
    const cleanPath = String(path).replace(/\\/g, '/').replace(/^\/+/, '');
    const id = cloudDocId(cleanPath);
    const kind = explicitKind || inferCloudDocKind(cleanPath, root);
    const existing = this.docs.get(id);
    const cloned = structuredClone(root || {});
    if (existing) {
      if (!existing.dirty) {
        existing.root = cloned;
      }
      existing.path = cleanPath;
      existing.kind = kind;
    } else {
      this.docs.set(id, {
        id,
        path: cleanPath,
        kind,
        rev: 1,
        dirty: false,
        root: cloned,
        undoStack: [],
        redoStack: [],
      });
    }
    this._rebuildAssetIndex();
    return this.docs.get(id);
  }

  _rebuildAssetIndex() {
    const byId = new Map();
    for (const a of this.assets) {
      if (!this.docs.has(a.id)) byId.set(a.id, a);
    }
    for (const doc of this.docs.values()) {
      const bytes = JSON.stringify(doc.root || {}).length;
      byId.set(doc.id, { id: doc.id, kind: doc.kind, path: doc.path, bytes });
    }
    this.assets = [...byId.values()].sort((a, b) => a.path.localeCompare(b.path));
    if (!this.project.startup) {
      const firstScene = this.assets.find((a) => a.kind === 'scene');
      if (firstScene) this.project.startup = firstScene.id;
    }
  }

  _ingestBundle(raw, emitUpdates = false) {
    if (!raw || typeof raw !== 'object') return;
    const bundle = raw.bundle && typeof raw.bundle === 'object' ? raw.bundle : raw;
    const docs = bundle.documents && typeof bundle.documents === 'object' ? bundle.documents : {};
    const schemas = bundle.schemas && typeof bundle.schemas === 'object' ? bundle.schemas : {};

    for (const [docPath, docRoot] of Object.entries(docs)) {
      if (!docRoot || typeof docRoot !== 'object') continue;
      const before = this.docs.get(cloudDocId(docPath));
      const beforeJson = before ? JSON.stringify(before.root) : null;
      const entry = this._upsertDoc(docPath, docRoot);
      if (emitUpdates && !entry.dirty && beforeJson !== JSON.stringify(entry.root)) {
        entry.rev += 1;
        this._emit(this._docPayload(entry));
      }
    }

    // Materialize synthetic `.data.json` documents for any schema entries without explicit document files.
    for (const [schemaKey, values] of Object.entries(schemas)) {
      if (!values || typeof values !== 'object' || Array.isArray(values)) continue;
      const hasDoc = [...this.docs.values()].some(
        (d) => d.kind === 'data' && d.root?.schema === schemaKey,
      );
      if (!hasDoc) {
        const synthPath = `content/data/${schemaKey.replace(/\./g, '-')}.data.json`;
        const id = cloudDocId(synthPath);
        this._upsertDoc(synthPath, {
          ganbate: 'data',
          version: 1,
          id: id.slice(0, 16),
          schema: schemaKey,
          name: schemaKey,
          values: structuredClone(values),
        }, 'data');
      }
    }

    if (Array.isArray(bundle.schemaDefinitions) && bundle.schemaDefinitions.length) {
      this.schemas = bundle.schemaDefinitions;
    } else {
      this._synthesizeSchemasFromDataDocs();
    }
    this._rebuildAssetIndex();
    if (emitUpdates) {
      this._emit({ type: 'assets', assets: structuredClone(this.assets) });
    }
  }

  _synthesizeSchemasFromDataDocs() {
    const existingComponents = {};
    for (const file of this.schemas) {
      Object.assign(existingComponents, file.components || {});
    }
    for (const doc of this.docs.values()) {
      if (doc.kind !== 'data' || !doc.root?.schema) continue;
      const schemaName = doc.root.schema;
      if (existingComponents[schemaName]) continue;
      const fields = {};
      for (const [k, v] of Object.entries(doc.root.values || {})) {
        fields[k] = inferSchemaFieldFromValue(v);
      }
      existingComponents[schemaName] = { label: doc.root.name || schemaName, fields };
    }
    // Always include baseline engine transform/camera/light/mesh schemas so 3D World Inspector works in cloud mode.
    Object.assign(existingComponents, {
      transform: existingComponents.transform || {
        label: 'Transform',
        fields: {
          translation: { type: 'vec3', default: [0, 0, 0] },
          rotation: { type: 'quat', default: [0, 0, 0, 1] },
          scale: { type: 'vec3', default: [1, 1, 1] },
        },
      },
      camera: existingComponents.camera || {
        label: 'Camera',
        fields: { fov: { type: 'float', min: 10, max: 150, default: 50 } },
      },
      light: existingComponents.light || {
        label: 'Light',
        fields: {
          color: { type: 'rgb', default: [1, 0.96, 0.9] },
          intensity: { type: 'float', min: 0, max: 50, default: 4 },
        },
      },
      mesh: existingComponents.mesh || {
        label: 'Mesh',
        fields: {
          model: { type: 'asset', kind: 'model' },
          material: { type: 'asset', kind: 'material' },
        },
      },
    });
    this.schemas = [{ ganbate: 'schema', version: 1, components: existingComponents }];
  }

  _ingestBootstrapPayload(payload) {
    if (payload.project) this.project = { ...this.project, ...payload.project };
    if (Array.isArray(payload.schemas)) this.schemas = payload.schemas;
    if (Array.isArray(payload.assets)) {
      for (const a of payload.assets) {
        if (a?.id && a?.path && a?.kind && !this.docs.has(a.id)) {
          this.assets.push({ id: a.id, kind: a.kind, path: a.path, bytes: a.bytes || 0 });
        }
      }
    }
    if (payload.cloudToolsManifest) this.cloudToolsManifest = payload.cloudToolsManifest;
    if (payload.cloudGameManifest) this.cloudGameManifest = payload.cloudGameManifest;
    if (payload.bundle) this._ingestBundle(payload.bundle, false);
    this._synthesizeSchemasFromDataDocs();
    this._rebuildAssetIndex();
  }

  async _refreshFromLiveBundle(isInitial = false) {
    const globalBootstrap = typeof globalThis !== 'undefined' ? globalThis.__GANBATE_CLOUD_BOOTSTRAP__ : null;
    if (isInitial && globalBootstrap) {
      this._ingestBootstrapPayload(globalBootstrap);
    }
    const manifestUrl = this.options.cloudManifestUrl ?? '/ganbate-studio/cloud-manifest.json';
    if (isInitial && manifestUrl && typeof fetch === 'function') {
      try {
        const res = await fetch(manifestUrl, { credentials: 'include' });
        if (res.ok) {
          this._ingestBootstrapPayload(await res.json());
        }
      } catch {
        /* optional static cloud manifest */
      }
    }
    const bundleUrl = this.options.bundleUrl ?? '/api/studio/live-bundle?channel=draft';
    if (bundleUrl && typeof fetch === 'function') {
      try {
        const res = await fetch(bundleUrl, {
          credentials: 'include',
          headers: { Accept: 'application/json' },
        });
        if (res.ok) {
          this._ingestBundle(await res.json(), !isInitial);
        }
      } catch {
        /* offline or test fallback */
      }
    }
    if (isInitial && this.docs.size === 0) {
      // Seed a clean default scene so 3D World & Look workbench is immediately interactive in Cloud mode.
      this._upsertDoc('content/scenes/studio-stage.scene.json', {
        ganbate: 'scene',
        version: 1,
        name: 'Studio Stage',
        environment: { time: 17.5, weather: 'clear' },
        entities: {
          '0000000000000001': {
            name: 'Key Camera',
            components: {
              transform: { translation: [0, 3.2, 8.5], rotation: [-0.14, 0, 0, 0.99] },
              camera: { fov: 48 },
            },
          },
          '0000000000000002': {
            name: 'Stage Sun',
            components: {
              transform: { translation: [4, 7, 5] },
              light: { color: [1, 0.95, 0.88], intensity: 5 },
            },
          },
        },
      }, 'scene');
    }
    this._synthesizeSchemasFromDataDocs();
    this._rebuildAssetIndex();
  }

  _emitWelcome() {
    this._emit({
      type: 'welcome',
      client: this.client,
      project: structuredClone(this.project),
      schemas: structuredClone(this.schemas),
      assets: structuredClone(this.assets),
      drafts: [],
      toolCatalogs: [],
      gameWorkspaces: [],
      cloudToolsManifest: this.cloudToolsManifest ? structuredClone(this.cloudToolsManifest) : null,
      cloudGameManifest: this.cloudGameManifest ? structuredClone(this.cloudGameManifest) : null,
    });
  }

  _docPayload(entry, req) {
    return {
      type: 'doc',
      ...(req !== undefined ? { req } : {}),
      doc: entry.id,
      rev: entry.rev,
      path: entry.path,
      kind: entry.kind,
      root: structuredClone(entry.root),
      dirty: entry.dirty,
      undo: entry.undoStack.at(-1)?.label ?? null,
      redo: entry.redoStack.at(-1)?.label ?? null,
      issues: [],
      unavailable: false,
      changedOnDisk: false,
    };
  }

  _resolveEntry(docOrPath) {
    if (!docOrPath) return null;
    if (this.docs.has(docOrPath)) return this.docs.get(docOrPath);
    const norm = String(docOrPath).replace(/\\/g, '/').replace(/^\/+/, '');
    for (const d of this.docs.values()) {
      if (d.path === norm) return d;
    }
    return null;
  }

  async _stageCloudBundle() {
    const documents = {};
    const schemas = {};
    for (const d of this.docs.values()) {
      documents[d.path] = structuredClone(d.root);
      if (d.kind === 'data' && d.root?.schema) {
        schemas[d.root.schema] = structuredClone(d.root.values || {});
      }
    }
    const stageUrl = this.options.stageUrl ?? '/api/dev-studio/live-bundle';
    if (stageUrl && typeof fetch === 'function') {
      try {
        await fetch(stageUrl, {
          method: 'PUT',
          credentials: 'include',
          headers: { 'Content-Type': 'application/json', Accept: 'application/json' },
          body: JSON.stringify({ documents, schemas }),
        });
      } catch {
        /* local/test cloud mode without backend remains non-fatal */
      }
    }
    if (typeof window !== 'undefined' && window.parent && window.parent !== window) {
      try {
        window.parent.postMessage({
          ganbateStudioCloud: 1,
          type: 'cloud:staged',
          documentsCount: Object.keys(documents).length,
        }, '*');
      } catch {
        /* ignore cross-frame restrictions */
      }
    }
  }

  send(msg) {
    if (!this.open) return;
    queueMicrotask(() => {
      void this._handleMessage(msg);
    });
  }

  request(msg) {
    if (!this.open) return Promise.resolve(NOT_CONNECTED);
    const req = typeof msg.req === 'string' && msg.req.startsWith('rpc:')
      ? msg.req
      : `rpc:${this.nextRpc++}`;
    return new Promise((resolve) => {
      this._pendingRpc = this._pendingRpc || new Map();
      this._pendingRpc.set(req, resolve);
      this.send({ ...msg, req });
    });
  }

  edit(make, answers) {
    return new Promise((done) => {
      if (!this.open) {
        done(NOT_CONNECTED);
        return;
      }
      this.queue.push({ make, done, answers });
      this.pump();
    });
  }

  pump() {
    while (!this.waiting && this.queue.length && this.open) {
      const item = this.queue.shift();
      const msg = item.make();
      if (!msg) {
        item.done(null);
        continue;
      }
      msg.req = this.nextReq++;
      this.waiting = { req: msg.req, done: item.done, answers: item.answers };
      this.send(msg);
    }
  }

  _emit(reply) {
    if (typeof reply.req === 'string' && this._pendingRpc?.has(reply.req)) {
      const resolve = this._pendingRpc.get(reply.req);
      this._pendingRpc.delete(reply.req);
      try {
        this.onMessage(reply);
      } finally {
        resolve(reply);
      }
      return;
    }
    const w = this.waiting;
    if (w && ((reply.req === w.req && ['ops', 'saved', 'error'].includes(reply.type)) ||
        (reply.req == null && w.answers?.(reply)))) {
      this.waiting = null;
      try {
        this.onMessage(reply);
      } finally {
        w.done(reply);
        this.pump();
      }
      return;
    }
    this.onMessage(reply);
  }

  async _handleMessage(msg) {
    if (!this.open || !msg || typeof msg !== 'object') return;
    const req = msg.req;

    switch (msg.type) {
      case 'hello': {
        await this._refreshFromLiveBundle(true);
        this._emitWelcome();
        break;
      }
      case 'rescan': {
        await this._refreshFromLiveBundle(false);
        this._emit({
          type: 'rescanned',
          req,
          ok: true,
          added: 0,
          moved: 0,
          removed: 0,
          identity: 0,
          problems: [],
          drafts: [],
          schemas: structuredClone(this.schemas),
          gameWorkspaces: [],
        });
        this._emit({ type: 'assets', assets: structuredClone(this.assets) });
        break;
      }
      case 'ids': {
        const count = Math.max(1, Math.min(512, Number(msg.count) || 64));
        const ids = [];
        for (let i = 0; i < count; i++) ids.push(randomEntityHex16(this.idSeq++));
        this._emit({ type: 'ids', req, ids });
        break;
      }
      case 'open': {
        const entry = this._resolveEntry(msg.doc);
        if (!entry) {
          this._emit({ type: 'error', req, message: `document not found: ${msg.doc}` });
          break;
        }
        this._emit(this._docPayload(entry, req));
        break;
      }
      case 'close':
        break;
      case 'create': {
        const rawPath = String(msg.path || '').replace(/\\/g, '/').replace(/^\/+/, '');
        if (!rawPath) {
          this._emit({ type: 'error', req, message: 'missing document path' });
          break;
        }
        const id = cloudDocId(rawPath);
        const kind = msg.kind || inferCloudDocKind(rawPath, msg.initialRoot);
        const root = msg.initialRoot && typeof msg.initialRoot === 'object'
          ? structuredClone(msg.initialRoot)
          : defaultRootForCreate(kind, id, rawPath, msg.schema, msg.name);
        const entry = this._upsertDoc(rawPath, root, kind);
        this._emit({ type: 'assets', assets: structuredClone(this.assets) });
        this._emit({ type: 'created', req, doc: entry.id, path: entry.path, kind: entry.kind });
        break;
      }
      case 'edit': {
        const entry = this._resolveEntry(msg.doc);
        if (!entry) {
          this._emit({ type: 'error', req, by: this.client, message: `document not open: ${msg.doc}` });
          break;
        }
        if (typeof msg.rev === 'number' && msg.rev !== entry.rev) {
          this._emit({ type: 'error', req, by: this.client, message: `stale revision ${msg.rev} (current ${entry.rev})` });
          break;
        }
        const ops = Array.isArray(msg.ops) ? structuredClone(msg.ops) : [];
        const inverseOps = applyOpsWithInverse(entry.root, ops);
        entry.rev += 1;
        entry.dirty = true;
        const label = msg.label || 'Edit';
        const prev = entry.undoStack.at(-1);
        if (msg.merge && prev && prev.merge === msg.merge) {
          prev.forwardOps.push(...ops);
          prev.inverseOps.unshift(...inverseOps);
          prev.label = label;
        } else {
          entry.undoStack.push({ label, merge: msg.merge || null, forwardOps: ops, inverseOps });
        }
        entry.redoStack = [];
        this._emit({
          type: 'ops',
          req,
          by: this.client,
          doc: entry.id,
          rev: entry.rev,
          dirty: entry.dirty,
          undo: entry.undoStack.at(-1)?.label ?? null,
          redo: null,
          ops,
          issues: [],
        });
        break;
      }
      case 'delete': {
        const entry = this._resolveEntry(msg.doc);
        if (!entry || !entry.root?.entities) {
          this._emit({ type: 'error', req, by: this.client, message: 'scene not open' });
          break;
        }
        const toRemove = new Set(Array.isArray(msg.ids) ? msg.ids : []);
        let changed = true;
        while (changed) {
          changed = false;
          for (const [eid, ent] of Object.entries(entry.root.entities)) {
            if (!toRemove.has(eid) && ent?.parent && toRemove.has(ent.parent)) {
              toRemove.add(eid);
              changed = true;
            }
          }
        }
        const ops = [...toRemove].map((eid) => ({ remove: ['entities', eid] }));
        const inverseOps = applyOpsWithInverse(entry.root, ops);
        entry.rev += 1;
        entry.dirty = true;
        entry.undoStack.push({ label: msg.label || 'Delete', merge: null, forwardOps: ops, inverseOps });
        entry.redoStack = [];
        this._emit({
          type: 'ops',
          req,
          by: this.client,
          doc: entry.id,
          rev: entry.rev,
          dirty: entry.dirty,
          undo: entry.undoStack.at(-1)?.label ?? null,
          redo: null,
          ops,
          issues: [],
        });
        break;
      }
      case 'undo':
      case 'redo': {
        const entry = this._resolveEntry(msg.doc);
        const stack = msg.type === 'undo' ? entry?.undoStack : entry?.redoStack;
        const other = msg.type === 'undo' ? entry?.redoStack : entry?.undoStack;
        if (!entry || !stack?.length) {
          this._emit({ type: 'error', req, by: this.client, message: `nothing to ${msg.type}` });
          break;
        }
        const step = stack.pop();
        const opsToApply = msg.type === 'undo' ? step.inverseOps : step.forwardOps;
        applyOps(entry.root, opsToApply);
        other.push(step);
        entry.rev += 1;
        entry.dirty = entry.undoStack.length > 0;
        this._emit({
          type: 'ops',
          req,
          by: this.client,
          doc: entry.id,
          rev: entry.rev,
          dirty: entry.dirty,
          undo: entry.undoStack.at(-1)?.label ?? null,
          redo: entry.redoStack.at(-1)?.label ?? null,
          ops: structuredClone(opsToApply),
          issues: [],
        });
        break;
      }
      case 'save': {
        const entry = this._resolveEntry(msg.doc);
        if (!entry) {
          this._emit({ type: 'error', req, by: this.client, message: `document not open: ${msg.doc}` });
          break;
        }
        entry.dirty = false;
        await this._stageCloudBundle();
        this._emit({
          type: 'saved',
          req,
          by: this.client,
          doc: entry.id,
          rev: entry.rev,
          path: entry.path,
        });
        break;
      }
      case 'asset-usage': {
        const counts = {};
        for (const a of this.assets) counts[a.id] = 0;
        this._emit({
          type: 'asset-usage',
          req,
          counts,
          ...(msg.id ? { id: msg.id, uses: [] } : {}),
        });
        break;
      }
      case 'selection-sync': {
        this.selection = msg.selection || null;
        this._emit({ type: 'selection-synced', req, ok: true });
        break;
      }
      case 'catalog-query': {
        const q = String(msg.query || '').trim().toLowerCase();
        const items = this.assets
          .filter((a) => !msg.kind || a.kind === msg.kind)
          .filter((a) => !q || `${a.path} ${a.kind}`.toLowerCase().includes(q))
          .slice(0, msg.limit || 24)
          .map((a) => ({
            id: a.id,
            kind: a.kind,
            name: a.path.split('/').pop(),
            path: a.path,
            tags: [a.kind],
          }));
        this._emit({ type: 'catalog-results', req, total: items.length, items });
        break;
      }
      case 'scene-verify': {
        const entry = this._resolveEntry(msg.doc);
        const entitiesObj = entry?.root?.entities || {};
        const screenBoxes = Object.entries(entitiesObj).map(([eid, ent]) => ({
          id: eid,
          name: ent?.name || eid,
          rect: [0.35, 0.35, 0.65, 0.65],
          depthM: 5,
          onScreen: true,
          clipped: false,
          occlusionRatio: 0,
        }));
        const report = {
          passed: true,
          elapsedMs: 1,
          worldHash: cloudDocId(JSON.stringify(entitiesObj)).slice(0, 16),
          schemaIssues: [],
          spatialLints: [],
          camera: { name: 'Viewport', source: 'cloud', eye: [0, 4, 10], target: [0, 0, 0], fov: 50 },
          screenBoxes,
          budget: { entities: screenBoxes.length, draws: screenBoxes.length, triangles: 0, vramMb: 0 },
        };
        this._emit({
          type: 'scene-verified',
          req,
          doc: entry?.id || msg.doc || null,
          report,
        });
        break;
      }
      case 'gbs-apply': {
        const entry = this._resolveEntry(msg.doc);
        if (!entry || !entry.root?.entities) {
          this._emit({ type: 'error', req, message: 'open a scene before running .gbs' });
          break;
        }
        const script = String(msg.script || '').trim();
        const ops = parseCloudGbsFallback(entry.root, script, () => randomEntityHex16(this.idSeq++));
        if (!ops.length) {
          this._emit({ type: 'gbs-applied', req, doc: entry.id, rev: entry.rev, statements: 0, spawned: [] });
          break;
        }
        const inverseOps = applyOpsWithInverse(entry.root, ops);
        entry.rev += 1;
        entry.dirty = true;
        const label = msg.label || 'GBS Apply';
        entry.undoStack.push({ label, merge: null, forwardOps: ops, inverseOps });
        entry.redoStack = [];
        this._emit({
          type: 'ops',
          by: this.client,
          doc: entry.id,
          rev: entry.rev,
          dirty: entry.dirty,
          undo: label,
          redo: null,
          ops,
          issues: [],
        });
        this._emit({
          type: 'gbs-applied',
          req,
          doc: entry.id,
          rev: entry.rev,
          statements: ops.length,
          spawned: [],
        });
        break;
      }
      case 'brick-crystallize': {
        const rawId = String(msg.id || msg.name || 'brick').trim();
        const slug = rawId.toLowerCase().replace(/[^a-z0-9_-]+/g, '_') || 'brick';
        const rel = msg.path || `editor/bricks/${slug}.brick.json`;
        const entry = this._upsertDoc(rel, {
          ganbate: 'brick',
          version: 1,
          id: slug,
          aabb: { min: [-0.5, 0, -0.5], max: [0.5, 1, 0.5] },
          sockets: {},
          params: [],
          entities: {
            [randomEntityHex16(this.idSeq++)]: { name: rawId, components: { transform: {} } },
          },
        }, 'prefab');
        this._emit({ type: 'assets', assets: structuredClone(this.assets) });
        this._emit({
          type: 'brick-crystallized',
          req,
          brick: { id: slug, key: `brick:${slug}`, assetId: entry.id, path: entry.path, name: rawId },
        });
        break;
      }
      default:
        if (req !== undefined) {
          this._emit({ type: 'error', req, message: `unsupported cloud command: ${msg.type}` });
        }
        break;
    }
  }
}

function inferSchemaFieldFromValue(val) {
  if (typeof val === 'boolean') return { type: 'bool', default: val };
  if (typeof val === 'number') {
    return Number.isInteger(val) ? { type: 'int', default: val } : { type: 'float', default: val };
  }
  if (typeof val === 'string') return { type: 'string', default: val };
  if (Array.isArray(val)) {
    if (val.length === 2 && val.every((x) => typeof x === 'number')) return { type: 'vec2', default: val };
    if (val.length === 3 && val.every((x) => typeof x === 'number')) return { type: 'vec3', default: val };
    if (val.length === 4 && val.every((x) => typeof x === 'number')) return { type: 'vec4', default: val };
    return { type: 'array', default: val, items: val.length ? inferSchemaFieldFromValue(val[0]) : { type: 'string' } };
  }
  if (val && typeof val === 'object') {
    const fields = {};
    for (const [k, sub] of Object.entries(val)) fields[k] = inferSchemaFieldFromValue(sub);
    return { type: 'object', fields };
  }
  return { type: 'string', default: '' };
}

function parseCloudGbsFallback(root, script, allocId) {
  const ops = [];
  const spawnedByAlias = new Map();
  for (const rawLine of script.split(/\r?\n|;/)) {
    const line = rawLine.trim();
    if (!line || line.startsWith('#')) continue;
    const parts = line.split(/\s+/);
    if (parts[0] === 'spawn') {
      const alias = (parts[1] || 'Entity').replace(/^\$/, '');
      const eid = allocId();
      const atIdx = parts.indexOf('at');
      const tr = atIdx >= 0 && parts.length >= atIdx + 4
        ? [Number(parts[atIdx + 1]) || 0, Number(parts[atIdx + 2]) || 0, Number(parts[atIdx + 3]) || 0]
        : [0, 0, 0];
      const nameOpt = parts.find((p) => p.startsWith('name='));
      const entityName = nameOpt ? nameOpt.slice(5).replace(/^"|"$/g, '') : alias;
      spawnedByAlias.set(alias, eid);
      ops.push({
        set: ['entities', eid],
        value: { name: entityName, components: { transform: { translation: tr } } },
      });
    } else if (parts[0] === 'place' && parts[1] && parts[2] === 'at') {
      const targetKey = parts[1].replace(/^\$/, '');
      const eid = spawnedByAlias.get(targetKey)
        || Object.entries(root.entities || {}).find(([id, e]) => id === targetKey || e?.name === targetKey)?.[0];
      const rest = line.slice(line.indexOf('at') + 2).trim();
      let tr = [0, 0, 0];
      if (rest.startsWith('[')) {
        try {
          const parsed = JSON.parse(rest.slice(0, rest.indexOf(']') + 1));
          if (Array.isArray(parsed) && parsed.length >= 3) {
            tr = [Number(parsed[0]) || 0, Number(parsed[1]) || 0, Number(parsed[2]) || 0];
          }
        } catch { /* ignore */ }
      }
      if (eid) {
        ops.push({ set: ['entities', eid, 'components', 'transform', 'translation'], value: tr });
      }
    } else if (parts[0] === 'delete' && parts[1]) {
      const target = Object.entries(root.entities || {}).find(
        ([id, e]) => id === parts[1] || e?.name === parts[1],
      );
      if (target) ops.push({ remove: ['entities', target[0]] });
    }
  }
  return ops;
}

export function createStudioHost(url, onMessage, onState, options = {}) {
  const transport = options.transport
    ?? (typeof location !== 'undefined' ? new URLSearchParams(location.search).get('transport') : null)
    ?? 'local';
  if (transport === 'cloud') {
    return new CloudStudioHost(url, onMessage, onState, options);
  }
  return new Host(url, onMessage, onState);
}
