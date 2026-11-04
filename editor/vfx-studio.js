/**
 * VFX Studio workbench (`*.vfx.json`, schema `ganbate.vfx/1`).
 *
 * Genre-neutral authoring surface for deterministic 3D particle emitters:
 * - Preset Emitter Library (`blossom_drift`, `ember_updraft`, `radial_burst`, `mist_plume`)
 * - Multi-emitter stack with per-emitter enable & alpha/additive blend toggle
 * - Deterministic 1/60s fixed-step simulator + analytic 3D curl turbulence
 * - Live 2.5D `<canvas>` viewport preview & particle/overdraw budget HUD
 */

export const VFX_SCHEMA = 'ganbate.vfx/1';
const FIXED_DT = 1 / 60;

export const VFX_EMITTER_PRESETS = Object.freeze({
  blossom_drift: () => ({
    id: 'blossom_drift',
    enabled: true,
    blendMode: 'alpha',
    maxParticles: 160,
    spawnRate: 18,
    bursts: [],
    lifetimeS: [4.5, 7.0],
    shape: { kind: 'box', halfExtents: [3.5, 0.4, 2.5] },
    offset: [0, 3.2, 0],
    direction: [0.25, -0.9, 0.15],
    speed: [0.25, 0.55],
    gravity: [0.08, -0.32, 0.03],
    drag: 0.85,
    curlNoise: { amplitude: 0.55, frequency: 0.65, scrollSpeed: 0.35 },
    initialRotationDeg: [-180, 180],
    angularVelocityDeg: [-95, 95],
    sizeBase: [0.028, 0.048],
    sizeOverLife: [
      { t: 0, value: 0.6 },
      { t: 0.2, value: 1.0 },
      { t: 0.85, value: 0.95 },
      { t: 1.0, value: 0.3 },
    ],
    colorOverLife: [
      { t: 0, rgba: [0.99, 0.82, 0.88, 0.0] },
      { t: 0.12, rgba: [0.99, 0.82, 0.88, 0.92] },
      { t: 0.85, rgba: [0.96, 0.72, 0.82, 0.82] },
      { t: 1.0, rgba: [0.92, 0.64, 0.76, 0.0] },
    ],
  }),
  ember_updraft: () => ({
    id: 'ember_updraft',
    enabled: true,
    blendMode: 'additive',
    maxParticles: 96,
    spawnRate: 22,
    bursts: [],
    lifetimeS: [1.6, 3.0],
    shape: { kind: 'disc', radius: 0.35 },
    offset: [0, 0, 0],
    direction: [0, 1, 0],
    speed: [0.45, 1.15],
    gravity: [0, 0.45, 0],
    drag: 0.4,
    curlNoise: { amplitude: 0.75, frequency: 1.35, scrollSpeed: 0.8 },
    initialRotationDeg: [0, 360],
    angularVelocityDeg: [-140, 140],
    sizeBase: [0.012, 0.026],
    sizeOverLife: [
      { t: 0, value: 0.5 },
      { t: 0.25, value: 1.0 },
      { t: 1.0, value: 0 },
    ],
    colorOverLife: [
      { t: 0, rgba: [1.0, 0.92, 0.58, 0.0] },
      { t: 0.1, rgba: [1.0, 0.78, 0.32, 0.95] },
      { t: 0.7, rgba: [0.96, 0.38, 0.12, 0.7] },
      { t: 1.0, rgba: [0.55, 0.12, 0.04, 0.0] },
    ],
  }),
  radial_burst: () => ({
    id: 'radial_burst',
    enabled: true,
    blendMode: 'additive',
    maxParticles: 120,
    spawnRate: 0,
    bursts: [{ timeS: 0, count: 72 }],
    lifetimeS: [0.65, 1.35],
    shape: { kind: 'cone', angleDeg: 58, radius: 0.08 },
    offset: [0, 0.05, 0],
    direction: [0, 1, 0],
    speed: [1.8, 3.8],
    gravity: [0, -4.8, 0],
    drag: 0.9,
    curlNoise: { amplitude: 0.3, frequency: 1.8, scrollSpeed: 0.6 },
    initialRotationDeg: [-180, 180],
    angularVelocityDeg: [-240, 240],
    sizeBase: [0.018, 0.038],
    sizeOverLife: [
      { t: 0, value: 1.0 },
      { t: 0.5, value: 0.8 },
      { t: 1.0, value: 0.0 },
    ],
    colorOverLife: [
      { t: 0, rgba: [1.0, 0.96, 0.78, 1.0] },
      { t: 0.45, rgba: [0.96, 0.76, 0.28, 0.9] },
      { t: 1.0, rgba: [0.85, 0.42, 0.14, 0.0] },
    ],
  }),
  mist_plume: () => ({
    id: 'mist_plume',
    enabled: true,
    blendMode: 'alpha',
    maxParticles: 64,
    spawnRate: 8,
    bursts: [],
    lifetimeS: [3.0, 5.0],
    shape: { kind: 'sphere', radius: 0.45 },
    offset: [0, 0.2, 0],
    direction: [0, 0.4, 0],
    speed: [0.08, 0.22],
    gravity: [0, 0.04, 0],
    drag: 1.2,
    curlNoise: { amplitude: 0.22, frequency: 0.45, scrollSpeed: 0.18 },
    initialRotationDeg: [-180, 180],
    angularVelocityDeg: [-18, 18],
    sizeBase: [0.22, 0.42],
    sizeOverLife: [
      { t: 0, value: 0.5 },
      { t: 0.5, value: 1.0 },
      { t: 1.0, value: 1.35 },
    ],
    colorOverLife: [
      { t: 0, rgba: [0.76, 0.82, 0.92, 0.0] },
      { t: 0.3, rgba: [0.76, 0.82, 0.92, 0.22] },
      { t: 1.0, rgba: [0.70, 0.76, 0.88, 0.0] },
    ],
  }),
});

export function createDefaultVfxDocument(id = 'ambient_canopy', name = 'Ambient Canopy') {
  return {
    schema: VFX_SCHEMA,
    id,
    name,
    seed: 42,
    durationS: 5.0,
    looping: true,
    warmupS: 0.5,
    budgetMaxParticles: 320,
    emitters: [VFX_EMITTER_PRESETS.blossom_drift(), VFX_EMITTER_PRESETS.ember_updraft()],
  };
}

export function normalizeVfxDocument(raw) {
  const fallback = createDefaultVfxDocument();
  if (!raw || typeof raw !== 'object') return fallback;
  return {
    schema: VFX_SCHEMA,
    id: String(raw.id ?? fallback.id),
    name: String(raw.name ?? raw.id ?? fallback.name),
    seed: Math.max(0, Math.floor(Number(raw.seed ?? 1))),
    durationS: Math.max(0.1, Number(raw.durationS ?? 4.0)),
    looping: raw.looping !== false,
    warmupS: Math.max(0, Number(raw.warmupS ?? 0)),
    budgetMaxParticles: Math.max(1, Math.floor(Number(raw.budgetMaxParticles ?? 512))),
    emitters: Array.isArray(raw.emitters) && raw.emitters.length
      ? raw.emitters.map((e, idx) => ({
          id: String(e.id ?? `emitter_${idx}`),
          enabled: e.enabled !== false,
          blendMode: e.blendMode === 'additive' ? 'additive' : 'alpha',
          maxParticles: Math.max(1, Math.floor(Number(e.maxParticles ?? 128))),
          spawnRate: Math.max(0, Number(e.spawnRate ?? 0)),
          bursts: Array.isArray(e.bursts) ? e.bursts.map((b) => ({ timeS: Number(b.timeS ?? 0), count: Number(b.count ?? 0) })) : [],
          lifetimeS: Array.isArray(e.lifetimeS) ? [...e.lifetimeS] : [1, 2],
          shape: e.shape ? { ...e.shape } : { kind: 'point' },
          offset: Array.isArray(e.offset) ? [...e.offset] : [0, 0, 0],
          direction: Array.isArray(e.direction) ? [...e.direction] : [0, 1, 0],
          speed: Array.isArray(e.speed) ? [...e.speed] : [0.5, 1.0],
          gravity: Array.isArray(e.gravity) ? [...e.gravity] : [0, 0, 0],
          drag: Math.max(0, Number(e.drag ?? 0)),
          curlNoise: {
            amplitude: Number(e.curlNoise?.amplitude ?? 0),
            frequency: Number(e.curlNoise?.frequency ?? 1),
            scrollSpeed: Number(e.curlNoise?.scrollSpeed ?? 0.4),
          },
          initialRotationDeg: Array.isArray(e.initialRotationDeg) ? [...e.initialRotationDeg] : [-180, 180],
          angularVelocityDeg: Array.isArray(e.angularVelocityDeg) ? [...e.angularVelocityDeg] : [0, 0],
          sizeBase: Array.isArray(e.sizeBase) ? [...e.sizeBase] : [0.03, 0.05],
          sizeOverLife: Array.isArray(e.sizeOverLife) ? e.sizeOverLife.map((p) => ({ ...p })) : [{ t: 0, value: 1 }, { t: 1, value: 0 }],
          colorOverLife: Array.isArray(e.colorOverLife)
            ? e.colorOverLife.map((c) => ({ t: Number(c.t ?? 0), rgba: [...(c.rgba ?? [1, 1, 1, 1])] }))
            : [{ t: 0, rgba: [1, 1, 1, 1] }, { t: 1, rgba: [1, 1, 1, 0] }],
        }))
      : fallback.emitters,
  };
}

function makePrng(seed) {
  let state = (Number(seed) >>> 0) ^ 0x9e3779b9;
  const nextU32 = () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let t = Math.imul(state ^ (state >>> 15), 1 | state);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return (t ^ (t >>> 14)) >>> 0;
  };
  const nextF = () => nextU32() / 4294967296;
  return {
    nextF,
    range: (min, max) => min + (max - min) * nextF(),
  };
}

function curlField(pos, freq, timeS, scroll) {
  const f = Math.max(1e-4, freq);
  const t = timeS * scroll;
  const x = pos[0] * f + t * 0.37;
  const y = pos[1] * f + t * 0.73;
  const z = pos[2] * f - t * 0.29;
  return [
    Math.cos(y * 2.1 + z * 1.3) - Math.sin(z * 1.9 - x * 0.7),
    Math.cos(z * 2.3 + x * 1.1) - Math.sin(x * 1.7 - y * 0.9),
    Math.cos(x * 1.9 + y * 1.5) - Math.sin(y * 2.1 - z * 0.8),
  ];
}

function sampleScalar(points, t) {
  if (!points?.length) return 1;
  const u = Math.min(1, Math.max(0, t));
  if (u <= points[0].t) return points[0].value;
  for (let i = 0; i < points.length - 1; i++) {
    const a = points[i];
    const b = points[i + 1];
    if (u <= b.t) {
      const k = (u - a.t) / Math.max(1e-6, b.t - a.t);
      return a.value + (b.value - a.value) * k;
    }
  }
  return points[points.length - 1].value;
}

function sampleColor(stops, t) {
  if (!stops?.length) return [1, 1, 1, 1];
  const u = Math.min(1, Math.max(0, t));
  if (u <= stops[0].t) return [...stops[0].rgba];
  for (let i = 0; i < stops.length - 1; i++) {
    const a = stops[i];
    const b = stops[i + 1];
    if (u <= b.t) {
      const k = (u - a.t) / Math.max(1e-6, b.t - a.t);
      return [
        a.rgba[0] + (b.rgba[0] - a.rgba[0]) * k,
        a.rgba[1] + (b.rgba[1] - a.rgba[1]) * k,
        a.rgba[2] + (b.rgba[2] - a.rgba[2]) * k,
        a.rgba[3] + (b.rgba[3] - a.rgba[3]) * k,
      ];
    }
  }
  return [...stops[stops.length - 1].rgba];
}

function normalize3(v) {
  const len = Math.hypot(v[0], v[1], v[2]);
  return len > 1e-6 ? [v[0] / len, v[1] / len, v[2] / len] : [0, 1, 0];
}

export function simulateVfxAt(rawDoc, targetTimeS = 0) {
  const doc = normalizeVfxDocument(rawDoc);
  const rng = makePrng(doc.seed);
  const particles = [];
  const states = doc.emitters.map((e) => ({
    accum: 0,
    fired: new Array(e.bursts.length).fill(false),
  }));
  let simTime = 0;

  const spawnOne = (eIdx) => {
    if (particles.length >= doc.budgetMaxParticles) return;
    const spec = doc.emitters[eIdx];
    const ownCount = particles.filter((p) => p.emitterIndex === eIdx).length;
    if (ownCount >= spec.maxParticles) return;

    const dir = normalize3(spec.direction);
    let local = [0, 0, 0];
    let velDir = dir;
    if (spec.shape.kind === 'box') {
      const he = spec.shape.halfExtents ?? [0.5, 0.5, 0.5];
      local = [rng.range(-he[0], he[0]), rng.range(-he[1], he[1]), rng.range(-he[2], he[2])];
    } else if (spec.shape.kind === 'sphere' || spec.shape.kind === 'disc') {
      const r = Number(spec.shape.radius ?? 0.3) * Math.sqrt(rng.nextF());
      const th = rng.range(0, Math.PI * 2);
      local = [Math.cos(th) * r, spec.shape.kind === 'sphere' ? rng.range(-r, r) : 0, Math.sin(th) * r];
    } else if (spec.shape.kind === 'cone') {
      const spread = ((spec.shape.angleDeg ?? 35) * 0.5 * Math.PI) / 180;
      velDir = normalize3([
        dir[0] + rng.range(-spread, spread),
        dir[1] + rng.range(-spread * 0.5, spread * 0.5),
        dir[2] + rng.range(-spread, spread),
      ]);
    }
    const spd = rng.range(spec.speed[0], spec.speed[1]);
    const life = Math.max(0.01, rng.range(spec.lifetimeS[0], spec.lifetimeS[1]));
    const sizeBase = rng.range(spec.sizeBase[0], spec.sizeBase[1]);
    particles.push({
      emitterIndex: eIdx,
      blendMode: spec.blendMode,
      position: [spec.offset[0] + local[0], spec.offset[1] + local[1], spec.offset[2] + local[2]],
      velocity: [velDir[0] * spd, velDir[1] * spd, velDir[2] * spd],
      ageS: 0,
      lifetimeS: life,
      sizeBase,
      size: sizeBase * sampleScalar(spec.sizeOverLife, 0),
      rgba: sampleColor(spec.colorOverLife, 0),
    });
  };

  const fireBursts = (cycleT) => {
    doc.emitters.forEach((spec, eIdx) => {
      if (!spec.enabled) return;
      spec.bursts.forEach((b, bIdx) => {
        if (!states[eIdx].fired[bIdx] && cycleT >= b.timeS) {
          states[eIdx].fired[bIdx] = true;
          for (let i = 0; i < b.count; i++) spawnOne(eIdx);
        }
      });
    });
  };

  const step = (dt) => {
    const dur = Math.max(0.01, doc.durationS);
    const prevCycle = Math.floor(simTime / dur);
    simTime += dt;
    const nextCycle = Math.floor(simTime / dur);
    if (doc.looping && nextCycle > prevCycle) {
      for (const st of states) st.fired.fill(false);
    }
    const cycleT = doc.looping ? simTime % dur : simTime;
    if (doc.looping || simTime <= dur) {
      fireBursts(cycleT);
      doc.emitters.forEach((spec, eIdx) => {
        if (!spec.enabled || spec.spawnRate <= 0) return;
        states[eIdx].accum += spec.spawnRate * dt;
        const cnt = Math.floor(states[eIdx].accum);
        states[eIdx].accum -= cnt;
        for (let i = 0; i < cnt; i++) spawnOne(eIdx);
      });
    }
    for (let i = particles.length - 1; i >= 0; i--) {
      const p = particles[i];
      p.ageS += dt;
      if (p.ageS >= p.lifetimeS) {
        particles.splice(i, 1);
        continue;
      }
      const spec = doc.emitters[p.emitterIndex];
      const damp = Math.min(1, Math.max(0, 1 - spec.drag * dt));
      const curl = spec.curlNoise.amplitude
        ? curlField(p.position, spec.curlNoise.frequency, simTime, spec.curlNoise.scrollSpeed)
        : [0, 0, 0];
      p.velocity[0] = (p.velocity[0] + (spec.gravity[0] + curl[0] * spec.curlNoise.amplitude) * dt) * damp;
      p.velocity[1] = (p.velocity[1] + (spec.gravity[1] + curl[1] * spec.curlNoise.amplitude) * dt) * damp;
      p.velocity[2] = (p.velocity[2] + (spec.gravity[2] + curl[2] * spec.curlNoise.amplitude) * dt) * damp;
      p.position[0] += p.velocity[0] * dt;
      p.position[1] += p.velocity[1] * dt;
      p.position[2] += p.velocity[2] * dt;
      const u = p.ageS / p.lifetimeS;
      p.size = p.sizeBase * sampleScalar(spec.sizeOverLife, u);
      p.rgba = sampleColor(spec.colorOverLife, u);
    }
  };

  const advance = (total) => {
    let rem = Math.max(0, total);
    while (rem > FIXED_DT) {
      step(FIXED_DT);
      rem -= FIXED_DT;
    }
    if (rem > 1e-6) step(rem);
  };

  if (doc.warmupS > 0) {
    advance(doc.warmupS);
    simTime = 0;
    for (const st of states) st.fired.fill(false);
  }
  fireBursts(0);
  if (targetTimeS > 0) advance(targetTimeS);

  let additiveParticles = 0;
  let alphaParticles = 0;
  let overdraw = 0;
  for (const p of particles) {
    if (p.blendMode === 'additive') {
      additiveParticles++;
      overdraw += p.size * p.size * 1.35;
    } else {
      alphaParticles++;
      overdraw += p.size * p.size;
    }
  }

  return {
    particles,
    stats: {
      activeParticles: particles.length,
      additiveParticles,
      alphaParticles,
      estimatedOverdrawScore: Number(overdraw.toFixed(4)),
      withinBudget: particles.length <= doc.budgetMaxParticles,
    },
  };
}

export function buildAddPresetEmitterOps(doc, presetId) {
  const factory = VFX_EMITTER_PRESETS[presetId] ?? VFX_EMITTER_PRESETS.blossom_drift;
  const spec = factory();
  const nextIndex = doc.emitters?.length ?? 0;
  spec.id = `${presetId}_${nextIndex}`;
  return [{ set: ['emitters', String(nextIndex)], value: spec }];
}

export function buildEmitterFieldOps(emitterIndex, field, value) {
  return [{ set: ['emitters', String(emitterIndex), field], value }];
}

export function buildCurlNoiseOps(emitterIndex, patch) {
  return Object.entries(patch).map(([k, v]) => ({
    set: ['emitters', String(emitterIndex), 'curlNoise', k],
    value: Number(v),
  }));
}

function applyOpLocally(root, op) {
  if (op.set && op.set.length) {
    let cur = root;
    for (let i = 0; i < op.set.length - 1; i++) {
      const k = op.set[i];
      if (cur[k] == null || typeof cur[k] !== 'object') cur[k] = {};
      cur = cur[k];
    }
    cur[op.set[op.set.length - 1]] = op.value;
  }
}

export function mountVfxStudio(container, studioApi = {}) {
  const dom = container.ownerDocument || globalThis.document;
  let activeDocId = null;
  let docData = createDefaultVfxDocument();
  let selectedEmitter = 0;
  let scrubTimeS = 1.2;
  let active = true;
  let destroyed = false;
  const unsubscribers = [];

  const syncFromStudio = () => {
    const activeDoc = studioApi.getActiveDocument?.();
    if (activeDoc && (String(activeDoc.path ?? activeDoc.id ?? '').endsWith('.vfx.json') ||
        activeDoc.root?.schema === VFX_SCHEMA)) {
      activeDocId = activeDoc.id ?? activeDoc.path;
      docData = normalizeVfxDocument(activeDoc.root ?? activeDoc.data ?? activeDoc);
      if (selectedEmitter >= docData.emitters.length) selectedEmitter = 0;
    }
  };

  const commitOps = (ops, label, merge = false) => {
    if (!ops.length) return;
    for (const op of ops) applyOpLocally(docData, op);
    docData = normalizeVfxDocument(docData);
    if (activeDocId && typeof studioApi.edit === 'function') {
      studioApi.edit(activeDocId, ops, label, merge);
    }
    render();
  };

  const root = dom.createElement('div');
  root.className = 'gb-vfx-studio';
  container.innerHTML = '';
  container.appendChild(root);

  const el = (tag, className, text) => {
    const node = dom.createElement(tag);
    if (className) node.className = className;
    if (text != null) node.textContent = text;
    return node;
  };

  const paintPreviewCanvas = (canvas, sim) => {
    const ctx = canvas.getContext?.('2d');
    if (!ctx) return;
    const w = (canvas.width = 420);
    const h = (canvas.height = 240);
    ctx.clearRect(0, 0, w, h);
    for (const p of sim.particles) {
      const depth = Math.max(0.6, 3.2 - p.position[2] * 0.35);
      const sx = w * 0.5 + (p.position[0] / depth) * 150;
      const sy = h * 0.72 - (p.position[1] / depth) * 150;
      const rad = Math.max(1.5, (p.size / depth) * 220);
      const [r, g, b, a] = p.rgba;
      ctx.fillStyle = `rgba(${Math.round(r * 255)}, ${Math.round(g * 255)}, ${Math.round(b * 255)}, ${a.toFixed(3)})`;
      ctx.beginPath();
      ctx.arc(sx, sy, rad, 0, Math.PI * 2);
      ctx.fill();
    }
  };

  const render = () => {
    if (destroyed) return;
    root.innerHTML = '';
    const sim = simulateVfxAt(docData, scrubTimeS);

    const toolbar = el('div', 'gb-vfx-toolbar');
    toolbar.appendChild(el('span', 'gb-vfx-title', 'VFX Studio'));
    toolbar.appendChild(el('span', '', `${docData.name} (seed ${docData.seed})`));
    toolbar.appendChild(el('div', 'gb-vfx-spacer'));

    const hud = el(
      'span',
      `gb-vfx-hud${sim.stats.withinBudget ? '' : ' is-over'}`,
      `${sim.stats.activeParticles}/${docData.budgetMaxParticles} pts · OD ${sim.stats.estimatedOverdrawScore.toFixed(2)}`,
    );
    hud.dataset.role = 'vfx-budget-hud';
    toolbar.appendChild(hud);

    const saveBtn = el('button', 'gb-vfx-btn', 'Save');
    saveBtn.dataset.role = 'vfx-save';
    saveBtn.addEventListener('click', () => {
      if (activeDocId) studioApi.save?.(activeDocId);
    });
    toolbar.appendChild(saveBtn);
    root.appendChild(toolbar);

    const grid = el('div', 'gb-vfx-grid');

    // Left col: Preset library + Emitter stack
    const leftCol = el('div', 'gb-vfx-col');
    const presetCard = el('div', 'gb-vfx-card');
    presetCard.appendChild(el('div', 'gb-vfx-title', 'Add Emitter Preset'));
    for (const presetId of Object.keys(VFX_EMITTER_PRESETS)) {
      const btn = el('button', 'gb-vfx-btn', `+ ${presetId}`);
      btn.dataset.preset = presetId;
      btn.addEventListener('click', () => {
        commitOps(buildAddPresetEmitterOps(docData, presetId), `Add emitter ${presetId}`);
      });
      presetCard.appendChild(btn);
    }
    leftCol.appendChild(presetCard);

    const stackCard = el('div', 'gb-vfx-card');
    stackCard.appendChild(el('div', 'gb-vfx-title', `Emitters (${docData.emitters.length})`));
    docData.emitters.forEach((em, idx) => {
      const btn = el(
        'button',
        `gb-vfx-btn${idx === selectedEmitter ? ' is-active' : ''}`,
        `${em.enabled ? '●' : '○'} ${em.id} [${em.blendMode}]`,
      );
      btn.dataset.emitterIndex = String(idx);
      btn.addEventListener('click', () => {
        selectedEmitter = idx;
        render();
      });
      stackCard.appendChild(btn);
    });
    leftCol.appendChild(stackCard);
    grid.appendChild(leftCol);

    // Center col: Canvas preview + Time scrubber
    const centerCol = el('div', 'gb-vfx-col');
    const previewCard = el('div', 'gb-vfx-card');
    const canvas = el('canvas', 'gb-vfx-canvas');
    canvas.dataset.role = 'vfx-canvas';
    paintPreviewCanvas(canvas, sim);
    previewCard.appendChild(canvas);

    const scrubRow = el('div', 'gb-vfx-row');
    scrubRow.appendChild(el('span', '', 'Scrub Time'));
    const scrub = el('input');
    scrub.type = 'range';
    scrub.min = '0';
    scrub.max = String(docData.durationS);
    scrub.step = '0.02';
    scrub.value = String(scrubTimeS);
    scrub.dataset.role = 'vfx-scrubber';
    scrub.addEventListener('input', (ev) => {
      scrubTimeS = Number(ev.target.value);
      render();
    });
    scrubRow.appendChild(scrub);
    scrubRow.appendChild(el('span', 'gb-vfx-val', `${scrubTimeS.toFixed(2)}s`));
    previewCard.appendChild(scrubRow);
    centerCol.appendChild(previewCard);
    grid.appendChild(centerCol);

    // Right col: Emitter Inspector
    const rightCol = el('div', 'gb-vfx-col');
    const em = docData.emitters[selectedEmitter];
    if (em) {
      const inspCard = el('div', 'gb-vfx-card');
      inspCard.appendChild(el('div', 'gb-vfx-title', `Inspector: ${em.id}`));

      const enableBtn = el('button', `gb-vfx-btn${em.enabled ? ' is-active' : ''}`, em.enabled ? 'Enabled' : 'Disabled');
      enableBtn.dataset.role = 'emitter-enable';
      enableBtn.addEventListener('click', () => {
        commitOps(buildEmitterFieldOps(selectedEmitter, 'enabled', !em.enabled), `Toggle ${em.id}`);
      });
      inspCard.appendChild(enableBtn);

      const blendSelect = el('select', 'gb-vfx-select');
      blendSelect.dataset.role = 'emitter-blend';
      for (const mode of ['alpha', 'additive']) {
        const opt = el('option', '', mode);
        opt.value = mode;
        if (mode === em.blendMode) opt.selected = true;
        blendSelect.appendChild(opt);
      }
      blendSelect.addEventListener('change', (ev) => {
        commitOps(buildEmitterFieldOps(selectedEmitter, 'blendMode', ev.target.value), `Blend mode ${em.id}`);
      });
      inspCard.appendChild(blendSelect);

      const rateRow = el('div', 'gb-vfx-row');
      rateRow.appendChild(el('span', '', 'Spawn Rate'));
      const rateIn = el('input');
      rateIn.type = 'range';
      rateIn.min = '0';
      rateIn.max = '120';
      rateIn.step = '1';
      rateIn.value = String(em.spawnRate);
      rateIn.dataset.role = 'emitter-spawn-rate';
      rateIn.addEventListener('input', (ev) => {
        commitOps(buildEmitterFieldOps(selectedEmitter, 'spawnRate', Number(ev.target.value)), `Spawn rate ${em.id}`, true);
      });
      rateRow.appendChild(rateIn);
      rateRow.appendChild(el('span', 'gb-vfx-val', `${em.spawnRate}/s`));
      inspCard.appendChild(rateRow);

      const curlRow = el('div', 'gb-vfx-row');
      curlRow.appendChild(el('span', '', 'Curl Amp'));
      const curlIn = el('input');
      curlIn.type = 'range';
      curlIn.min = '0';
      curlIn.max = '3';
      curlIn.step = '0.05';
      curlIn.value = String(em.curlNoise.amplitude);
      curlIn.dataset.role = 'emitter-curl-amp';
      curlIn.addEventListener('input', (ev) => {
        commitOps(buildCurlNoiseOps(selectedEmitter, { amplitude: Number(ev.target.value) }), `Curl noise ${em.id}`, true);
      });
      curlRow.appendChild(curlIn);
      curlRow.appendChild(el('span', 'gb-vfx-val', em.curlNoise.amplitude.toFixed(2)));
      inspCard.appendChild(curlRow);

      rightCol.appendChild(inspCard);
    }
    grid.appendChild(rightCol);
    root.appendChild(grid);
  };

  syncFromStudio();
  render();

  if (typeof studioApi.subscribe === 'function') {
    const unsub = studioApi.subscribe(() => {
      if (!active || destroyed) return;
      syncFromStudio();
      render();
    });
    if (typeof unsub === 'function') unsubscribers.push(unsub);
  }

  const destroy = () => {
    if (destroyed) return;
    destroyed = true;
    while (unsubscribers.length) {
      const fn = unsubscribers.pop();
      try {
        fn();
      } catch {
        // Ignore teardown errors.
      }
    }
    container.innerHTML = '';
  };

  return {
    destroy,
    unmount: destroy,
    refresh() {
      syncFromStudio();
      render();
    },
    setDocument(pathOrId, rawDoc) {
      activeDocId = pathOrId;
      docData = normalizeVfxDocument(rawDoc?.root ?? rawDoc);
      selectedEmitter = 0;
      render();
    },
    onActivate() {
      active = true;
      syncFromStudio();
      render();
    },
    onDeactivate() {
      active = false;
    },
  };
}
