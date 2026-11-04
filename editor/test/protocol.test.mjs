import assert from 'node:assert/strict';
import { test } from 'node:test';
import { Host, CloudStudioHost, createStudioHost, cloudDocId, applyOps, applyOpsWithInverse } from '../protocol.js';

test('another editor’s edit or save with the same request number cannot settle my pending request', async () => {
  const old = globalThis.WebSocket;
  try {
    globalThis.WebSocket = class {
      send(json) { this.sent = JSON.parse(json); }
      message(msg) { this.onmessage({data: JSON.stringify(msg)}); }
    };
    const seen = [];
    const host = new Host('ws://localhost/', msg => seen.push(msg), () => {});
    const connected = host.connect();
    host.ws.onopen();
    await connected;
    host.ws.message({type: 'welcome', client: 5});
    for (const [operation, answer] of [['edit', 'ops'], ['save', 'saved']]) {
      const result = host.edit(() => ({type: operation, doc: 'style'}), msg => msg.type === answer);
      const { req } = host.ws.sent;
      host.ws.message({type: answer, doc: 'style', by: 8, req});
      assert.ok(host.waiting);
      host.ws.message({type: answer, doc: 'style', by: 5, req});
      assert.equal((await result).by, 5);
    }
    assert.equal(seen.length, 5);
  } finally { globalThis.WebSocket = old; }
});

test('project JSON keys stay data when applying and undoing Look edits', () => {
  const root = {};
  applyOps(root, [{set: ['__proto__', 'exposure'], value: 2}]);
  assert.equal(root.__proto__.exposure, 2);
  assert.equal(Object.getPrototypeOf(root), Object.prototype);
  assert.equal(Object.prototype.exposure, undefined);
  applyOps(root, [{remove: ['__proto__']}]);
  assert.deepEqual(root, {});
});

test('applyOpsWithInverse produces exact inverse ops for multi-step undo', () => {
  const root = { values: { alpha: 1, nested: { title: 'before' } } };
  const inv = applyOpsWithInverse(root, [
    { set: ['values', 'alpha'], value: 2 },
    { set: ['values', 'added'], value: [10, 20] },
    { remove: ['values', 'nested', 'title'] },
  ]);
  assert.equal(root.values.alpha, 2);
  assert.deepEqual(root.values.added, [10, 20]);
  assert.equal(root.values.nested.title, undefined);

  applyOps(root, inv);
  assert.deepEqual(root, { values: { alpha: 1, nested: { title: 'before' } } });
});

test('CloudStudioHost bootstraps live bundle, coalesces merged edits, supports undo/redo, stages saves, and runs M-AI fallbacks', async () => {
  const oldFetch = globalThis.fetch;
  try {
    let stagedPayload = null;
    globalThis.fetch = async (url, init = {}) => {
      const u = String(url);
      if (u.includes('/api/studio/live-bundle')) {
        return new Response(JSON.stringify({
          revision: 3,
          documents: {
            'content/data/hud.data.json': {
              ganbate: 'data',
              version: 1,
              id: '0123456789abcdef',
              schema: 'hud.anchors',
              name: 'Main HUD',
              values: { opacity: 0.9 },
            },
          },
          schemas: {
            'hud.anchors': { opacity: 0.9 },
          },
        }), { status: 200, headers: { 'content-type': 'application/json' } });
      }
      if (u.includes('/api/dev-studio/live-bundle') && init.method === 'PUT') {
        stagedPayload = JSON.parse(init.body);
        return new Response(JSON.stringify({ ok: true, revision: 3, hasDraft: true }), {
          status: 200,
          headers: { 'content-type': 'application/json' },
        });
      }
      return new Response('{}', { status: 404 });
    };

    const messages = [];
    const states = [];
    const host = createStudioHost('ws://localhost/api/ws', (m) => messages.push(m), (s) => states.push(s), {
      transport: 'cloud',
      sseUrl: null,
    });
    assert.ok(host instanceof CloudStudioHost);
    assert.equal(host.kind, 'cloud');

    await host.connect();
    await new Promise((r) => setTimeout(r, 10));
    assert.deepEqual(states, ['connected']);

    host.send({ type: 'hello', client: 'editor' });
    await new Promise((r) => setTimeout(r, 20));
    const welcome = messages.find((m) => m.type === 'welcome');
    assert.ok(welcome);
    const hudId = cloudDocId('content/data/hud.data.json');
    assert.equal(hudId.length, 32);
    assert.ok(welcome.assets.some((a) => a.id === hudId && a.kind === 'data'));

    host.send({ type: 'open', doc: hudId });
    await new Promise((r) => setTimeout(r, 10));
    const opened = messages.find((m) => m.type === 'doc' && m.doc === hudId);
    assert.equal(opened.root.values.opacity, 0.9);

    // Merged drag edits coalesce into a single undo entry
    await host.edit(() => ({
      type: 'edit',
      doc: hudId,
      rev: 1,
      label: 'Drag Opacity',
      merge: 'drag-1',
      ops: [{ set: ['values', 'opacity'], value: 0.7 }],
    }));
    await host.edit(() => ({
      type: 'edit',
      doc: hudId,
      rev: 2,
      label: 'Drag Opacity',
      merge: 'drag-1',
      ops: [{ set: ['values', 'opacity'], value: 0.5 }],
    }));
    assert.equal(host.getCachedDocRoot(hudId).values.opacity, 0.5);

    const undone = await host.edit(() => ({ type: 'undo', doc: hudId }));
    assert.equal(undone.type, 'ops');
    assert.equal(host.getCachedDocRoot(hudId).values.opacity, 0.9);

    const redone = await host.edit(() => ({ type: 'redo', doc: hudId }));
    assert.equal(redone.type, 'ops');
    assert.equal(host.getCachedDocRoot(hudId).values.opacity, 0.5);

    const saved = await host.edit(() => ({ type: 'save', doc: hudId }), (m) => m.type === 'saved');
    assert.equal(saved.type, 'saved');
    assert.equal(stagedPayload.documents['content/data/hud.data.json'].values.opacity, 0.5);

    // Create scene + run .gbs + verify scene
    host.send({ type: 'create', kind: 'scene', path: 'content/scenes/stage.scene.json', name: 'Stage' });
    await new Promise((r) => setTimeout(r, 10));
    const sceneCreated = messages.find((m) => m.type === 'created' && m.path === 'content/scenes/stage.scene.json');
    assert.ok(sceneCreated);

    const gbsReply = await host.request({
      type: 'gbs-apply',
      doc: sceneCreated.doc,
      script: 'spawn Beacon at 1 2 3',
    });
    assert.equal(gbsReply.type, 'gbs-applied');
    assert.equal(gbsReply.statements, 1);

    const verifyReply = await host.request({
      type: 'scene-verify',
      doc: sceneCreated.doc,
    });
    assert.equal(verifyReply.type, 'scene-verified');
    assert.equal(verifyReply.report.passed, true);
    assert.equal(verifyReply.report.screenBoxes.length, 1);

    host.disconnect();
  } finally {
    globalThis.fetch = oldFetch;
  }
});

