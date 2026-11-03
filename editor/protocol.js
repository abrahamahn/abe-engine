// The edit protocol's client side (see crates/abe-studio/src/session.rs): one WebSocket to the
// host, messages as JSON objects with a `type`. Edits to a document, and the undo, redo and save
// that follow them, are sent one at a time, each after the host has answered the one before, so
// the host sees them in the order they were made and every edit carries the current revision.

/** What a waiting request gets when the connection is gone. */
const NOT_CONNECTED = { type: 'error', message: 'not connected to the studio host' };

export class Host {
  constructor(url, onMessage, onState) {
    this.url = url;
    this.onMessage = onMessage;
    this.onState = onState;
    this.queue = [];
    this.waiting = null;
    this.nextReq = 1;
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
        if (was) this.onState('disconnected');
        setTimeout(() => this.connect().catch(() => {}), this.retry);
        this.retry = Math.min(10000, this.retry * 2);
      };
      ws.onmessage = (e) => {
        const msg = JSON.parse(e.data);
        if (msg.type === 'welcome') this.client = msg.client;
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

/** Applies ops to a document root the way the host does (`abe_scene::doc`). */
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
