/**
 * Character Studio workbench (`*.character.json`, schema `ganbate.character/1`).
 *
 * Genre-neutral authoring surface for humanoid & creature skeletal rigs:
 * - Hierarchy Bone Tree with search + local Euler `(pitch, yaw, roll)` offset sliders
 * - Animation Clip scrubber & Hermite (`3t^2 - 2t^3`) state cross-fade preview
 * - Procedural Look-At 2D/3D target pad + joint distribution weights
 * - Analytical Two-Bone IK chain inspector (`root`, `mid`, `end`, `target`, `pole`, `weight`)
 * - Grouped Morph & Body Shape sliders (`expression`, `face`, `body`, `secondary`) + presets
 * - Modular Outfit variants & Named Socket attachment fitting
 */

import {
  hermiteFade,
  poseModelClip,
  quatFromEuler,
  sampleBlendSpace1D,
  sampleBlendSpace2D,
} from '../web/character-customization.mjs';
import { drawGlbPreviewCanvas, parseGlbPreview, parseGlbPreviewAsync } from './asset-previews.js';

export const CHARACTER_SCHEMA = 'ganbate.character/1';

export const TURNTABLE_LIGHTING_PRESETS = Object.freeze({
  studio_neutral: {
    id: 'studio_neutral',
    label: 'Studio Neutral',
    sunDirection: [-0.38, -0.78, -0.49],
    sunColor: [1.0, 0.98, 0.95],
    ambientSky: [0.34, 0.37, 0.44],
    ambientGround: [0.16, 0.15, 0.17],
  },
  warm_rim: {
    id: 'warm_rim',
    label: 'Warm Rim',
    sunDirection: [0.52, -0.62, -0.58],
    sunColor: [1.0, 0.86, 0.68],
    ambientSky: [0.30, 0.26, 0.34],
    ambientGround: [0.18, 0.13, 0.11],
  },
  cool_moonlight: {
    id: 'cool_moonlight',
    label: 'Cool Moonlight',
    sunDirection: [-0.45, -0.68, 0.57],
    sunColor: [0.76, 0.86, 1.0],
    ambientSky: [0.18, 0.23, 0.36],
    ambientGround: [0.08, 0.10, 0.15],
  },
});

const DEG_TO_RAD = Math.PI / 180;

export function createDefaultCharacterDocument(id = 'humanoid_actor', name = 'Humanoid Actor') {
  return {
    schema: CHARACTER_SCHEMA,
    id,
    name,
    skeleton: {
      scale: 1.0,
      scalePivot: [0, 0, 0],
      placement: { position: [0, 0, 0], yawRad: 0 },
      joints: [
        { index: 0, name: 'hips', parent: -1 },
        { index: 1, name: 'spine_01', parent: 0 },
        { index: 2, name: 'neck_01', parent: 1 },
        { index: 3, name: 'head', parent: 2 },
        { index: 4, name: 'upperarm_r', parent: 1 },
        { index: 5, name: 'lowerarm_r', parent: 4 },
        { index: 6, name: 'hand_r', parent: 5 },
      ],
    },
    activeOutfitId: 'standard_attire',
    outfits: [
      {
        id: 'standard_attire',
        label: 'Standard Attire',
        modelPath: 'content/characters/humanoid_standard.glb',
        scale: 1.0,
        hiddenMaterials: [],
      },
    ],
    sockets: [
      {
        id: 'prop_hand_r',
        parentJoint: 'hand_r',
        translation: [0.02, 0.04, 0.0],
        eulerDeg: [0, 0, 0],
        attachedAsset: '',
      },
    ],
    clips: [
      { id: 'idle_calm', index: 0, label: 'Idle Calm', durationS: 2.4, looping: true },
      { id: 'gesture_greet', index: 1, label: 'Gesture Greet', durationS: 1.6, looping: false },
    ],
    stateChains: [
      { fromClip: 'gesture_greet', toClip: 'idle_calm', crossfadeMs: 220 },
    ],
    blendSpaces: [
      {
        id: 'stance_blend_1d',
        dimensions: 1,
        samples: [
          { clipId: 'idle_calm', position: 0, rate: 1 },
          { clipId: 'gesture_greet', position: 1, rate: 1 },
        ],
      },
    ],
    lookAt: {
      enabled: true,
      origin: [0, 1.55, 0],
      target: [0, 1.55, 2.0],
      maxYawDeg: 55,
      maxPitchDeg: 35,
      chain: [
        { joint: 'neck_01', weight: 0.35 },
        { joint: 'head', weight: 0.65 },
      ],
    },
    ikChains: [
      {
        id: 'reach_hand_r',
        enabled: false,
        rootJoint: 'upperarm_r',
        midJoint: 'lowerarm_r',
        endJoint: 'hand_r',
        target: [0.32, 1.15, 0.38],
        poleVector: [0.25, 1.05, -0.35],
        weight: 1.0,
      },
    ],
    morphTargets: [
      { id: 'blink', index: 0, node: 3, label: 'Blink', group: 'expression', min: 0, max: 1, defaultWeight: 0 },
      { id: 'smile', index: 1, node: 3, label: 'Smile', group: 'expression', min: 0, max: 1, defaultWeight: 0 },
      { id: 'jaw_width', index: 2, node: 3, label: 'Jaw Width', group: 'face', min: -1, max: 1, defaultWeight: 0 },
      { id: 'shoulder_breadth', index: 3, node: 1, label: 'Shoulder Breadth', group: 'body', min: -1, max: 1, defaultWeight: 0 },
      { id: 'cloth_settle', index: 4, node: 1, label: 'Cloth Settle', group: 'secondary', min: 0, max: 1, defaultWeight: 0 },
    ],
    expressionIntensity: 1.0,
    morphWeights: {
      blink: 0,
      smile: 0.15,
      jaw_width: 0,
      shoulder_breadth: 0,
      cloth_settle: 0,
    },
    expressionPresets: {
      neutral: { blink: 0, smile: 0 },
      warm_smile: { blink: 0.08, smile: 0.72 },
      focused: { blink: 0.18, smile: 0.05 },
    },
    boneOffsets: {},
  };
}

export function normalizeCharacterDocument(raw) {
  const fallback = createDefaultCharacterDocument();
  if (!raw || typeof raw !== 'object') return fallback;
  const joints = Array.isArray(raw.skeleton?.joints) && raw.skeleton.joints.length
    ? raw.skeleton.joints.map((j, idx) => ({
        index: typeof j.index === 'number' ? j.index : idx,
        name: String(j.name ?? `joint_${idx}`),
        parent: typeof j.parent === 'number' ? j.parent : -1,
      }))
    : fallback.skeleton.joints;
  const outfits = Array.isArray(raw.outfits) && raw.outfits.length ? raw.outfits : fallback.outfits;
  return {
    schema: CHARACTER_SCHEMA,
    id: String(raw.id ?? fallback.id),
    name: String(raw.name ?? raw.id ?? fallback.name),
    skeleton: {
      scale: Number.isFinite(raw.skeleton?.scale) ? raw.skeleton.scale : 1.0,
      scalePivot: Array.isArray(raw.skeleton?.scalePivot) ? [...raw.skeleton.scalePivot] : [0, 0, 0],
      placement: raw.skeleton?.placement
        ? {
            position: [...(raw.skeleton.placement.position ?? [0, 0, 0])],
            yawRad: Number(raw.skeleton.placement.yawRad ?? 0),
          }
        : { position: [0, 0, 0], yawRad: 0 },
      joints,
    },
    activeOutfitId: String(raw.activeOutfitId ?? outfits[0].id),
    outfits: outfits.map((o) => ({
      id: String(o.id),
      label: String(o.label ?? o.id),
      modelPath: String(o.modelPath ?? ''),
      scale: Number.isFinite(o.scale) ? o.scale : 1.0,
      hiddenMaterials: Array.isArray(o.hiddenMaterials) ? [...o.hiddenMaterials] : [],
    })),
    sockets: Array.isArray(raw.sockets)
      ? raw.sockets.map((s) => ({
          id: String(s.id),
          parentJoint: String(s.parentJoint ?? ''),
          translation: Array.isArray(s.translation) ? [...s.translation] : [0, 0, 0],
          eulerDeg: Array.isArray(s.eulerDeg) ? [...s.eulerDeg] : [0, 0, 0],
          attachedAsset: String(s.attachedAsset ?? ''),
        }))
      : [],
    clips: Array.isArray(raw.clips) && raw.clips.length
      ? raw.clips.map((c, idx) => ({
          id: String(c.id ?? `clip_${idx}`),
          index: typeof c.index === 'number' ? c.index : idx,
          label: String(c.label ?? c.id ?? `Clip ${idx}`),
          durationS: Number.isFinite(c.durationS) && c.durationS > 0 ? c.durationS : 1.0,
          looping: c.looping !== false,
        }))
      : fallback.clips,
    stateChains: Array.isArray(raw.stateChains) ? raw.stateChains.map((ch) => ({ ...ch })) : [],
    blendSpaces: Array.isArray(raw.blendSpaces) ? raw.blendSpaces.map((bs) => ({ ...bs })) : fallback.blendSpaces,
    lookAt: {
      enabled: raw.lookAt?.enabled !== false,
      origin: Array.isArray(raw.lookAt?.origin) ? [...raw.lookAt.origin] : [0, 1.55, 0],
      target: Array.isArray(raw.lookAt?.target) ? [...raw.lookAt.target] : [0, 1.55, 2.0],
      maxYawDeg: Number.isFinite(raw.lookAt?.maxYawDeg) ? raw.lookAt.maxYawDeg : 55,
      maxPitchDeg: Number.isFinite(raw.lookAt?.maxPitchDeg) ? raw.lookAt.maxPitchDeg : 35,
      chain: Array.isArray(raw.lookAt?.chain)
        ? raw.lookAt.chain.map(( entry ) => ({
            joint: String(entry.joint),
            weight: Number(entry.weight ?? 0.5),
          }))
        : fallback.lookAt.chain,
    },
    ikChains: Array.isArray(raw.ikChains)
      ? raw.ikChains.map((ik, idx) => ({
          id: String(ik.id ?? `ik_${idx}`),
          enabled: Boolean(ik.enabled),
          rootJoint: String(ik.rootJoint ?? ''),
          midJoint: String(ik.midJoint ?? ''),
          endJoint: String(ik.endJoint ?? ''),
          target: Array.isArray(ik.target) ? [...ik.target] : [0, 1, 0.3],
          poleVector: Array.isArray(ik.poleVector) ? [...ik.poleVector] : [0, 1, -0.4],
          weight: Number.isFinite(ik.weight) ? ik.weight : 1.0,
        }))
      : [],
    morphTargets: Array.isArray(raw.morphTargets)
      ? raw.morphTargets.map((m, idx) => ({
          id: String(m.id ?? `morph_${idx}`),
          index: typeof m.index === 'number' ? m.index : idx,
          node: typeof m.node === 'number' ? m.node : 0,
          label: String(m.label ?? m.id ?? `Morph ${idx}`),
          group: ['expression', 'face', 'body', 'secondary'].includes(m.group) ? m.group : 'expression',
          min: Number.isFinite(m.min) ? m.min : 0,
          max: Number.isFinite(m.max) ? m.max : 1,
          defaultWeight: Number.isFinite(m.defaultWeight) ? m.defaultWeight : 0,
        }))
      : fallback.morphTargets,
    expressionIntensity: Number.isFinite(raw.expressionIntensity) ? raw.expressionIntensity : 1.0,
    morphWeights: raw.morphWeights && typeof raw.morphWeights === 'object' ? { ...raw.morphWeights } : {},
    expressionPresets:
      raw.expressionPresets && typeof raw.expressionPresets === 'object'
        ? { ...raw.expressionPresets }
        : fallback.expressionPresets,
    boneOffsets: raw.boneOffsets && typeof raw.boneOffsets === 'object' ? { ...raw.boneOffsets } : {},
  };
}

export function filterBoneTree(joints, query = '') {
  const depthByIndex = new Map();
  const byIndex = new Map(joints.map((j) => [j.index, j]));
  const depthOf = (j, seen = new Set()) => {
    if (depthByIndex.has(j.index)) return depthByIndex.get(j.index);
    if (j.parent < 0 || !byIndex.has(j.parent) || seen.has(j.index)) {
      depthByIndex.set(j.index, 0);
      return 0;
    }
    seen.add(j.index);
    const d = depthOf(byIndex.get(j.parent), seen) + 1;
    depthByIndex.set(j.index, d);
    return d;
  };
  const needle = String(query ?? '').trim().toLowerCase();
  return joints
    .map((j) => ({ ...j, depth: depthOf(j) }))
    .filter((j) => !needle || j.name.toLowerCase().includes(needle));
}

export function groupMorphTargets(morphTargets) {
  const groups = {
    expression: [],
    face: [],
    body: [],
    secondary: [],
  };
  for (const m of morphTargets ?? []) {
    const key = Object.hasOwn(groups, m.group) ? m.group : 'expression';
    groups[key].push(m);
  }
  return groups;
}

export function buildBoneOffsetOps(jointName, eulerDeg, translation = [0, 0, 0]) {
  const isZero = eulerDeg.every((v) => Math.abs(v) < 1e-6) && translation.every((v) => Math.abs(v) < 1e-6);
  if (isZero) {
    return [{ remove: ['boneOffsets', jointName] }];
  }
  return [
    {
      set: ['boneOffsets', jointName],
      value: {
        eulerDeg: eulerDeg.map((n) => Number(n)),
        translation: translation.map((n) => Number(n)),
      },
    },
  ];
}

export function buildMorphWeightOps(morphId, weight) {
  return [{ set: ['morphWeights', morphId], value: Number(weight) }];
}

export function buildPresetApplyOps(doc, presetName) {
  const preset = doc.expressionPresets?.[presetName];
  if (!preset || typeof preset !== 'object') return [];
  return Object.entries(preset).map(([morphId, weight]) => ({
    set: ['morphWeights', morphId],
    value: Number(weight),
  }));
}

export function buildLookAtTargetOps(targetVec3, enabled = true) {
  return [
    { set: ['lookAt', 'enabled'], value: Boolean(enabled) },
    { set: ['lookAt', 'target'], value: targetVec3.map((n) => Number(n)) },
  ];
}

export function buildIkChainOps(chainIndex, patch) {
  return Object.entries(patch).map(([key, value]) => ({
    set: ['ikChains', String(chainIndex), key],
    value: Array.isArray(value) ? value.map((n) => Number(n)) : value,
  }));
}

export function buildActiveOutfitOps(outfitId) {
  return [{ set: ['activeOutfitId'], value: String(outfitId) }];
}

export function buildSocketTransformOps(socketIndex, patch) {
  return Object.entries(patch).map(([key, value]) => ({
    set: ['sockets', String(socketIndex), key],
    value: Array.isArray(value) ? value.map((n) => Number(n)) : value,
  }));
}

/**
 * Compiles a `.character.json` document + live playback state into a single-call `PoseSpec`
 * suitable for `poseModelClip` / `ganbate_rt_pose_model_clip`.
 */
export function evaluateCharacterPoseSpec(rawDoc, state = {}) {
  const doc = normalizeCharacterDocument(rawDoc);
  const jointByName = new Map(doc.skeleton.joints.map((j) => [j.name, j.index]));
  const clipById = new Map(doc.clips.map((c) => [c.id, c]));

  const primaryClip = clipById.get(state.clipId) ?? doc.clips[0];
  const clips = [];
  let hermiteWeight = 1;

  if (state.blendSpaceId) {
    const bs = doc.blendSpaces.find((b) => b.id === state.blendSpaceId);
    if (bs) {
      const resolvedSamples = (bs.samples ?? [])
        .map((s) => {
          const c = typeof s.clip === 'number' ? { index: s.clip } : clipById.get(s.clipId);
          return c ? { clip: c.index, position: s.position, rate: s.rate ?? 1 } : null;
        })
        .filter(Boolean);
      const layers = bs.dimensions === 2
        ? sampleBlendSpace2D(resolvedSamples, Array.isArray(state.blendParam) ? state.blendParam : [0, 0], Number(state.timeS ?? 0))
        : sampleBlendSpace1D(resolvedSamples, Number(state.blendParam ?? 0), Number(state.timeS ?? 0));
      clips.push(...layers);
    }
  } else {
    if (state.fromClipId && state.crossfadeDurationS > 0) {
      const prevClip = clipById.get(state.fromClipId);
      hermiteWeight = hermiteFade(state.crossfadeElapsedS ?? 0, state.crossfadeDurationS);
      if (prevClip && hermiteWeight < 1) {
        clips.push({
          clip: prevClip.index,
          time: state.fromClipTimeS ?? 0,
          weight: 1,
        });
      }
    }
    if (primaryClip) {
      const rawTime = Number(state.timeS ?? 0);
      const clipTime = primaryClip.looping && primaryClip.durationS > 0
        ? ((rawTime % primaryClip.durationS) + primaryClip.durationS) % primaryClip.durationS
        : Math.min(primaryClip.durationS, Math.max(0, rawTime));
      clips.push({
        clip: primaryClip.index,
        time: clipTime,
        weight: clips.length ? hermiteWeight : 1,
      });
    }
  }

  const lookAt = [];
  if (doc.lookAt.enabled) {
    const nodes = doc.lookAt.chain
      .map((entry) => [jointByName.get(entry.joint), entry.weight])
      .filter(([idx]) => idx != null);
    if (nodes.length) {
      lookAt.push({
        targetModel: doc.lookAt.target,
        originModel: doc.lookAt.origin,
        forwardLocal: [0, 0, 1],
        upLocal: [0, 1, 0],
        maxYaw: doc.lookAt.maxYawDeg * DEG_TO_RAD,
        maxPitch: doc.lookAt.maxPitchDeg * DEG_TO_RAD,
        weight: 1,
        nodes,
      });
    }
  }

  const twoBoneIk = [];
  for (const ik of doc.ikChains) {
    if (!ik.enabled || ik.weight <= 0) continue;
    const root = jointByName.get(ik.rootJoint);
    const mid = jointByName.get(ik.midJoint);
    const end = jointByName.get(ik.endJoint);
    if (root == null || mid == null || end == null) continue;
    twoBoneIk.push({
      root,
      mid,
      end,
      targetModel: ik.target,
      poleModel: ik.poleVector,
      weight: Math.min(1, Math.max(0, ik.weight)),
    });
  }

  const boneOffsets = [];
  for (const [jointName, off] of Object.entries(doc.boneOffsets)) {
    const node = jointByName.get(jointName);
    if (node == null || !off) continue;
    const deg = off.eulerDeg ?? [0, 0, 0];
    const rotation = quatFromEuler(deg[0] * DEG_TO_RAD, deg[1] * DEG_TO_RAD, deg[2] * DEG_TO_RAD);
    boneOffsets.push({
      node,
      translation: off.translation ?? [0, 0, 0],
      rotation,
      scale: [1, 1, 1],
    });
  }

  const intensity = Number.isFinite(doc.expressionIntensity) ? doc.expressionIntensity : 1.0;
  const morphOverrides = [];
  for (const target of doc.morphTargets) {
    const rawWeight = doc.morphWeights[target.id] ?? target.defaultWeight ?? 0;
    const scaled = target.group === 'expression' ? rawWeight * intensity : rawWeight;
    const clamped = Math.min(target.max, Math.max(target.min, scaled));
    morphOverrides.push({
      node: target.node,
      target: target.index,
      weight: clamped,
    });
  }

  return {
    resetToRest: true,
    commitUpdate: true,
    clips,
    crossfade: null,
    lookAt,
    twoBoneIk,
    boneOffsets,
    morphOverrides,
    hermiteWeight,
  };
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
  } else if (op.remove && op.remove.length) {
    let cur = root;
    for (let i = 0; i < op.remove.length - 1; i++) {
      cur = cur?.[op.remove[i]];
      if (!cur) return;
    }
    delete cur[op.remove[op.remove.length - 1]];
  }
}

export function mountCharacterStudio(container, studioApi = {}) {
  const dom = container.ownerDocument || globalThis.document;
  let activeDocId = null;
  let docData = createDefaultCharacterDocument();
  let selectedJoint = docData.skeleton.joints[0]?.name ?? 'hips';
  let boneQuery = '';
  let lightingPreset = 'studio_neutral';
  let clipId = docData.clips[0]?.id ?? '';
  let targetClipId = docData.clips[1]?.id ?? docData.clips[0]?.id ?? '';
  let fromClipId = null;
  let crossfadeMs = 220;
  let crossfadeElapsedS = 0;
  let timeS = 0;
  let playing = false;
  let active = true;
  let destroyed = false;

  // Persistent 3D Character Viewport Stage state
  let stageYaw = 0.24;
  let stagePitch = 0.14;
  let stageZoom = 1.05;
  let stagePanX = 0;
  let stagePanY = 0;
  let stageFocus = 'body';
  let stageShading = 'textured';
  let stageTurntable = false;
  let stageRaf = 0;
  let activeModelUrl = '';
  let activeParsedGlb = null;
  let stageStatusText = '';
  const glbByUrl = new Map();

  const unsubscribers = [];

  const el = (tag, className, text) => {
    const node = dom.createElement(tag);
    if (className) node.className = className;
    if (text != null) node.textContent = text;
    return node;
  };

  const resolveCharacterModelUrl = (rawPath) => {
    const clean = String(rawPath || '').trim();
    if (!clean) return '';
    if (clean.startsWith('/') || clean.startsWith('http://') || clean.startsWith('https://')) {
      return clean;
    }
    if (typeof studioApi.resolveAssetUrl === 'function') {
      return studioApi.resolveAssetUrl(clean);
    }
    return `/project/${clean.split('/').map(encodeURIComponent).join('/')}`;
  };

  const currentOutfit = () =>
    docData.outfits.find((o) => o.id === docData.activeOutfitId) ?? docData.outfits[0] ?? null;

  // Create persistent canvas once so dragging sliders never recreates or flickers the 3D stage
  const stageCanvas = el('canvas', 'gb-char-viewport-canvas');
  stageCanvas.dataset.role = 'character-stage-canvas';
  const hasRealCanvas = typeof stageCanvas.getContext === 'function';

  const repaintStage = () => {
    if (destroyed || !hasRealCanvas) return;
    const cssW = Math.max(260, stageCanvas.clientWidth || 480);
    const cssH = Math.max(220, stageCanvas.clientHeight || 360);
    drawGlbPreviewCanvas(stageCanvas, activeParsedGlb, {
      width: cssW,
      height: cssH,
      yaw: stageYaw + (Number(docData.skeleton?.placement?.yawRad) || 0),
      pitch: stagePitch,
      zoom: stageZoom,
      panX: stagePanX,
      panY: stagePanY,
      shading: stageShading,
      focus: stageFocus,
      onTextureLoaded: repaintStage,
    });

    const ctx = stageCanvas.getContext('2d');
    if (!ctx) return;
    const dpr = typeof window !== 'undefined' ? Math.min(2, window.devicePixelRatio || 1) : 1;
    ctx.save?.();
    ctx.setTransform?.(dpr, 0, 0, dpr, 0, 0);

    // Top-left model / outfit badge
    const outfit = currentOutfit();
    const topLabel = activeParsedGlb
      ? `${outfit?.label || docData.name} · ${activeParsedGlb.triangles.toLocaleString()} tris`
      : stageStatusText || 'Loading 3D character mesh…';
    ctx.fillStyle = 'rgba(14, 17, 24, 0.76)';
    ctx.fillRect?.(10, 10, Math.min(cssW - 20, Math.max(150, topLabel.length * 6.4 + 18)), 24);
    ctx.fillStyle = '#e8ecf4';
    ctx.font = '600 11px ui-sans-serif, system-ui, sans-serif';
    ctx.fillText?.(topLabel, 18, 25);

    // Live Look-At reticle indicator when enabled
    if (docData.lookAt?.enabled && activeParsedGlb) {
      const tx = Math.min(1, Math.max(-1, (docData.lookAt.target[0] ?? 0) / 1.5));
      const ty = Math.min(
        1,
        Math.max(-1, ((docData.lookAt.target[1] ?? 1.55) - (docData.lookAt.origin[1] ?? 1.55)) / 1.0),
      );
      const gx = cssW * 0.5 + tx * (cssW * 0.28);
      const gy = cssH * (stageFocus === 'bust' ? 0.42 : 0.26) - ty * (cssH * 0.2);
      ctx.strokeStyle = 'rgba(232, 180, 74, 0.85)';
      ctx.lineWidth = 1.4;
      ctx.beginPath?.();
      ctx.arc?.(gx, gy, 6, 0, Math.PI * 2);
      ctx.stroke?.();
    }

    // Bottom status bar showing active pose / morph summary
    const activeMorphs = Object.entries(docData.morphWeights || {})
      .filter(([, w]) => Math.abs(Number(w) || 0) > 0.02)
      .slice(0, 3)
      .map(([k, w]) => `${k}:${Number(w).toFixed(2)}`);
    const boneOffCount = Object.keys(docData.boneOffsets || {}).length;
    const footerParts = [`Clip: ${clipId || 'rest'} (${timeS.toFixed(2)}s)`];
    if (boneOffCount > 0) footerParts.push(`${boneOffCount} bone offset${boneOffCount > 1 ? 's' : ''}`);
    if (activeMorphs.length) footerParts.push(activeMorphs.join(' '));
    const footerText = footerParts.join(' · ');
    ctx.fillStyle = 'rgba(14, 17, 24, 0.72)';
    ctx.fillRect?.(10, cssH - 30, Math.min(cssW - 20, Math.max(160, footerText.length * 6.0 + 18)), 20);
    ctx.fillStyle = '#aeb6c6';
    ctx.font = '11px ui-sans-serif, system-ui, sans-serif';
    ctx.fillText?.(footerText, 18, cssH - 16);
    ctx.restore?.();
  };

  const ensureOutfitModelLoaded = async () => {
    if (destroyed || !hasRealCanvas || typeof globalThis.fetch !== 'function') return;
    const outfit = currentOutfit();
    const candidates = [];
    if (outfit?.modelPath) candidates.push(resolveCharacterModelUrl(outfit.modelPath));
    // Fallback to first project `.glb` asset if document uses a placeholder model path
    if (typeof studioApi.listAssets === 'function') {
      const firstCharAsset = studioApi
        .listAssets({ kind: 'model' })
        .find((a) => String(a.path || '').toLowerCase().includes('char') || String(a.path || '').endsWith('.glb'));
      if (firstCharAsset) candidates.push(resolveCharacterModelUrl(firstCharAsset.path));
    }
    const targetUrl = candidates[0] || '';
    if (!targetUrl) {
      activeParsedGlb = null;
      stageStatusText = 'No 3D .glb assigned to active outfit';
      repaintStage();
      return;
    }
    if (glbByUrl.has(targetUrl)) {
      activeModelUrl = targetUrl;
      activeParsedGlb = glbByUrl.get(targetUrl);
      stageStatusText = '';
      repaintStage();
      return;
    }
    activeModelUrl = targetUrl;
    activeParsedGlb = null;
    stageStatusText = `Loading ${outfit?.label || 'character'}…`;
    repaintStage();

    for (const url of candidates) {
      if (!url) continue;
      if (glbByUrl.has(url)) {
        if (destroyed) return;
        activeModelUrl = url;
        activeParsedGlb = glbByUrl.get(url);
        stageStatusText = '';
        repaintStage();
        return;
      }
      try {
        const res = await globalThis.fetch(url);
        if (!res?.ok) continue;
        const buf = await res.arrayBuffer();
        const parsed = (await parseGlbPreviewAsync(buf, { maxFullTris: 140000 })) || parseGlbPreview(buf);
        if (parsed) {
          glbByUrl.set(url, parsed);
          if (!destroyed && activeModelUrl === targetUrl) {
            activeModelUrl = url;
            activeParsedGlb = parsed;
            stageStatusText = '';
            repaintStage();
          }
          return;
        }
      } catch {
        // try next candidate
      }
    }
    if (!destroyed && activeModelUrl === targetUrl) {
      stageStatusText = `Model not found: ${outfit?.modelPath || targetUrl}`;
      repaintStage();
    }
  };

  if (hasRealCanvas) {
    let dragging = false;
    let panMode = false;
    let lx = 0;
    let ly = 0;
    stageCanvas.addEventListener('contextmenu', (ev) => ev.preventDefault?.());
    stageCanvas.addEventListener('pointerdown', (ev) => {
      dragging = true;
      panMode = ev.button === 2 || ev.button === 1 || Boolean(ev.shiftKey);
      lx = ev.clientX ?? 0;
      ly = ev.clientY ?? 0;
      stageCanvas.setPointerCapture?.(ev.pointerId);
    });
    stageCanvas.addEventListener('pointermove', (ev) => {
      if (!dragging) return;
      const dx = (ev.clientX ?? 0) - lx;
      const dy = (ev.clientY ?? 0) - ly;
      lx = ev.clientX ?? 0;
      ly = ev.clientY ?? 0;
      if (panMode) {
        stagePanX += dx;
        stagePanY += dy;
      } else {
        stageYaw -= dx * 0.01;
        stagePitch = Math.max(-1.15, Math.min(1.15, stagePitch + dy * 0.01));
      }
      repaintStage();
    });
    const endDrag = (ev) => {
      if (!dragging) return;
      dragging = false;
      try { stageCanvas.releasePointerCapture?.(ev.pointerId); } catch { /* ignore */ }
    };
    stageCanvas.addEventListener('pointerup', endDrag);
    stageCanvas.addEventListener('pointercancel', endDrag);
    stageCanvas.addEventListener('wheel', (ev) => {
      ev.preventDefault?.();
      const factor = (ev.deltaY ?? 0) < 0 ? 1.12 : 1 / 1.12;
      stageZoom = Math.max(0.35, Math.min(6.0, stageZoom * factor));
      repaintStage();
    });
  }

  const stepStageAnimation = () => {
    if (destroyed || !active || (!stageTurntable && !playing)) {
      stageRaf = 0;
      return;
    }
    if (stageTurntable) stageYaw += 0.012;
    if (playing) {
      const clipObj = docData.clips.find((c) => c.id === clipId) ?? docData.clips[0];
      const dur = Math.max(0.2, clipObj?.durationS ?? 2.0);
      timeS = clipObj?.looping !== false ? (timeS + 1 / 60) % dur : Math.min(dur, timeS + 1 / 60);
      pushPoseToEngine();
    }
    repaintStage();
    if (typeof requestAnimationFrame === 'function') {
      stageRaf = requestAnimationFrame(stepStageAnimation);
    }
  };

  const ensureAnimLoop = () => {
    if ((stageTurntable || playing) && !stageRaf && typeof requestAnimationFrame === 'function') {
      stageRaf = requestAnimationFrame(stepStageAnimation);
    }
  };

  const syncFromStudio = () => {
    const activeDoc = studioApi.getActiveDocument?.();
    if (activeDoc && (String(activeDoc.path ?? activeDoc.id ?? '').endsWith('.character.json') ||
        activeDoc.root?.schema === CHARACTER_SCHEMA)) {
      activeDocId = activeDoc.id ?? activeDoc.path;
      docData = normalizeCharacterDocument(activeDoc.root ?? activeDoc.data ?? activeDoc);
      if (!docData.skeleton.joints.some((j) => j.name === selectedJoint)) {
        selectedJoint = docData.skeleton.joints[0]?.name ?? '';
      }
      if (!docData.clips.some((c) => c.id === clipId)) {
        clipId = docData.clips[0]?.id ?? '';
      }
    }
  };

  const commitOps = (ops, label, merge = false, { skipRebuild = false } = {}) => {
    if (!ops.length) return;
    for (const op of ops) applyOpLocally(docData, op);
    docData = normalizeCharacterDocument(docData);
    if (activeDocId && typeof studioApi.edit === 'function') {
      studioApi.edit(activeDocId, ops, label, merge);
    }
    pushPoseToEngine();
    if (skipRebuild) {
      repaintStage();
    } else {
      render();
      void ensureOutfitModelLoaded();
    }
  };

  const pushPoseToEngine = () => {
    const spec = evaluateCharacterPoseSpec(docData, {
      clipId,
      fromClipId,
      crossfadeElapsedS,
      crossfadeDurationS: crossfadeMs / 1000,
      timeS,
    });
    const engine = studioApi.getEngine?.();
    const viewport = studioApi.getViewport?.();
    const instance = viewport?.characterInstance ?? engine?.activeCharacterInstance;
    if (engine && instance != null) {
      poseModelClip(engine, instance, spec);
    }
    if (viewport?.setLightingPreset && TURNTABLE_LIGHTING_PRESETS[lightingPreset]) {
      viewport.setLightingPreset(TURNTABLE_LIGHTING_PRESETS[lightingPreset]);
    }
    return spec;
  };

  const root = dom.createElement('div');
  root.className = 'gb-char-studio';
  container.innerHTML = '';
  container.appendChild(root);

  const render = () => {
    if (destroyed) return;
    root.innerHTML = '';

    // Toolbar
    const toolbar = el('div', 'gb-char-toolbar');
    toolbar.appendChild(el('span', 'gb-char-title', 'Character Studio'));
    toolbar.appendChild(el('span', 'gb-char-meta', `${docData.name} (${docData.id})`));

    // Project Character Document Switcher (when multiple `.character.json` files exist)
    const charDocs = typeof studioApi.listDocuments === 'function'
      ? studioApi.listDocuments({ suffix: '.character.json' })
      : [];
    if (Array.isArray(charDocs) && charDocs.length > 0) {
      const docSelect = el('select', 'gb-char-select');
      docSelect.dataset.role = 'character-doc-select';
      for (const cd of charDocs) {
        const shortName = String(cd.path || cd.id).split('/').pop().replace(/\.character\.json$/i, '');
        const opt = el('option', '', shortName);
        opt.value = cd.id;
        if (cd.id === activeDocId || cd.path === activeDocId) opt.selected = true;
        docSelect.appendChild(opt);
      }
      docSelect.addEventListener('change', async (ev) => {
        const nextId = ev.target.value;
        if (nextId && typeof studioApi.openDocument === 'function') {
          const opened = await studioApi.openDocument(nextId);
          if (opened) {
            activeDocId = opened.id ?? nextId;
            docData = normalizeCharacterDocument(opened.root ?? opened);
            selectedJoint = docData.skeleton.joints[0]?.name ?? '';
            clipId = docData.clips[0]?.id ?? '';
            render();
            pushPoseToEngine();
            void ensureOutfitModelLoaded();
          }
        }
      });
      toolbar.appendChild(docSelect);
    }

    toolbar.appendChild(el('div', 'gb-char-spacer'));

    const lightSelect = el('select', 'gb-char-select');
    lightSelect.dataset.role = 'lighting-select';
    for (const preset of Object.values(TURNTABLE_LIGHTING_PRESETS)) {
      const opt = el('option', '', preset.label);
      opt.value = preset.id;
      if (preset.id === lightingPreset) opt.selected = true;
      lightSelect.appendChild(opt);
    }
    lightSelect.addEventListener('change', (ev) => {
      lightingPreset = ev.target.value;
      pushPoseToEngine();
    });
    toolbar.appendChild(lightSelect);

    const resetBtn = el('button', 'gb-char-btn', 'Reset Pose');
    resetBtn.dataset.role = 'reset-pose';
    resetBtn.addEventListener('click', () => {
      commitOps([{ set: ['boneOffsets'], value: {} }], 'Reset bone offsets');
    });
    toolbar.appendChild(resetBtn);

    const saveBtn = el('button', 'gb-char-btn', 'Save');
    saveBtn.dataset.role = 'save-doc';
    saveBtn.addEventListener('click', () => {
      if (activeDocId) studioApi.save?.(activeDocId);
    });
    toolbar.appendChild(saveBtn);
    root.appendChild(toolbar);

    const grid = el('div', 'gb-char-grid');

    // Left column: Bone tree + Outfits & Sockets
    const leftCol = el('div', 'gb-char-col');

    const boneCard = el('div', 'gb-char-card');
    boneCard.appendChild(el('div', 'gb-char-card-header', `Skeleton (${docData.skeleton.joints.length} joints)`));
    const searchInput = el('input', 'gb-char-input');
    searchInput.type = 'search';
    searchInput.placeholder = 'Filter joints...';
    searchInput.value = boneQuery;
    searchInput.dataset.role = 'bone-search';
    searchInput.addEventListener('input', (ev) => {
      boneQuery = ev.target.value;
      render();
    });
    boneCard.appendChild(searchInput);

    const boneList = el('div', 'gb-char-bone-list');
    for (const joint of filterBoneTree(docData.skeleton.joints, boneQuery)) {
      const btn = el(
        'button',
        `gb-char-bone-item${joint.name === selectedJoint ? ' is-selected' : ''}`,
        `${'  '.repeat(joint.depth)}${joint.name}`,
      );
      btn.dataset.joint = joint.name;
      btn.addEventListener('click', () => {
        selectedJoint = joint.name;
        render();
      });
      boneList.appendChild(btn);
    }
    boneCard.appendChild(boneList);

    if (selectedJoint) {
      const curEuler = docData.boneOffsets[selectedJoint]?.eulerDeg ?? [0, 0, 0];
      const axes = ['Pitch', 'Yaw', 'Roll'];
      axes.forEach((axisLabel, axisIdx) => {
        const row = el('div', 'gb-char-slider-row');
        row.appendChild(el('span', '', `${selectedJoint} ${axisLabel}`));
        const slider = el('input');
        slider.type = 'range';
        slider.min = '-90';
        slider.max = '90';
        slider.step = '1';
        slider.value = String(curEuler[axisIdx] ?? 0);
        slider.dataset.role = `bone-euler-${axisIdx}`;
        const valSpan = el('span', 'gb-char-val', `${curEuler[axisIdx] ?? 0}°`);
        slider.addEventListener('input', (ev) => {
          const cur = docData.boneOffsets[selectedJoint]?.eulerDeg ?? [0, 0, 0];
          const next = [...cur];
          next[axisIdx] = Number(ev.target.value);
          valSpan.textContent = `${next[axisIdx]}°`;
          commitOps(
            buildBoneOffsetOps(selectedJoint, next, docData.boneOffsets[selectedJoint]?.translation ?? [0, 0, 0]),
            `Adjust ${selectedJoint} ${axisLabel}`,
            true,
            { skipRebuild: true },
          );
        });
        row.appendChild(slider);
        row.appendChild(valSpan);
        boneCard.appendChild(row);
      });
    }
    leftCol.appendChild(boneCard);

    // Outfit & Sockets card
    const outfitCard = el('div', 'gb-char-card');
    outfitCard.appendChild(el('div', 'gb-char-card-header', 'Outfits & Sockets'));
    const outfitSelect = el('select', 'gb-char-select');
    outfitSelect.dataset.role = 'outfit-select';
    for (const outfit of docData.outfits) {
      const opt = el('option', '', outfit.label);
      opt.value = outfit.id;
      if (outfit.id === docData.activeOutfitId) opt.selected = true;
      outfitSelect.appendChild(opt);
    }
    outfitSelect.addEventListener('change', (ev) => {
      commitOps(buildActiveOutfitOps(ev.target.value), 'Switch outfit variant');
    });
    outfitCard.appendChild(outfitSelect);

    docData.sockets.forEach((sock, sIdx) => {
      const sockRow = el('div', 'gb-char-meta', `Socket ${sock.id} → ${sock.parentJoint} [${sock.translation.map((n) => n.toFixed(2)).join(', ')}]`);
      sockRow.dataset.socket = sock.id;
      outfitCard.appendChild(sockRow);
      const offsetSlider = el('input');
      offsetSlider.type = 'range';
      offsetSlider.min = '-0.5';
      offsetSlider.max = '0.5';
      offsetSlider.step = '0.01';
      offsetSlider.value = String(sock.translation[1] ?? 0);
      offsetSlider.dataset.role = `socket-y-${sIdx}`;
      offsetSlider.addEventListener('input', (ev) => {
        const curSock = docData.sockets[sIdx] ?? sock;
        const nextT = [...curSock.translation];
        nextT[1] = Number(ev.target.value);
        sockRow.textContent = `Socket ${curSock.id} → ${curSock.parentJoint} [${nextT.map((n) => n.toFixed(2)).join(', ')}]`;
        commitOps(buildSocketTransformOps(sIdx, { translation: nextT }), `Move socket ${sock.id}`, true, { skipRebuild: true });
      });
      outfitCard.appendChild(offsetSlider);
    });
    leftCol.appendChild(outfitCard);
    grid.appendChild(leftCol);

    // Center column: Live 3D Character Stage + Animation Clips + Look-At + Two-Bone IK
    const centerCol = el('div', 'gb-char-col');

    const stageCard = el('div', 'gb-char-stage-card');
    const stageHdr = el('div', 'gb-char-card-header');
    stageHdr.appendChild(el('span', '', '3D Character Turntable'));

    const stageControls = el('div', 'gb-char-stage-controls');
    const focusModes = [
      ['body', 'Full Body'],
      ['bust', 'Face / Bust'],
    ];
    for (const [fid, flabel] of focusModes) {
      const fBtn = el('button', `gb-char-btn${stageFocus === fid ? ' is-active' : ''}`, flabel);
      fBtn.dataset.role = `stage-focus-${fid}`;
      fBtn.addEventListener('click', () => {
        stageFocus = fid;
        stagePanX = 0;
        stagePanY = 0;
        render();
        repaintStage();
      });
      stageControls.appendChild(fBtn);
    }

    const shadingModes = [
      ['textured', 'Textured'],
      ['clay', 'Clay'],
      ['wireframe', 'Wire'],
      ['normals', 'Normals'],
    ];
    for (const [sid, slabel] of shadingModes) {
      const sBtn = el('button', `gb-char-btn${stageShading === sid ? ' is-active' : ''}`, slabel);
      sBtn.dataset.role = `stage-shade-${sid}`;
      sBtn.addEventListener('click', () => {
        stageShading = sid;
        render();
        repaintStage();
      });
      stageControls.appendChild(sBtn);
    }

    const turnBtn = el('button', `gb-char-btn${stageTurntable ? ' is-active' : ''}`, 'Turn');
    turnBtn.dataset.role = 'stage-turntable';
    turnBtn.addEventListener('click', () => {
      stageTurntable = !stageTurntable;
      render();
      ensureAnimLoop();
      repaintStage();
    });
    stageControls.appendChild(turnBtn);

    const resetCamBtn = el('button', 'gb-char-btn', 'Reset Cam');
    resetCamBtn.dataset.role = 'stage-reset-cam';
    resetCamBtn.addEventListener('click', () => {
      stageYaw = 0.24;
      stagePitch = 0.14;
      stageZoom = 1.05;
      stagePanX = 0;
      stagePanY = 0;
      repaintStage();
    });
    stageControls.appendChild(resetCamBtn);
    stageHdr.appendChild(stageControls);
    stageCard.appendChild(stageHdr);

    const stageWrap = el('div', 'gb-char-stage-wrap');
    stageWrap.appendChild(stageCanvas);
    stageCard.appendChild(stageWrap);

    const stageHint = el('div', 'gb-char-stage-hint');
    stageHint.appendChild(el('span', '', 'Left-drag: Orbit · Right/Shift-drag: Pan · Wheel: Zoom'));
    const activeOutfitObj = currentOutfit();
    stageHint.appendChild(el('span', '', activeOutfitObj?.modelPath || ''));
    stageCard.appendChild(stageHint);
    centerCol.appendChild(stageCard);

    const animCard = el('div', 'gb-char-card');
    animCard.appendChild(el('div', 'gb-char-card-header', 'Animation & Hermite Crossfade'));
    const clipSelect = el('select', 'gb-char-select');
    clipSelect.dataset.role = 'clip-select';
    for (const clip of docData.clips) {
      const opt = el('option', '', `${clip.label} (${clip.durationS.toFixed(2)}s)`);
      opt.value = clip.id;
      if (clip.id === clipId) opt.selected = true;
      clipSelect.appendChild(opt);
    }
    clipSelect.addEventListener('change', (ev) => {
      clipId = ev.target.value;
      timeS = 0;
      pushPoseToEngine();
      render();
      repaintStage();
    });
    animCard.appendChild(clipSelect);

    const activeClipObj = docData.clips.find((c) => c.id === clipId) ?? docData.clips[0];
    const scrubRow = el('div', 'gb-char-slider-row');
    const playBtn = el('button', `gb-char-btn${playing ? ' is-active' : ''}`, playing ? 'Pause' : 'Play');
    playBtn.dataset.role = 'clip-play';
    playBtn.addEventListener('click', () => {
      playing = !playing;
      render();
      ensureAnimLoop();
      repaintStage();
    });
    scrubRow.appendChild(playBtn);
    const scrubber = el('input');
    scrubber.type = 'range';
    scrubber.min = '0';
    scrubber.max = String(activeClipObj?.durationS ?? 2);
    scrubber.step = '0.01';
    scrubber.value = String(timeS);
    scrubber.dataset.role = 'clip-scrubber';
    const scrubVal = el('span', 'gb-char-val', `${timeS.toFixed(2)}s`);
    scrubber.addEventListener('input', (ev) => {
      timeS = Number(ev.target.value);
      scrubVal.textContent = `${timeS.toFixed(2)}s`;
      pushPoseToEngine();
      repaintStage();
    });
    scrubRow.appendChild(scrubber);
    scrubRow.appendChild(scrubVal);
    animCard.appendChild(scrubRow);

    const fadeRow = el('div', 'gb-char-slider-row');
    const triggerFadeBtn = el('button', 'gb-char-btn', 'Crossfade');
    triggerFadeBtn.dataset.role = 'trigger-crossfade';
    triggerFadeBtn.addEventListener('click', () => {
      fromClipId = clipId;
      clipId = targetClipId;
      crossfadeElapsedS = crossfadeMs / 2000;
      pushPoseToEngine();
      render();
      repaintStage();
    });
    fadeRow.appendChild(triggerFadeBtn);
    const fadeSlider = el('input');
    fadeSlider.type = 'range';
    fadeSlider.min = '40';
    fadeSlider.max = '800';
    fadeSlider.step = '10';
    fadeSlider.value = String(crossfadeMs);
    fadeSlider.dataset.role = 'crossfade-ms';
    const hVal = hermiteFade(crossfadeElapsedS, crossfadeMs / 1000);
    const fadeVal = el('span', 'gb-char-val', `${crossfadeMs}ms (${hVal.toFixed(2)})`);
    fadeSlider.addEventListener('input', (ev) => {
      crossfadeMs = Number(ev.target.value);
      const hv = hermiteFade(crossfadeElapsedS, crossfadeMs / 1000);
      fadeVal.textContent = `${crossfadeMs}ms (${hv.toFixed(2)})`;
    });
    fadeRow.appendChild(fadeSlider);
    fadeRow.appendChild(fadeVal);
    animCard.appendChild(fadeRow);
    centerCol.appendChild(animCard);

    // Look-At Pad
    const lookCard = el('div', 'gb-char-card');
    lookCard.appendChild(el('div', 'gb-char-card-header', 'Procedural Look-At Target'));
    const pad = el('div', 'gb-char-lookpad');
    pad.dataset.role = 'look-at-pad';
    const dot = el('div', 'gb-char-lookpad-dot');
    const tx = Math.min(1, Math.max(-1, (docData.lookAt.target[0] ?? 0) / 1.5));
    const ty = Math.min(1, Math.max(-1, ((docData.lookAt.target[1] ?? 1.55) - (docData.lookAt.origin[1] ?? 1.55)) / 1.0));
    dot.style.left = `${(tx * 0.5 + 0.5) * 100}%`;
    dot.style.top = `${(0.5 - ty * 0.5) * 100}%`;
    pad.appendChild(dot);
    pad.addEventListener('click', (ev) => {
      const rect = pad.getBoundingClientRect?.() ?? { left: 0, top: 0, width: 200, height: 110 };
      const nx = rect.width > 0 ? ((ev.clientX ?? 100) - rect.left) / rect.width : 0.5;
      const ny = rect.height > 0 ? ((ev.clientY ?? 55) - rect.top) / rect.height : 0.5;
      const worldX = Number(((nx - 0.5) * 3.0).toFixed(3));
      const worldY = Number(((docData.lookAt.origin[1] ?? 1.55) + (0.5 - ny) * 2.0).toFixed(3));
      const worldZ = docData.lookAt.target[2] ?? 2.0;
      commitOps(buildLookAtTargetOps([worldX, worldY, worldZ], true), 'Move Look-At target', true);
    });
    lookCard.appendChild(pad);
    lookCard.appendChild(
      el(
        'div',
        'gb-char-meta',
        `Chain: ${docData.lookAt.chain.map((c) => `${c.joint} (${Math.round(c.weight * 100)}%)`).join(' → ')}`,
      ),
    );
    centerCol.appendChild(lookCard);

    // Two-Bone IK Chains
    const ikCard = el('div', 'gb-char-card');
    ikCard.appendChild(el('div', 'gb-char-card-header', 'Analytical Two-Bone IK'));
    docData.ikChains.forEach((ik, idx) => {
      const toggle = el('button', `gb-char-btn${ik.enabled ? ' is-active' : ''}`, `${ik.id} (${ik.enabled ? 'ON' : 'OFF'})`);
      toggle.dataset.role = `ik-toggle-${idx}`;
      toggle.addEventListener('click', () => {
        commitOps(buildIkChainOps(idx, { enabled: !ik.enabled }), `Toggle IK ${ik.id}`);
      });
      ikCard.appendChild(toggle);
      const wRow = el('div', 'gb-char-slider-row');
      wRow.appendChild(el('span', '', 'IK Weight'));
      const wSlider = el('input');
      wSlider.type = 'range';
      wSlider.min = '0';
      wSlider.max = '1';
      wSlider.step = '0.05';
      wSlider.value = String(ik.weight);
      wSlider.dataset.role = `ik-weight-${idx}`;
      const wVal = el('span', 'gb-char-val', ik.weight.toFixed(2));
      wSlider.addEventListener('input', (ev) => {
        const nextW = Number(ev.target.value);
        wVal.textContent = nextW.toFixed(2);
        commitOps(buildIkChainOps(idx, { weight: nextW }), `Adjust IK ${ik.id}`, true, { skipRebuild: true });
      });
      wRow.appendChild(wSlider);
      wRow.appendChild(wVal);
      ikCard.appendChild(wRow);
    });
    centerCol.appendChild(ikCard);
    grid.appendChild(centerCol);

    // Right column: Morphs & Expression Presets
    const rightCol = el('div', 'gb-char-col');
    const presetCard = el('div', 'gb-char-card');
    presetCard.appendChild(el('div', 'gb-char-card-header', 'Expression Presets'));
    const strip = el('div', 'gb-char-preset-strip');
    for (const presetName of Object.keys(docData.expressionPresets)) {
      const pBtn = el('button', 'gb-char-btn', presetName);
      pBtn.dataset.preset = presetName;
      pBtn.addEventListener('click', () => {
        commitOps(buildPresetApplyOps(docData, presetName), `Apply preset ${presetName}`);
      });
      strip.appendChild(pBtn);
    }
    presetCard.appendChild(strip);

    const intensityRow = el('div', 'gb-char-slider-row');
    intensityRow.appendChild(el('span', '', 'Intensity'));
    const intSlider = el('input');
    intSlider.type = 'range';
    intSlider.min = '0';
    intSlider.max = '2';
    intSlider.step = '0.05';
    intSlider.value = String(docData.expressionIntensity);
    intSlider.dataset.role = 'expression-intensity';
    const intVal = el('span', 'gb-char-val', `${docData.expressionIntensity.toFixed(2)}x`);
    intSlider.addEventListener('input', (ev) => {
      const v = Number(ev.target.value);
      intVal.textContent = `${v.toFixed(2)}x`;
      commitOps(
        [{ set: ['expressionIntensity'], value: v }],
        'Adjust expression intensity',
        true,
        { skipRebuild: true },
      );
    });
    intensityRow.appendChild(intSlider);
    intensityRow.appendChild(intVal);
    presetCard.appendChild(intensityRow);
    rightCol.appendChild(presetCard);

    const groups = groupMorphTargets(docData.morphTargets);
    for (const [groupName, targets] of Object.entries(groups)) {
      if (!targets.length) continue;
      const gCard = el('div', 'gb-char-card');
      gCard.appendChild(el('div', 'gb-char-card-header', `${groupName} (${targets.length})`));
      for (const target of targets) {
        const curVal = docData.morphWeights[target.id] ?? target.defaultWeight ?? 0;
        const row = el('div', 'gb-char-slider-row');
        row.appendChild(el('span', '', target.label));
        const mSlider = el('input');
        mSlider.type = 'range';
        mSlider.min = String(target.min);
        mSlider.max = String(target.max);
        mSlider.step = '0.01';
        mSlider.value = String(curVal);
        mSlider.dataset.morph = target.id;
        const mVal = el('span', 'gb-char-val', curVal.toFixed(2));
        mSlider.addEventListener('input', (ev) => {
          const nextV = Number(ev.target.value);
          mVal.textContent = nextV.toFixed(2);
          commitOps(buildMorphWeightOps(target.id, nextV), `Morph ${target.id}`, true, { skipRebuild: true });
        });
        row.appendChild(mSlider);
        row.appendChild(mVal);
        gCard.appendChild(row);
      }
      rightCol.appendChild(gCard);
    }

    grid.appendChild(rightCol);
    root.appendChild(grid);
    if (typeof requestAnimationFrame === 'function') {
      requestAnimationFrame(() => repaintStage());
    } else {
      repaintStage();
    }
  };

  syncFromStudio();
  render();
  pushPoseToEngine();
  void ensureOutfitModelLoaded();

  if (typeof studioApi.subscribe === 'function') {
    const unsub = studioApi.subscribe(() => {
      if (!active || destroyed) return;
      const focusedTag = dom.activeElement?.tagName;
      if (focusedTag === 'INPUT' || focusedTag === 'SELECT') return;
      const prevId = activeDocId;
      const prevOutfit = docData.activeOutfitId;
      syncFromStudio();
      if (activeDocId !== prevId || docData.activeOutfitId !== prevOutfit) {
        render();
        void ensureOutfitModelLoaded();
      }
    });
    if (typeof unsub === 'function') unsubscribers.push(unsub);
  }

  const destroy = () => {
    if (destroyed) return;
    destroyed = true;
    playing = false;
    stageTurntable = false;
    if (stageRaf && typeof cancelAnimationFrame === 'function') {
      cancelAnimationFrame(stageRaf);
      stageRaf = 0;
    }
    while (unsubscribers.length) {
      const fn = unsubscribers.pop();
      try {
        fn();
      } catch {
        // Ignore teardown errors from external listeners.
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
      pushPoseToEngine();
      void ensureOutfitModelLoaded();
    },
    setDocument(pathOrId, rawDoc) {
      activeDocId = pathOrId;
      docData = normalizeCharacterDocument(rawDoc?.root ?? rawDoc);
      selectedJoint = docData.skeleton.joints[0]?.name ?? '';
      clipId = docData.clips[0]?.id ?? '';
      render();
      pushPoseToEngine();
      void ensureOutfitModelLoaded();
    },
    onActivate() {
      active = true;
      syncFromStudio();
      render();
      pushPoseToEngine();
      void ensureOutfitModelLoaded();
    },
    onDeactivate() {
      active = false;
      playing = false;
      stageTurntable = false;
      if (stageRaf && typeof cancelAnimationFrame === 'function') {
        cancelAnimationFrame(stageRaf);
        stageRaf = 0;
      }
    },
  };
}
