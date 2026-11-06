import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createLivePublish } from './live-publish.js';
import {
  BUILTIN_GAMEPLAY_TOOLS,
  resolveGameplayTools,
  resolveProjectTools,
  resolveTableLayoutTools,
} from './project-tools.js';
import { CloudStudioHost, cloudDocId, createStudioHost } from './protocol.js';

const tick = (ms = 15) => new Promise((resolve) => setTimeout(resolve, ms));

test('CloudStudioHost emits cooked worldUrl with composed TRS matrices & lamps on open, cook, and recook', async () => {
  const oldFetch = globalThis.fetch;
  try {
    globalThis.fetch = async (url) => {
      const u = String(url);
      if (u.includes('/ganbate-studio/cloud-manifest.json')) {
        return new Response(JSON.stringify({
          project: {
            id: 'ganbate-poker-cloud',
            name: 'Ganbate Live Studio',
            defaultWorldUrl: '/ganbate-worlds/blackjack-lounge.gbw',
          },
          bundle: {
            documents: {
              'content/scenes/lounge.scene.json': {
                ganbate: 'scene',
                version: 1,
                name: 'Lounge',
                entities: {
                  rig: {
                    name: 'Table Rig',
                    components: {
                      transform: { translation: [2, 1, -3], scale: [1, 1, 1] },
                    },
                  },
                  lamp_child: {
                    name: 'Key Spot',
                    parent: 'rig',
                    components: {
                      transform: { translation: [0.5, 4, 1.5] },
                      light: { color: [1, 0.95, 0.85], intensity: 6 },
                    },
                  },
                },
              },
              'content/scenes/vip.scene.json': {
                ganbate: 'scene',
                version: 1,
                name: 'VIP Room',
                worldUrl: '/ganbate-worlds/vip-room.gbw',
                entities: {
                  cam: {
                    name: 'VIP Camera',
                    components: {
                      transform: { translation: [0, 2.5, 6] },
                      camera: { fov: 50 },
                    },
                  },
                },
              },
            },
          },
        }), { status: 200, headers: { 'content-type': 'application/json' } });
      }
      return new Response('Bad Gateway', { status: 502, statusText: 'Bad Gateway' });
    };

    const messages = [];
    const host = createStudioHost('ws://localhost/api/ws', (m) => messages.push(m), () => {}, {
      transport: 'cloud',
    });
    assert.ok(host instanceof CloudStudioHost);

    await host.connect();
    host.send({ type: 'hello', client: 'editor' });
    await tick(25);

    const loungeId = cloudDocId('content/scenes/lounge.scene.json');
    const vipId = cloudDocId('content/scenes/vip.scene.json');

    // 1. Opening scene without explicit root.worldUrl falls back to project.defaultWorldUrl
    host.send({ type: 'open', doc: loungeId });
    await tick(15);

    const loungeDoc = messages.find((m) => m.type === 'doc' && m.doc === loungeId);
    const loungeCooked = messages.find((m) => m.type === 'cooked' && m.doc === loungeId);
    assert.ok(loungeDoc);
    assert.ok(loungeCooked);
    assert.equal(loungeCooked.worldUrl, '/ganbate-worlds/blackjack-lounge.gbw');
    assert.equal(loungeCooked.file, '/ganbate-worlds/blackjack-lounge.gbw');
    assert.deepEqual(loungeCooked.warnings, []);
    assert.equal(loungeCooked.entities.length, 2);

    const rigRec = loungeCooked.entities.find((e) => e.id === 'rig');
    const lampRec = loungeCooked.entities.find((e) => e.id === 'lamp_child');
    assert.ok(rigRec && lampRec);
    assert.equal(rigRec.transform.length, 16);
    assert.deepEqual(rigRec.transform.slice(12, 15), [2, 1, -3]);
    // Child world translation = [2 + 0.5, 1 + 4, -3 + 1.5] = [2.5, 5, -1.5]
    assert.deepEqual(lampRec.transform.slice(12, 15), [2.5, 5, -1.5]);
    assert.deepEqual(loungeCooked.lamps, [[2.5, 5, -1.5]]);

    // 2. Opening scene with explicit root.worldUrl overrides project.defaultWorldUrl
    host.send({ type: 'open', doc: vipId });
    await tick(15);
    const vipCooked = messages.find((m) => m.type === 'cooked' && m.doc === vipId);
    assert.ok(vipCooked);
    assert.equal(vipCooked.worldUrl, '/ganbate-worlds/vip-room.gbw');

    // 3. `cook` and `recook` re-emit `cooked` cleanly without `unsupported cloud command`
    const countBeforeCook = messages.filter((m) => m.type === 'cooked' && m.doc === vipId).length;
    host.send({ type: 'cook', doc: vipId });
    await tick(15);
    assert.equal(
      messages.filter((m) => m.type === 'cooked' && m.doc === vipId).length,
      countBeforeCook + 1,
    );

    const recookReply = await host.request({ type: 'recook', doc: loungeId });
    assert.equal(recookReply.type, 'cooked');
    assert.equal(recookReply.worldUrl, '/ganbate-worlds/blackjack-lounge.gbw');
    assert.equal(messages.some((m) => m.type === 'error'), false);

    host.disconnect();
  } finally {
    globalThis.fetch = oldFetch;
  }
});

test('CloudStudioHost skips SSE on 502 live-bundle and closes EventSource on pre-message error', async () => {
  const oldFetch = globalThis.fetch;
  const oldEventSource = globalThis.EventSource;
  const createdStreams = [];

  class MockEventSource {
    constructor(url, opts) {
      this.url = url;
      this.opts = opts;
      this.closed = false;
      this.listeners = new Map();
      this.onmessage = null;
      this.onerror = null;
      createdStreams.push(this);
    }

    addEventListener(type, fn) {
      const list = this.listeners.get(type) || [];
      list.push(fn);
      this.listeners.set(type, list);
    }

    close() {
      this.closed = true;
    }
  }

  try {
    globalThis.EventSource = MockEventSource;

    // Case A: `/api/studio/live-bundle` returns 502 -> EventSource is NEVER opened
    globalThis.fetch = async () => new Response('Bad Gateway', { status: 502, statusText: 'Bad Gateway' });

    const offlineHost = createStudioHost('ws://localhost/api/ws', () => {}, () => {}, {
      transport: 'cloud',
    });
    await offlineHost.connect();
    offlineHost.send({ type: 'hello', client: 'editor' });
    await tick(25);

    assert.equal(offlineHost._liveBundleReachable, false);
    assert.equal(createdStreams.length, 0);
    assert.equal(offlineHost.eventSource, null);
    offlineHost.disconnect();

    // Case B: `/api/studio/live-bundle` returns 200 -> EventSource opens; pre-message onerror closes it cleanly
    globalThis.fetch = async (url) => {
      if (String(url).includes('/api/studio/live-bundle')) {
        return new Response(JSON.stringify({ revision: 1, documents: {}, schemas: {} }), {
          status: 200,
          headers: { 'content-type': 'application/json' },
        });
      }
      return new Response('{}', { status: 404 });
    };

    const onlineHost = createStudioHost('ws://localhost/api/ws', () => {}, () => {}, {
      transport: 'cloud',
    });
    await onlineHost.connect();
    assert.equal(createdStreams.length, 0); // Not opened until live-bundle probe succeeds
    onlineHost.send({ type: 'hello', client: 'editor' });
    await tick(25);

    assert.equal(onlineHost._liveBundleReachable, true);
    assert.equal(createdStreams.length, 1);
    assert.equal(createdStreams[0].url, '/api/studio/live-changes');
    assert.equal(createdStreams[0].closed, false);

    // Simulate 502 on SSE before any message arrives -> closes immediately to stop browser retry spam
    createdStreams[0].onerror?.(new Event('error'));
    assert.equal(createdStreams[0].closed, true);
    assert.equal(onlineHost.eventSource, null);

    onlineHost.disconnect();
  } finally {
    globalThis.fetch = oldFetch;
    globalThis.EventSource = oldEventSource;
  }
});

test('createLivePublish setWorkspace degrades silently when backend is unreachable or returns 502', async () => {
  const oldFetch = globalThis.fetch;
  const oldDoc = globalThis.document;
  try {
    let fetchCalls = 0;
    globalThis.fetch = async () => {
      fetchCalls += 1;
      return new Response('Bad Gateway', { status: 502, statusText: 'Bad Gateway' });
    };
    globalThis.document = { getElementById: () => null };

    const logs = [];
    const unreachablePub = createLivePublish({
      el: () => ({}),
      t: (k) => k,
      getContext: () => ({}),
      collectBundle: async () => ({}),
      ensureDocumentOpen: async () => null,
      editDocument: async () => {},
      isBackendReachable: () => false,
      log: (lvl, msg) => logs.push([lvl, msg]),
    });

    unreachablePub.setWorkspace({
      liveRelease: { statusPath: '/api/dev-studio/live-release' },
    });
    await tick(15);
    assert.equal(fetchCalls, 0);
    assert.equal(logs.length, 0);

    const silent502Pub = createLivePublish({
      el: () => ({}),
      t: (k) => k,
      getContext: () => ({}),
      collectBundle: async () => ({}),
      ensureDocumentOpen: async () => null,
      editDocument: async () => {},
      isBackendReachable: () => true,
      log: (lvl, msg) => logs.push([lvl, msg]),
    });
    silent502Pub.setWorkspace({
      liveRelease: { statusPath: '/api/dev-studio/live-release' },
    });
    await tick(15);
    assert.equal(fetchCalls, 1);
    assert.equal(logs.length, 0);
  } finally {
    globalThis.fetch = oldFetch;
    globalThis.document = oldDoc;
  }
});

test('BUILTIN_GAMEPLAY_TOOLS align with /__studio-tool/<id> while preserving explicit manifest paths', () => {
  assert.equal(BUILTIN_GAMEPLAY_TOOLS.length, 4);
  for (const tool of BUILTIN_GAMEPLAY_TOOLS) {
    assert.equal(tool.path, `/__studio-tool/${tool.id}`);
    assert.equal(tool.open, 'embed');
  }

  // Cloud same-origin fallback with empty manifest synthesizes all 4 `/__studio-tool/*` embed routes
  const fallbackCloud = resolveGameplayTools([], 'https://ganbate.example', { allowSameOrigin: true });
  assert.equal(fallbackCloud.motionTuner?.embedHref, 'https://ganbate.example/__studio-tool/motion-tuner?embed=studio');
  assert.equal(fallbackCloud.actionSequencer?.embedHref, 'https://ganbate.example/__studio-tool/action-sequencer?embed=studio');
  assert.equal(fallbackCloud.godMode?.embedHref, 'https://ganbate.example/__studio-tool/god-mode?embed=studio');
  assert.equal(fallbackCloud.audioMixer?.embedHref, 'https://ganbate.example/__studio-tool/audio-mixer?embed=studio');

  // Explicit `cloudToolsManifest` paths take precedence while missing companions use `/__studio-tool/*`
  const cloudManifest = {
    version: 1,
    origin: 'same-origin',
    tools: [
      { id: 'layout-tuner', label: 'Table Layout', path: '/__studio-tool/layout-tuner', open: 'embed' },
      { id: 'motion-tuner', label: 'Custom Motion', path: '/custom-tools/motion-tuner', open: 'embed' },
    ],
  };
  const resolved = resolveProjectTools(cloudManifest, 'https://ganbate.example', null, { allowSameOrigin: true });
  const gp = resolveGameplayTools(resolved, 'https://ganbate.example', { allowSameOrigin: true });
  assert.equal(gp.motionTuner?.embedHref, 'https://ganbate.example/custom-tools/motion-tuner?embed=studio');
  assert.equal(gp.actionSequencer?.embedHref, 'https://ganbate.example/__studio-tool/action-sequencer?embed=studio');

  const tablePair = resolveTableLayoutTools(resolved);
  assert.equal(tablePair.layoutTuner?.embedHref, 'https://ganbate.example/__studio-tool/layout-tuner?embed=studio');
  assert.equal(
    tablePair.graphicsLayoutTuner?.embedHref,
    'https://ganbate.example/__studio-tool/graphics-layout-tuner?embed=studio',
  );
});
