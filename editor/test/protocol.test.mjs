import assert from 'node:assert/strict';
import { test } from 'node:test';
import { Host, applyOps } from '../protocol.js';

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
