/**
 * Sequencer Studio workbench (`*.seq.json`, schema `ganbate.sequence/1`).
 *
 * Genre-neutral deterministic multi-track timeline synchronizing:
 * - Camera cuts & dolly rails (`camera`)
 * - Skeletal animation clips (`skeletal`)
 * - Parametric Bezier / Spring 3D prop transforms (`transform`)
 * - Particle VFX triggers (`vfx`)
 * - Event-driven audio cues (`audio`)
 */

export const SEQUENCE_SCHEMA = 'ganbate.sequence/1';

function cubicBezierAxis(u, p1, p2) {
  const inv = 1 - u;
  return 3 * inv * inv * u * p1 + 3 * inv * u * u * p2 + u * u * u;
}

export function sampleSequenceCurve(curve, progress, durationS = 1) {
  if (!Number.isFinite(progress) || progress <= 0) return 0;
  if (progress >= 1) return 1;
  const t = Math.min(1, Math.max(0, progress));
  const kind = typeof curve === 'string' ? curve : curve?.type ?? 'smoothstep';
  if (kind === 'linear') return t;
  if (kind === 'easeOutQuart') {
    const inv = 1 - t;
    return 1 - inv * inv * inv * inv;
  }
  if (kind === 'cubicBezier') {
    const x1 = Number(curve?.x1 ?? 0.25);
    const y1 = Number(curve?.y1 ?? 0.1);
    const x2 = Number(curve?.x2 ?? 0.25);
    const y2 = Number(curve?.y2 ?? 1.0);
    let low = 0;
    let high = 1;
    for (let i = 0; i < 20; i++) {
      const mid = 0.5 * (low + high);
      if (cubicBezierAxis(mid, x1, x2) < t) low = mid;
      else high = mid;
    }
    return cubicBezierAxis(0.5 * (low + high), y1, y2);
  }
  if (kind === 'spring') {
    const dur = Number.isFinite(durationS) && durationS > 1e-4 ? durationS : 1;
    const sec = t * dur;
    const m = Math.max(1e-4, Number(curve?.mass ?? 1));
    const k = Math.max(1e-4, Number(curve?.stiffness ?? 240));
    const c = Math.max(0, Number(curve?.damping ?? 22));
    const omega0 = Math.sqrt(k / m);
    const zeta = c / (2 * Math.sqrt(k * m));
    let raw;
    if (zeta < 1 - 1e-4) {
      const omegad = omega0 * Math.sqrt(1 - zeta * zeta);
      const env = Math.exp(-zeta * omega0 * sec);
      const ratio = (zeta * omega0) / omegad;
      raw = 1 - env * (Math.cos(omegad * sec) + ratio * Math.sin(omegad * sec));
    } else if (zeta <= 1 + 1e-4) {
      const env = Math.exp(-omega0 * sec);
      raw = 1 - env * (1 + omega0 * sec);
    } else {
      const s = Math.sqrt(zeta * zeta - 1);
      const r1 = -omega0 * (zeta - s);
      const r2 = -omega0 * (zeta + s);
      const denom = Math.max(1e-6, r1 - r2);
      raw = 1 - (r1 * Math.exp(r2 * sec) - r2 * Math.exp(r1 * sec)) / denom;
    }
    if (t > 0.9) {
      const tail = (t - 0.9) / 0.1;
      const blend = tail * tail * (3 - 2 * tail);
      return raw + (1 - raw) * blend;
    }
    return raw;
  }
  // smoothstep default
  return t * t * (3 - 2 * t);
}

export function snapMsToFrame(timeMs, fps = 60) {
  const safeFps = Math.max(1, Number(fps) || 60);
  const frameDurationMs = 1000 / safeFps;
  const frameIndex = Math.round(Math.max(0, Number(timeMs || 0)) / frameDurationMs);
  return Math.max(0, Math.round(frameIndex * frameDurationMs));
}

export function msToFrameLabel(timeMs, fps = 60) {
  const safeFps = Math.max(1, Number(fps) || 60);
  const frame = Math.floor((Math.max(0, Number(timeMs || 0)) / 1000) * safeFps);
  return `${Math.round(timeMs)}ms · f${frame}`;
}

function lerp(a, b, t) {
  return a + (b - a) * t;
}

function lerp3(a, b, t) {
  return [lerp(a[0], b[0], t), lerp(a[1], b[1], t), lerp(a[2], b[2], t)];
}

export function createDefaultSequenceDocument(name = 'stage_intro') {
  return {
    schema: SEQUENCE_SCHEMA,
    name,
    fps: 60,
    durationMs: 2400,
    looping: false,
    markers: [
      { id: 'mk_anticipation', label: 'Anticipation', atMs: 300 },
      { id: 'mk_impact', label: 'Impact', atMs: 1200 },
    ],
    tracks: [
      {
        id: 'trk_cam',
        kind: 'camera',
        label: 'Main Camera',
        muted: false,
        sections: [
          {
            id: 'cam_establish',
            startMs: 0,
            endMs: 2400,
            view: 'establishing_wide',
            fromEye: [0, 1.8, 3.6],
            toEye: [0, 1.65, 2.8],
            fromTarget: [0, 1.1, 0],
            toTarget: [0, 1.15, 0],
            fovFrom: 44,
            fovTo: 38,
            curve: { type: 'cubicBezier', x1: 0.22, y1: 1.0, x2: 0.36, y2: 1.0 },
          },
        ],
      },
      {
        id: 'trk_actor',
        kind: 'skeletal',
        label: 'Lead Actor',
        target: 'actor_lead',
        muted: false,
        sections: [
          {
            id: 'act_gesture',
            startMs: 0,
            endMs: 1800,
            clip: 'gesture_greet',
            clipOffsetS: 0,
            speed: 1.0,
            blendInMs: 120,
            blendOutMs: 180,
            weight: 1.0,
          },
        ],
      },
      {
        id: 'trk_prop',
        kind: 'transform',
        label: 'Hero Prop',
        target: 'prop_hero',
        muted: false,
        sections: [
          {
            id: 'prop_arc',
            startMs: 300,
            endMs: 1200,
            fromPos: [-0.4, 0.9, 0.2],
            toPos: [0.0, 0.9, 0.6],
            arcLift: 0.18,
            fromEulerDeg: [0, 0, 0],
            toEulerDeg: [0, 180, 0],
            fromScale: [1, 1, 1],
            toScale: [1, 1, 1],
            curve: { type: 'spring', stiffness: 260, damping: 24, mass: 1 },
          },
        ],
      },
      {
        id: 'trk_vfx',
        kind: 'vfx',
        label: 'Accent Burst',
        target: 'stage_center',
        muted: false,
        triggers: [
          {
            id: 'vfx_hit',
            atMs: 1200,
            effect: 'radial_burst',
            durationMs: 900,
            scale: 1.0,
            burstMultiplier: 1.0,
          },
        ],
      },
      {
        id: 'trk_audio',
        kind: 'audio',
        label: 'SFX Cues',
        bus: 'sfx',
        muted: false,
        triggers: [
          {
            id: 'aud_accent',
            atMs: 1200,
            cue: 'sfx/stage_chime',
            gainDb: 0,
            pitchSemitones: 0,
          },
        ],
      },
    ],
  };
}

export function normalizeSequenceDocument(raw) {
  const fallback = createDefaultSequenceDocument();
  if (!raw || typeof raw !== 'object') return fallback;
  return {
    schema: SEQUENCE_SCHEMA,
    name: String(raw.name ?? fallback.name),
    fps: Math.max(1, Number(raw.fps ?? 60)),
    durationMs: Math.max(1, Number(raw.durationMs ?? 2400)),
    looping: Boolean(raw.looping),
    markers: Array.isArray(raw.markers) ? raw.markers.map((m) => ({ ...m })) : [],
    tracks: Array.isArray(raw.tracks)
      ? raw.tracks.map((t) => ({
          ...t,
          muted: Boolean(t.muted),
          sections: Array.isArray(t.sections) ? t.sections.map((s) => ({ ...s })) : undefined,
          triggers: Array.isArray(t.triggers) ? t.triggers.map((tr) => ({ ...tr })) : undefined,
        }))
      : fallback.tracks,
  };
}

export function evaluateSequenceAt(rawDoc, rawTimeMs) {
  const doc = normalizeSequenceDocument(rawDoc);
  const dur = Math.max(1, doc.durationMs);
  const clampedMs = doc.looping
    ? ((Number(rawTimeMs || 0) % dur) + dur) % dur
    : Math.min(dur, Math.max(0, Number(rawTimeMs || 0)));

  let camera = null;
  const skeletal = [];
  const transforms = [];

  for (const track of doc.tracks) {
    if (track.muted) continue;
    if (track.kind === 'camera') {
      for (const sec of track.sections ?? []) {
        if (clampedMs >= sec.startMs && clampedMs <= sec.endMs) {
          const spanMs = Math.max(1, sec.endMs - sec.startMs);
          const rawT = (clampedMs - sec.startMs) / spanMs;
          const eased = sampleSequenceCurve(sec.curve, rawT, spanMs / 1000);
          camera = {
            trackId: track.id,
            sectionId: sec.id,
            view: sec.view,
            eye: lerp3(sec.fromEye ?? [0, 1.6, 3], sec.toEye ?? [0, 1.6, 3], eased),
            target: lerp3(sec.fromTarget ?? [0, 1, 0], sec.toTarget ?? [0, 1, 0], eased),
            fov: lerp(Number(sec.fovFrom ?? 45), Number(sec.fovTo ?? 45), eased),
          };
        }
      }
    } else if (track.kind === 'skeletal') {
      for (const sec of track.sections ?? []) {
        if (clampedMs >= sec.startMs && clampedMs <= sec.endMs) {
          const localMs = clampedMs - sec.startMs;
          const remMs = sec.endMs - clampedMs;
          let env = 1;
          if ((sec.blendInMs ?? 0) > 0 && localMs < sec.blendInMs) {
            const u = localMs / sec.blendInMs;
            env *= u * u * (3 - 2 * u);
          }
          if ((sec.blendOutMs ?? 0) > 0 && remMs < sec.blendOutMs) {
            const u = remMs / sec.blendOutMs;
            env *= u * u * (3 - 2 * u);
          }
          skeletal.push({
            trackId: track.id,
            target: track.target,
            sectionId: sec.id,
            clip: sec.clip,
            secondaryClip: sec.secondaryClip ?? null,
            sampleTimeS: Number(sec.clipOffsetS ?? 0) + (localMs / 1000) * Number(sec.speed ?? 1),
            weight: Number(sec.weight ?? 1) * env,
          });
        }
      }
    } else if (track.kind === 'transform') {
      for (const sec of track.sections ?? []) {
        if (clampedMs >= sec.startMs && clampedMs <= sec.endMs) {
          const spanMs = Math.max(1, sec.endMs - sec.startMs);
          const rawT = (clampedMs - sec.startMs) / spanMs;
          const eased = sampleSequenceCurve(sec.curve, rawT, spanMs / 1000);
          const pos = lerp3(sec.fromPos ?? [0, 0, 0], sec.toPos ?? [0, 0, 0], eased);
          const lift = 4 * rawT * (1 - rawT) * Number(sec.arcLift ?? 0);
          pos[1] += lift;
          transforms.push({
            trackId: track.id,
            target: track.target,
            sectionId: sec.id,
            position: pos,
            eulerDeg: lerp3(sec.fromEulerDeg ?? [0, 0, 0], sec.toEulerDeg ?? [0, 0, 0], eased),
            scale: lerp3(sec.fromScale ?? [1, 1, 1], sec.toScale ?? [1, 1, 1], eased),
          });
        }
      }
    }
  }

  return {
    timeMs: clampedMs,
    frame: Math.floor((clampedMs / 1000) * doc.fps),
    camera,
    skeletal,
    transforms,
  };
}

export function collectSequenceTriggers(rawDoc, prevMs, currMs) {
  const doc = normalizeSequenceDocument(rawDoc);
  const events = [];
  const inWindow = (atMs) => {
    if (currMs >= prevMs) {
      return prevMs === 0 ? atMs >= 0 && atMs <= currMs : atMs > prevMs && atMs <= currMs;
    }
    return doc.looping && (atMs > prevMs || atMs <= currMs);
  };
  for (const track of doc.tracks) {
    if (track.muted) continue;
    if (track.kind === 'vfx' || track.kind === 'audio') {
      for (const tr of track.triggers ?? []) {
        if (inWindow(tr.atMs)) {
          events.push({
            kind: track.kind,
            trackId: track.id,
            target: track.target ?? track.bus ?? '',
            ...tr,
          });
        }
      }
    }
  }
  events.sort((a, b) => a.atMs - b.atMs);
  return events;
}

export function buildSectionSpanOps(trackIndex, sectionIndex, startMs, endMs, fps = 60, snap = false) {
  const s = snap ? snapMsToFrame(startMs, fps) : Math.max(0, Math.round(startMs));
  const e = Math.max(s + 1, snap ? snapMsToFrame(endMs, fps) : Math.round(endMs));
  return [
    { set: ['tracks', String(trackIndex), 'sections', String(sectionIndex), 'startMs'], value: s },
    { set: ['tracks', String(trackIndex), 'sections', String(sectionIndex), 'endMs'], value: e },
  ];
}

export function buildTriggerTimingOps(trackIndex, triggerIndex, atMs, fps = 60, snap = false) {
  const t = snap ? snapMsToFrame(atMs, fps) : Math.max(0, Math.round(atMs));
  return [{ set: ['tracks', String(trackIndex), 'triggers', String(triggerIndex), 'atMs'], value: t }];
}

export function buildTrackMuteOps(trackIndex, muted) {
  return [{ set: ['tracks', String(trackIndex), 'muted'], value: Boolean(muted) }];
}

export function buildTransformCurveOps(trackIndex, sectionIndex, curve, arcLift) {
  const ops = [{ set: ['tracks', String(trackIndex), 'sections', String(sectionIndex), 'curve'], value: curve }];
  if (arcLift !== undefined) {
    ops.push({
      set: ['tracks', String(trackIndex), 'sections', String(sectionIndex), 'arcLift'],
      value: Number(arcLift),
    });
  }
  return ops;
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

export function mountSequencerStudio(container, studioApi = {}) {
  const dom = container.ownerDocument || globalThis.document;
  let activeDocId = null;
  let docData = createDefaultSequenceDocument();
  let playheadMs = 0;
  let playing = false;
  let snapToFrame = true;
  let playbackSpeed = 1.0;
  let selectedItem = { trackIndex: 0, itemType: 'section', itemIndex: 0 };
  let active = true;
  let destroyed = false;
  const unsubscribers = [];

  const syncFromStudio = () => {
    const activeDoc = studioApi.getActiveDocument?.();
    if (activeDoc && (String(activeDoc.path ?? activeDoc.id ?? '').endsWith('.seq.json') ||
        activeDoc.root?.schema === SEQUENCE_SCHEMA)) {
      activeDocId = activeDoc.id ?? activeDoc.path;
      docData = normalizeSequenceDocument(activeDoc.root ?? activeDoc.data ?? activeDoc);
    }
  };

  const broadcastPlayhead = (prevMs, nextMs) => {
    const frame = evaluateSequenceAt(docData, nextMs);
    studioApi.sendPieClock?.({
      playing,
      time: frame.timeMs / 1000,
      speed: playbackSpeed,
    });
    if (frame.camera) {
      studioApi.getViewport?.()?.setCameraPose?.(frame.camera);
    }
    if (prevMs !== null && prevMs !== nextMs) {
      for (const cue of collectSequenceTriggers(docData, prevMs, nextMs)) {
        studioApi.sendPieCue?.({
          cueId: cue.id,
          track: cue.trackId,
          payload: cue,
        });
      }
    }
    return frame;
  };

  const commitOps = (ops, label, merge = false) => {
    if (!ops.length) return;
    for (const op of ops) applyOpLocally(docData, op);
    docData = normalizeSequenceDocument(docData);
    if (activeDocId && typeof studioApi.edit === 'function') {
      studioApi.edit(activeDocId, ops, label, merge);
    }
    broadcastPlayhead(null, playheadMs);
    render();
  };

  const root = dom.createElement('div');
  root.className = 'gb-seq-studio';
  container.innerHTML = '';
  container.appendChild(root);

  const el = (tag, className, text) => {
    const node = dom.createElement(tag);
    if (className) node.className = className;
    if (text != null) node.textContent = text;
    return node;
  };

  const render = () => {
    if (destroyed) return;
    root.innerHTML = '';

    const toolbar = el('div', 'gb-seq-toolbar');
    toolbar.appendChild(el('span', 'gb-seq-title', 'Sequencer Studio'));
    toolbar.appendChild(el('span', 'gb-seq-clock', msToFrameLabel(playheadMs, docData.fps)));

    const playBtn = el('button', `gb-seq-btn${playing ? ' is-active' : ''}`, playing ? 'Pause' : 'Play');
    playBtn.dataset.role = 'seq-play';
    playBtn.addEventListener('click', () => {
      playing = !playing;
      broadcastPlayhead(null, playheadMs);
      render();
    });
    toolbar.appendChild(playBtn);

    const snapBtn = el('button', `gb-seq-btn${snapToFrame ? ' is-active' : ''}`, 'Snap');
    snapBtn.dataset.role = 'seq-snap';
    snapBtn.addEventListener('click', () => {
      snapToFrame = !snapToFrame;
      render();
    });
    toolbar.appendChild(snapBtn);

    const speedSelect = el('select', 'gb-seq-select');
    speedSelect.dataset.role = 'seq-speed';
    for (const spd of [0.25, 0.5, 1, 2]) {
      const opt = el('option', '', `${spd}x`);
      opt.value = String(spd);
      if (spd === playbackSpeed) opt.selected = true;
      speedSelect.appendChild(opt);
    }
    speedSelect.addEventListener('change', (ev) => {
      playbackSpeed = Number(ev.target.value);
      broadcastPlayhead(null, playheadMs);
    });
    toolbar.appendChild(speedSelect);

    toolbar.appendChild(el('div', 'gb-seq-spacer'));

    const saveBtn = el('button', 'gb-seq-btn', 'Save');
    saveBtn.dataset.role = 'seq-save';
    saveBtn.addEventListener('click', () => {
      if (activeDocId) studioApi.save?.(activeDocId);
    });
    toolbar.appendChild(saveBtn);
    root.appendChild(toolbar);

    const body = el('div', 'gb-seq-body');
    const timeline = el('div', 'gb-seq-timeline');

    // Playhead scrubber row
    const rulerRow = el('div', 'gb-seq-ruler-row');
    rulerRow.appendChild(el('span', '', `${docData.name} (${docData.durationMs}ms @ ${docData.fps}fps)`));
    const scrub = el('input');
    scrub.type = 'range';
    scrub.min = '0';
    scrub.max = String(docData.durationMs);
    scrub.step = '1';
    scrub.value = String(playheadMs);
    scrub.dataset.role = 'seq-scrubber';
    scrub.addEventListener('input', (ev) => {
      const prev = playheadMs;
      const raw = Number(ev.target.value);
      playheadMs = snapToFrame ? snapMsToFrame(raw, docData.fps) : raw;
      broadcastPlayhead(prev, playheadMs);
      render();
    });
    rulerRow.appendChild(scrub);
    timeline.appendChild(rulerRow);

    // Track lanes
    const dur = Math.max(1, docData.durationMs);
    const playheadPct = Math.min(100, Math.max(0, (playheadMs / dur) * 100));

    docData.tracks.forEach((track, tIdx) => {
      const lane = el('div', 'gb-seq-lane');
      const header = el('div', 'gb-seq-lane-header');
      header.appendChild(el('span', '', `${track.label} [${track.kind}]`));
      const muteBtn = el('button', `gb-seq-btn${track.muted ? ' is-active' : ''}`, track.muted ? 'M' : 'On');
      muteBtn.dataset.role = `mute-track-${tIdx}`;
      muteBtn.addEventListener('click', () => {
        commitOps(buildTrackMuteOps(tIdx, !track.muted), `Toggle mute ${track.label}`);
      });
      header.appendChild(muteBtn);
      lane.appendChild(header);

      const laneCanvas = el('div', 'gb-seq-lane-canvas');
      (track.sections ?? []).forEach((sec, sIdx) => {
        const isSel = selectedItem.trackIndex === tIdx && selectedItem.itemType === 'section' && selectedItem.itemIndex === sIdx;
        const bar = el('button', `gb-seq-section${isSel ? ' is-selected' : ''}`, sec.clip ?? sec.view ?? sec.id);
        bar.style.left = `${(sec.startMs / dur) * 100}%`;
        bar.style.width = `${Math.max(2, ((sec.endMs - sec.startMs) / dur) * 100)}%`;
        bar.dataset.sectionId = sec.id;
        bar.addEventListener('click', () => {
          selectedItem = { trackIndex: tIdx, itemType: 'section', itemIndex: sIdx };
          render();
        });
        laneCanvas.appendChild(bar);
      });
      (track.triggers ?? []).forEach((tr, trIdx) => {
        const isSel = selectedItem.trackIndex === tIdx && selectedItem.itemType === 'trigger' && selectedItem.itemIndex === trIdx;
        const diamond = el('button', `gb-seq-trigger${isSel ? ' is-selected' : ''}`);
        diamond.style.left = `${(tr.atMs / dur) * 100}%`;
        diamond.title = `${tr.cue ?? tr.effect ?? tr.id} @ ${tr.atMs}ms`;
        diamond.dataset.triggerId = tr.id;
        diamond.addEventListener('click', () => {
          selectedItem = { trackIndex: tIdx, itemType: 'trigger', itemIndex: trIdx };
          render();
        });
        laneCanvas.appendChild(diamond);
      });

      const ph = el('div', 'gb-seq-playhead');
      ph.style.left = `${playheadPct}%`;
      laneCanvas.appendChild(ph);
      lane.appendChild(laneCanvas);
      timeline.appendChild(lane);
    });
    body.appendChild(timeline);

    // Inspector
    const inspector = el('div', 'gb-seq-inspector');
    const selTrack = docData.tracks[selectedItem.trackIndex];
    if (selTrack && selectedItem.itemType === 'section') {
      const sec = selTrack.sections?.[selectedItem.itemIndex];
      if (sec) {
        inspector.appendChild(el('div', 'gb-seq-title', `Section: ${sec.id}`));
        const startRow = el('div', 'gb-seq-field');
        startRow.appendChild(el('span', '', 'Start (ms)'));
        const startIn = el('input', 'gb-seq-input');
        startIn.type = 'number';
        startIn.value = String(sec.startMs);
        startIn.dataset.role = 'inspector-start-ms';
        startIn.addEventListener('change', (ev) => {
          commitOps(
            buildSectionSpanOps(selectedItem.trackIndex, selectedItem.itemIndex, Number(ev.target.value), sec.endMs, docData.fps, snapToFrame),
            `Move ${sec.id} start`,
          );
        });
        startRow.appendChild(startIn);
        inspector.appendChild(startRow);

        const endRow = el('div', 'gb-seq-field');
        endRow.appendChild(el('span', '', 'End (ms)'));
        const endIn = el('input', 'gb-seq-input');
        endIn.type = 'number';
        endIn.value = String(sec.endMs);
        endIn.dataset.role = 'inspector-end-ms';
        endIn.addEventListener('change', (ev) => {
          commitOps(
            buildSectionSpanOps(selectedItem.trackIndex, selectedItem.itemIndex, sec.startMs, Number(ev.target.value), docData.fps, snapToFrame),
            `Move ${sec.id} end`,
          );
        });
        endRow.appendChild(endIn);
        inspector.appendChild(endRow);

        if (sec.curve) {
          const curveSelect = el('select', 'gb-seq-select');
          curveSelect.dataset.role = 'inspector-curve-type';
          const curType = typeof sec.curve === 'string' ? sec.curve : sec.curve.type ?? 'smoothstep';
          for (const kind of ['linear', 'smoothstep', 'easeOutQuart', 'cubicBezier', 'spring']) {
            const opt = el('option', '', kind);
            opt.value = kind;
            if (kind === curType) opt.selected = true;
            curveSelect.appendChild(opt);
          }
          curveSelect.addEventListener('change', (ev) => {
            const nextKind = ev.target.value;
            let nextCurve = { type: nextKind };
            if (nextKind === 'cubicBezier') nextCurve = { type: 'cubicBezier', x1: 0.22, y1: 1.0, x2: 0.36, y2: 1.0 };
            if (nextKind === 'spring') nextCurve = { type: 'spring', stiffness: 260, damping: 24, mass: 1 };
            commitOps(
              buildTransformCurveOps(selectedItem.trackIndex, selectedItem.itemIndex, nextCurve, sec.arcLift),
              `Change curve ${sec.id}`,
            );
          });
          inspector.appendChild(curveSelect);

          // Curve SVG preview
          const svg = dom.createElementNS ? dom.createElementNS('http://www.w3.org/2000/svg', 'svg') : el('svg');
          svg.setAttribute?.('viewBox', '0 0 120 60');
          svg.setAttribute?.('class', 'gb-seq-curve-svg');
          const poly = dom.createElementNS ? dom.createElementNS('http://www.w3.org/2000/svg', 'polyline') : el('polyline');
          const pts = [];
          const spanS = Math.max(0.05, (sec.endMs - sec.startMs) / 1000);
          for (let i = 0; i <= 24; i++) {
            const u = i / 24;
            const y = sampleSequenceCurve(sec.curve, u, spanS);
            const px = (4 + u * 112).toFixed(1);
            const py = (54 - Math.min(1.25, Math.max(-0.25, y)) * 42).toFixed(1);
            pts.push(`${px},${py}`);
          }
          poly.setAttribute?.('points', pts.join(' '));
          poly.setAttribute?.('fill', 'none');
          poly.setAttribute?.('stroke', '#e8b44a');
          poly.setAttribute?.('stroke-width', '2');
          svg.appendChild(poly);
          inspector.appendChild(svg);
        }
      }
    } else if (selTrack && selectedItem.itemType === 'trigger') {
      const tr = selTrack.triggers?.[selectedItem.itemIndex];
      if (tr) {
        inspector.appendChild(el('div', 'gb-seq-title', `Trigger: ${tr.id}`));
        const atRow = el('div', 'gb-seq-field');
        atRow.appendChild(el('span', '', 'At (ms)'));
        const atIn = el('input', 'gb-seq-input');
        atIn.type = 'number';
        atIn.value = String(tr.atMs);
        atIn.dataset.role = 'inspector-at-ms';
        atIn.addEventListener('change', (ev) => {
          commitOps(
            buildTriggerTimingOps(selectedItem.trackIndex, selectedItem.itemIndex, Number(ev.target.value), docData.fps, snapToFrame),
            `Retiming trigger ${tr.id}`,
          );
        });
        atRow.appendChild(atIn);
        inspector.appendChild(atRow);
      }
    }
    body.appendChild(inspector);
    root.appendChild(body);
  };

  syncFromStudio();
  render();
  broadcastPlayhead(null, playheadMs);

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
    playing = false;
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
      broadcastPlayhead(null, playheadMs);
    },
    setDocument(pathOrId, rawDoc) {
      activeDocId = pathOrId;
      docData = normalizeSequenceDocument(rawDoc?.root ?? rawDoc);
      playheadMs = 0;
      render();
      broadcastPlayhead(null, playheadMs);
    },
    onActivate() {
      active = true;
      syncFromStudio();
      render();
    },
    onDeactivate() {
      active = false;
      playing = false;
    },
  };
}
