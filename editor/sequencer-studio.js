/**
 * Spatio-Temporal Neuro-Symbolic Sequencer Studio (`*.seq.json`, schema `ganbate.sequence/1`).
 *
 * Unifies:
 * 1. Primary Academic Showcase: Articulated Character Rigging & IK Pose Choreography (`character-ik-rig`)
 *    - 2-Bone Analytic IK (`shoulder -> elbow -> wrist_hand`), Gaze Cone Constraints, Zero Foot-Slip Contact Lock
 * 2. Architectural & Interior Choreography: Yozakura Pavilion (`yozakura-pavilion`)
 * 3. Live Game Production Choreography: Ganbate Multiplayer Tabletop (`ganbate-poker-salon`)
 *
 * Core Neuro-Symbolic Capabilities:
 * - Inverse Spatio-Temporal Lifting ($\mathcal{L}_t: \mathbb{R}^3 \times [0, T] \to \mathcal{G}_{\text{rel}}(t)$)
 * - Minimal-Perturbation Trajectory & Kinematic Manifold Projection ($\hat{\tau}(t) \to \tau^*(t)$)
 * - Hallucination Trajectory Ghost ($\hat{\tau}$) vs. Projected Valid Arc ($\tau^*$)
 * - 2-Agent Temporal Choreography Conflict Resolution
 */

export const SEQUENCE_SCHEMA = 'ganbate.sequence/1';
export const DEFAULT_ACADEMIC_SEQUENCER_PRESET = 'character-ik-rig';

function clamp(v, lo, hi) {
  return Math.min(hi, Math.max(lo, v));
}

function vec3Sub(a, b) {
  return [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
}

function vec3Add(a, b) {
  return [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
}

function vec3Scale(a, s) {
  return [a[0] * s, a[1] * s, a[2] * s];
}

function vec3Len(a) {
  return Math.hypot(a[0], a[1], a[2]);
}

function vec3Norm(a) {
  const l = vec3Len(a);
  return l > 1e-6 ? [a[0] / l, a[1] / l, a[2] / l] : [0, 1, 0];
}

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

/**
 * Analytic 2-bone Inverse Kinematics solver (`shoulder -> elbow -> wrist_hand`)
 * with kinematic reach-sphere manifold projection ($\mathcal{C}_{\text{IK}}$)
 * and support-surface collision clamping.
 */
export function solveTwoBoneIkChain(
  shoulderPos = [0, 1.32, 0],
  rawTargetPos = [0.35, 1.05, 0.42],
  options = {},
) {
  const upperLen = Math.max(0.05, Number(options.upperLen ?? 0.36));
  const lowerLen = Math.max(0.05, Number(options.lowerLen ?? 0.36));
  const maxReach = Math.max(0.1, Number(options.maxReachRadius ?? upperLen + lowerLen - 0.005));
  const minReach = Math.max(0.04, Math.abs(upperLen - lowerLen) + 0.01);
  const surfaceMinY = Number.isFinite(options.surfaceMinY) ? Number(options.surfaceMinY) : -Infinity;
  const swivelRad = ((Number(options.swivelDeg ?? 18) || 0) * Math.PI) / 180;

  let projectedTarget = [...rawTargetPos];
  let surfaceClamped = false;
  if (projectedTarget[1] < surfaceMinY) {
    projectedTarget[1] = surfaceMinY;
    surfaceClamped = true;
  }

  const delta = vec3Sub(projectedTarget, shoulderPos);
  const rawDist = vec3Len(delta);
  let reachClamped = false;
  if (rawDist > maxReach) {
    projectedTarget = vec3Add(shoulderPos, vec3Scale(vec3Norm(delta), maxReach));
    if (projectedTarget[1] < surfaceMinY) {
      projectedTarget[1] = surfaceMinY;
    }
    reachClamped = true;
  } else if (rawDist < minReach) {
    projectedTarget = vec3Add(shoulderPos, vec3Scale(vec3Norm(delta), minReach));
    reachClamped = true;
  }

  const effDelta = vec3Sub(projectedTarget, shoulderPos);
  const d = clamp(vec3Len(effDelta), minReach, upperLen + lowerLen - 1e-4);
  const cosShoulder = clamp(
    (upperLen * upperLen + d * d - lowerLen * lowerLen) / (2 * upperLen * d),
    -1,
    1,
  );
  const cosElbow = clamp(
    (upperLen * upperLen + lowerLen * lowerLen - d * d) / (2 * upperLen * lowerLen),
    -1,
    1,
  );
  const shoulderAngle = Math.acos(cosShoulder);
  const elbowBendAngleDeg = Number(((Math.PI - Math.acos(cosElbow)) * (180 / Math.PI)).toFixed(2));

  const dir = vec3Norm(effDelta);
  const along = Math.cos(shoulderAngle) * upperLen;
  const perp = Math.sin(shoulderAngle) * upperLen;
  const elbowPos = [
    Number((shoulderPos[0] + dir[0] * along + Math.sin(swivelRad) * perp * 0.45).toFixed(4)),
    Number((shoulderPos[1] + dir[1] * along - Math.cos(swivelRad) * perp * 0.75).toFixed(4)),
    Number((shoulderPos[2] + dir[2] * along - 0.12 * perp).toFixed(4)),
  ];

  const l2Perturbation = Number(vec3Len(vec3Sub(projectedTarget, rawTargetPos)).toFixed(4));

  return {
    shoulderPos: [...shoulderPos],
    elbowPos,
    wristPos: projectedTarget.map((v) => Number(v.toFixed(4))),
    rawTargetPos: [...rawTargetPos],
    reachable: !reachClamped && !surfaceClamped,
    reachClamped,
    surfaceClamped,
    elbowBendAngleDeg,
    l2Perturbation,
  };
}

/**
 * Inverse Spatio-Temporal Semantic Lifting ($\mathcal{L}_t: \mathbb{R}^3 \times [0, T] \to \mathcal{G}_{\text{rel}}(t)$).
 * Given a node moved at beat $k$ to `proposedEndPos`, infers symbolic relations (`reaching_for`,
 * `on_top_of`, `gaze_at`, `contact_lock`, `in_front_of`, `left_of`, `right_of`).
 */
export function inferTemporalBeatRelations(nodeId, proposedEndPos, sceneNodes = [], options = {}) {
  const pos = Array.isArray(proposedEndPos) && proposedEndPos.length >= 3 ? proposedEndPos : [0, 0, 0];
  const edges = [];
  const kind = options.trackKind ?? 'transform';

  if (options.footContactLock || /foot|ankle/i.test(nodeId)) {
    edges.push({
      from: nodeId,
      rel: 'contact_lock',
      to: 'floor_plane',
      slipMm: 0,
    });
  }

  let closestNode = null;
  let closestDist = Infinity;

  for (const node of sceneNodes) {
    if (!node || node.id === nodeId) continue;
    const nPos = node.position ?? [0, 0, 0];
    const half = node.halfExtents ?? [0.4, 0.4, 0.4];
    const dx = pos[0] - nPos[0];
    const dy = pos[1] - nPos[1];
    const dz = pos[2] - nPos[2];
    const dist = Math.hypot(dx, dy, dz);

    const withinFootprint =
      Math.abs(dx) <= half[0] + 0.15 && Math.abs(dz) <= half[2] + 0.15;
    const topY = nPos[1] + half[1];
    if (withinFootprint && Math.abs(pos[1] - topY) <= 0.28) {
      edges.push({
        from: nodeId,
        rel: 'on_top_of',
        to: node.id,
      });
    }

    if (dist < closestDist) {
      closestDist = dist;
      closestNode = node;
    }
  }

  if (closestNode) {
    const nPos = closestNode.position ?? [0, 0, 0];
    const dx = pos[0] - nPos[0];
    const dz = pos[2] - nPos[2];
    if (kind === 'skeletal' || /hand|wrist|ik/i.test(nodeId)) {
      if (closestDist <= 0.48) {
        edges.push({
          from: nodeId,
          rel: 'reaching_for',
          to: closestNode.id,
        });
      }
      if (options.gazeTargetId) {
        edges.push({
          from: 'head_gaze',
          rel: 'gaze_at',
          to: options.gazeTargetId,
        });
      }
    }
    const spatialRel =
      Math.abs(dx) >= Math.abs(dz)
        ? dx < 0
          ? 'left_of'
          : 'right_of'
        : dz >= 0
          ? 'in_front_of'
          : 'behind';
    edges.push({
      from: nodeId,
      rel: spatialRel,
      to: closestNode.id,
    });
  }

  return {
    nodeId,
    edges,
    anchorId: closestNode?.id ?? null,
    distanceToAnchor: Number.isFinite(closestDist) ? Number(closestDist.toFixed(4)) : null,
  };
}

/**
 * Minimal-Perturbation Trajectory & Kinematic Manifold Projection ($\hat{\tau}(t) \to \tau^*(t)$).
 * Checks:
 * 1. Kinematic IK reach sphere & joint limits (if `rigConfig` is present)
 * 2. Obstacle AABB non-penetration along the trajectory spline (`arcLift` clearance projection)
 * 3. Support-surface & foot-contact invariants
 */
export function projectTrajectorySection(section, obstacles = [], options = {}) {
  const fromPos = [...(section.fromPos ?? [0, 0.9, 0])];
  const rawToPos = [...(section.toPos ?? [0, 0.9, 0.5])];
  const rawArcLift = Number(section.arcLift ?? 0.14);
  const violationReasons = [];

  let projectedFromPos = [...fromPos];
  let projectedToPos = [...rawToPos];
  let projectedArcLift = rawArcLift;
  let ikSolution = null;

  // 1. Foot-Contact Invariant (Zero Foot-Sliding)
  if (section.footContactLock || options.footContactLock) {
    const floorY = Number(options.floorY ?? 0);
    if (Math.abs(projectedToPos[1] - floorY) > 1e-3 || vec3Len(vec3Sub(projectedToPos, projectedFromPos)) > 1e-3) {
      violationReasons.push('foot_contact_slip_locked');
      projectedFromPos[1] = floorY;
      projectedToPos = [...projectedFromPos];
      projectedArcLift = 0;
    }
  }

  // 2. Articulated IK Reach Manifold Projection
  if (section.ikChain || options.ikChain) {
    const ikCfg = { ...(options.ikChain ?? {}), ...(section.ikChain ?? {}) };
    ikSolution = solveTwoBoneIkChain(ikCfg.shoulderPos ?? [0, 1.32, 0], projectedToPos, ikCfg);
    if (!ikSolution.reachable) {
      if (ikSolution.reachClamped) violationReasons.push('ik_reach_manifold_clamped');
      if (ikSolution.surfaceClamped) violationReasons.push('ik_surface_floor_clamped');
      projectedToPos = [...ikSolution.wristPos];
    }
  }

  // 3. Obstacle AABB Clearance along Trajectory Spline
  const sampleCount = 16;
  for (const obs of obstacles) {
    const c = obs.position ?? [0, 0, 0];
    const h = obs.halfExtents ?? [0.5, 0.4, 0.5];
    const topY = c[1] + h[1];

    // Check if endpoint penetrates obstacle AABB
    if (
      Math.abs(projectedToPos[0] - c[0]) < h[0] &&
      Math.abs(projectedToPos[2] - c[2]) < h[2] &&
      projectedToPos[1] < topY &&
      projectedToPos[1] > c[1] - h[1]
    ) {
      violationReasons.push(`endpoint_penetrates_${obs.id ?? 'obstacle'}`);
      projectedToPos[1] = Number((topY + 0.02).toFixed(4));
    }

    // Check mid-flight samples u in (0.15, 0.85)
    for (let i = 2; i < sampleCount - 1; i++) {
      const u = i / sampleCount;
      const basePos = lerp3(projectedFromPos, projectedToPos, u);
      const liftY = basePos[1] + 4 * u * (1 - u) * projectedArcLift;
      if (
        Math.abs(basePos[0] - c[0]) < h[0] &&
        Math.abs(basePos[2] - c[2]) < h[2] &&
        liftY < topY + 0.03 &&
        liftY > c[1] - h[1]
      ) {
        const neededLift = (topY + 0.06 - basePos[1]) / Math.max(0.2, 4 * u * (1 - u));
        if (neededLift > projectedArcLift) {
          projectedArcLift = Number(neededLift.toFixed(4));
          if (!violationReasons.includes(`arc_clearance_${obs.id ?? 'obstacle'}`)) {
            violationReasons.push(`arc_clearance_${obs.id ?? 'obstacle'}`);
          }
        }
      }
    }
  }

  const ghostArcPoints = [];
  const projectedArcPoints = [];
  for (let i = 0; i <= sampleCount; i++) {
    const u = i / sampleCount;
    const rawPt = lerp3(fromPos, rawToPos, u);
    rawPt[1] += 4 * u * (1 - u) * rawArcLift;
    ghostArcPoints.push(rawPt.map((v) => Number(v.toFixed(4))));

    const projPt = lerp3(projectedFromPos, projectedToPos, u);
    projPt[1] += 4 * u * (1 - u) * projectedArcLift;
    projectedArcPoints.push(projPt.map((v) => Number(v.toFixed(4))));
  }

  const endpointDelta = vec3Len(vec3Sub(projectedToPos, rawToPos));
  const arcDelta = Math.abs(projectedArcLift - rawArcLift);
  const l2Perturbation = Number(Math.hypot(endpointDelta, arcDelta).toFixed(4));
  const lifted = inferTemporalBeatRelations(
    section.target ?? section.id ?? 'node',
    projectedToPos,
    obstacles,
    {
      trackKind: section.ikChain ? 'skeletal' : 'transform',
      footContactLock: Boolean(section.footContactLock || options.footContactLock),
      gazeTargetId: section.gazeTargetId,
    },
  );

  return {
    projectedSection: {
      ...section,
      fromPos: projectedFromPos,
      toPos: projectedToPos,
      arcLift: projectedArcLift,
    },
    violated: violationReasons.length > 0,
    violationReasons,
    l2Perturbation,
    ikSolution,
    ghostArcPoints,
    projectedArcPoints,
    liftedEdges: lifted.edges,
  };
}

/**
 * Multi-Agent Temporal Conflict Resolution (`resolveTemporalAgentConflict`).
 * When Agent A relocates an anchor/obstacle (e.g. desk prop or torso stance) while
 * Agent B authors a reach/prop trajectory bound to that anchor at overlapping beats,
 * merges both intents by re-binding Agent B's target through its symbolic edge
 * and projecting onto the collision-free kinematic manifold $\mathcal{C}_{\text{traj}}$.
 */
export function resolveTemporalAgentConflict(agentASection, agentBSection, obstacles = []) {
  const anchorShift = vec3Sub(
    agentASection.toPos ?? [0, 0, 0],
    agentASection.fromPos ?? [0, 0, 0],
  );
  const rawMergedTarget =
    agentBSection.relativeToAnchor === false
      ? [...(agentBSection.toPos ?? [0, 0, 0])]
      : vec3Add(agentBSection.toPos ?? [0, 0, 0], anchorShift);

  const overlapStart = Math.max(Number(agentASection.startMs ?? 0), Number(agentBSection.startMs ?? 0));
  const overlapEnd = Math.min(Number(agentASection.endMs ?? 1000), Number(agentBSection.endMs ?? 1000));
  const temporalOverlapMs = Math.max(0, overlapEnd - overlapStart);

  const candidateB = {
    ...agentBSection,
    toPos: rawMergedTarget,
  };
  const projection = projectTrajectorySection(candidateB, obstacles);

  return {
    resolvedSection: projection.projectedSection,
    temporalOverlapMs,
    anchorShift: anchorShift.map((v) => Number(v.toFixed(4))),
    violatedBeforeProjection: projection.violated,
    violationReasons: projection.violationReasons,
    l2Perturbation: projection.l2Perturbation,
    ghostArcPoints: projection.ghostArcPoints,
    projectedArcPoints: projection.projectedArcPoints,
    liftedEdges: projection.liftedEdges,
    ikSolution: projection.ikSolution,
  };
}

export const SEQUENCER_DOMAIN_PRESETS = {
  'character-ik-rig': {
    id: 'character-ik-rig',
    label: '🦾 Character Rigging & IK Lab (Academic Default)',
    domain: 'character-rigging',
    description:
      'Articulated humanoid rig (Pelvis → Spine → 2-Bone Arm IK + Gaze Cone + Zero Foot-Slip Lock) interacting with a studio desk.',
    obstacles: [
      { id: 'work_desk', position: [0.0, 0.42, 0.48], halfExtents: [0.65, 0.42, 0.36] },
      { id: 'artifact_orb', position: [0.28, 0.86, 0.44], halfExtents: [0.09, 0.09, 0.09] },
      { id: 'target_pedestal', position: [-0.24, 0.86, 0.46], halfExtents: [0.12, 0.04, 0.12] },
    ],
  },
  'yozakura-pavilion': {
    id: 'yozakura-pavilion',
    label: '🏯 Yozakura Pavilion Interior Choreography',
    domain: 'architectural-interior',
    description:
      'Sliding shoji screen, lantern warm-up curve, clearance-verified walkway navigation, and parabolic tea placement onto the low table.',
    obstacles: [
      { id: 'tea_table', position: [0.0, 0.38, 0.0], halfExtents: [0.65, 0.38, 0.45] },
      { id: 'stone_lantern', position: [1.05, 0.55, -0.85], halfExtents: [0.24, 0.55, 0.24] },
      { id: 'zabuton_south', position: [0.0, 0.06, 0.78], halfExtents: [0.32, 0.06, 0.32] },
    ],
  },
  'ganbate-poker-salon': {
    id: 'ganbate-poker-salon',
    label: '🎴 Ganbate Tabletop Salon (Production Runtime)',
    domain: 'multiplayer-game',
    description:
      '6/9-Max card deal flights, parabolic chip wagers, street progression, and showdown pot sweep.',
    obstacles: [
      { id: 'poker_table', position: [0.0, 0.42, 0.0], halfExtents: [1.25, 0.42, 0.78] },
      { id: 'pot_center', position: [0.0, 0.86, -0.12], halfExtents: [0.22, 0.06, 0.22] },
      { id: 'seat_hero', position: [0.0, 0.85, 0.72], halfExtents: [0.25, 0.05, 0.18] },
    ],
  },
};

export function createPresetSequenceDocument(presetId = DEFAULT_ACADEMIC_SEQUENCER_PRESET) {
  if (presetId === 'character-ik-rig') {
    return {
      schema: SEQUENCE_SCHEMA,
      name: 'character_ik_reach_choreography',
      presetId: 'character-ik-rig',
      fps: 60,
      durationMs: 2400,
      looping: false,
      obstacles: SEQUENCER_DOMAIN_PRESETS['character-ik-rig'].obstacles,
      markers: [
        { id: 'mk_gaze_lock', label: '01 · Gaze & Foot Lock', atMs: 0 },
        { id: 'mk_ik_reach', label: '02 · 2-Bone IK Reach Arc', atMs: 600 },
        { id: 'mk_socket_place', label: '03 · Pedestal Socket Place', atMs: 1500 },
      ],
      beats: [
        {
          id: 'beat_01_gaze_stance',
          label: '01 · GAZE_AND_STANCE_LOCK',
          startMs: 0,
          endMs: 600,
          edges: [
            { from: 'foot_L', rel: 'contact_lock', to: 'floor_plane', slipMm: 0 },
            { from: 'foot_R', rel: 'contact_lock', to: 'floor_plane', slipMm: 0 },
            { from: 'head_gaze', rel: 'gaze_at', to: 'artifact_orb' },
          ],
        },
        {
          id: 'beat_02_ik_reach_grasp',
          label: '02 · IK_REACH_AND_GRASP',
          startMs: 600,
          endMs: 1500,
          edges: [
            { from: 'wrist_hand_R', rel: 'reaching_for', to: 'artifact_orb' },
            { from: 'wrist_hand_R', rel: 'on_top_of', to: 'work_desk' },
          ],
        },
        {
          id: 'beat_03_socket_transfer',
          label: '03 · SOCKET_TRANSFER_SETTLE',
          startMs: 1500,
          endMs: 2400,
          edges: [
            { from: 'artifact_orb', rel: 'on_top_of', to: 'target_pedestal' },
            { from: 'wrist_hand_R', rel: 'in_front_of', to: 'target_pedestal' },
          ],
        },
      ],
      tracks: [
        {
          id: 'trk_cam',
          kind: 'camera',
          label: 'Rig Inspection Camera',
          muted: false,
          sections: [
            {
              id: 'cam_rig_orbit',
              startMs: 0,
              endMs: 2400,
              view: 'character_rig_closeup',
              fromEye: [0.45, 1.48, 1.85],
              toEye: [-0.25, 1.42, 1.65],
              fromTarget: [0, 1.05, 0.35],
              toTarget: [0, 1.0, 0.42],
              fovFrom: 38,
              fovTo: 34,
              curve: { type: 'cubicBezier', x1: 0.22, y1: 1.0, x2: 0.36, y2: 1.0 },
            },
          ],
        },
        {
          id: 'trk_actor',
          kind: 'skeletal',
          label: 'Rei Full-Body IK Rig',
          target: 'rei_academic_rig',
          muted: false,
          sections: [
            {
              id: 'ik_upper_body_reach',
              startMs: 0,
              endMs: 2400,
              clip: 'ik_reach_and_place',
              secondaryClip: 'stance_breathe_idle',
              clipOffsetS: 0,
              speed: 1.0,
              blendInMs: 120,
              blendOutMs: 160,
              weight: 1.0,
            },
          ],
        },
        {
          id: 'trk_hand_ik',
          kind: 'transform',
          label: 'Right Hand IK Target (wrist_hand_R)',
          target: 'wrist_hand_R',
          muted: false,
          sections: [
            {
              id: 'sec_hand_reach_arc',
              startMs: 300,
              endMs: 1500,
              fromPos: [0.22, 1.02, 0.18],
              toPos: [0.28, 0.88, 0.44],
              arcLift: 0.22,
              fromEulerDeg: [0, 0, 0],
              toEulerDeg: [-18, 24, 0],
              fromScale: [1, 1, 1],
              toScale: [1, 1, 1],
              gazeTargetId: 'artifact_orb',
              ikChain: {
                shoulderPos: [0.18, 1.32, 0.0],
                upperLen: 0.36,
                lowerLen: 0.36,
                maxReachRadius: 0.71,
                surfaceMinY: 0.84,
                swivelDeg: 18,
              },
              curve: { type: 'cubicBezier', x1: 0.22, y1: 1.0, x2: 0.36, y2: 1.0 },
            },
          ],
        },
        {
          id: 'trk_vfx',
          kind: 'vfx',
          label: 'IK Contact Socket Indicator',
          target: 'target_pedestal',
          muted: false,
          triggers: [
            {
              id: 'vfx_socket_lock',
              atMs: 1200,
              effect: 'ik_contact_ring',
              durationMs: 600,
              scale: 1.0,
              burstMultiplier: 1.0,
            },
          ],
        },
        {
          id: 'trk_audio',
          kind: 'audio',
          label: 'Rig Foley & Contact Cues',
          bus: 'sfx',
          muted: false,
          triggers: [
            {
              id: 'aud_cloth_settle',
              atMs: 1200,
              cue: 'sfx/rig_contact_lock',
              gainDb: 0,
              pitchSemitones: 0,
            },
          ],
        },
      ],
    };
  }

  if (presetId === 'yozakura-pavilion') {
    const base = createDefaultSequenceDocument('yozakura_pavilion_ceremony');
    return {
      ...base,
      presetId: 'yozakura-pavilion',
      obstacles: SEQUENCER_DOMAIN_PRESETS['yozakura-pavilion'].obstacles,
      beats: [
        {
          id: 'beat_01_shoji_open',
          label: '01 · SHOJI_AND_LANTERN_WARMUP',
          startMs: 0,
          endMs: 800,
          edges: [
            { from: 'shoji_screen', rel: 'left_of', to: 'alcove_entrance' },
            { from: 'stone_lantern', rel: 'right_of', to: 'tea_table' },
          ],
        },
        {
          id: 'beat_02_tea_serving_arc',
          label: '02 · TEA_PLACEMENT_ARC',
          startMs: 800,
          endMs: 2400,
          edges: [
            { from: 'tea_cup', rel: 'on_top_of', to: 'tea_table' },
            { from: 'tea_cup', rel: 'in_front_of', to: 'zabuton_south' },
          ],
        },
      ],
    };
  }

  const base = createDefaultSequenceDocument('stage_intro');
  return {
    ...base,
    presetId: 'ganbate-poker-salon',
    obstacles: SEQUENCER_DOMAIN_PRESETS['ganbate-poker-salon'].obstacles,
  };
}

export function createDefaultSequenceDocument(name = 'stage_intro') {
  return {
    schema: SEQUENCE_SCHEMA,
    name,
    presetId: DEFAULT_ACADEMIC_SEQUENCER_PRESET,
    fps: 60,
    durationMs: 2400,
    looping: false,
    obstacles: SEQUENCER_DOMAIN_PRESETS[DEFAULT_ACADEMIC_SEQUENCER_PRESET].obstacles,
    beats: [
      {
        id: 'beat_01_gaze_stance',
        label: '01 · GAZE_AND_STANCE_LOCK',
        startMs: 0,
        endMs: 600,
        edges: [
          { from: 'foot_L', rel: 'contact_lock', to: 'floor_plane', slipMm: 0 },
          { from: 'foot_R', rel: 'contact_lock', to: 'floor_plane', slipMm: 0 },
          { from: 'head_gaze', rel: 'gaze_at', to: 'artifact_orb' },
        ],
      },
      {
        id: 'beat_02_ik_reach',
        label: '02 · IK_REACH_AND_GRASP',
        startMs: 600,
        endMs: 1500,
        edges: [
          { from: 'wrist_hand_R', rel: 'reaching_for', to: 'artifact_orb' },
          { from: 'wrist_hand_R', rel: 'on_top_of', to: 'work_desk' },
        ],
      },
      {
        id: 'beat_03_socket_place',
        label: '03 · SOCKET_TRANSFER_SETTLE',
        startMs: 1500,
        endMs: 2400,
        edges: [
          { from: 'artifact_orb', rel: 'on_top_of', to: 'target_pedestal' },
          { from: 'wrist_hand_R', rel: 'in_front_of', to: 'target_pedestal' },
        ],
      },
    ],
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
        label: 'Lead Actor (IK Rig)',
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
        label: 'Right Hand IK / Hero Prop',
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
            ikChain: {
              shoulderPos: [0.0, 1.32, 0.0],
              upperLen: 0.38,
              lowerLen: 0.38,
              maxReachRadius: 0.74,
              surfaceMinY: 0.84,
              swivelDeg: 16,
            },
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
  const presetId = String(raw.presetId ?? fallback.presetId ?? DEFAULT_ACADEMIC_SEQUENCER_PRESET);
  return {
    schema: SEQUENCE_SCHEMA,
    name: String(raw.name ?? fallback.name),
    presetId,
    fps: Math.max(1, Number(raw.fps ?? 60)),
    durationMs: Math.max(1, Number(raw.durationMs ?? 2400)),
    looping: Boolean(raw.looping),
    obstacles: Array.isArray(raw.obstacles)
      ? raw.obstacles.map((o) => ({ ...o }))
      : SEQUENCER_DOMAIN_PRESETS[presetId]?.obstacles ?? fallback.obstacles,
    beats: Array.isArray(raw.beats)
      ? raw.beats.map((b) => ({
          ...b,
          edges: Array.isArray(b.edges) ? b.edges.map((e) => ({ ...e })) : [],
        }))
      : fallback.beats,
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

export function evaluateSequenceAt(rawDoc, rawTimeMs, options = {}) {
  const doc = normalizeSequenceDocument(rawDoc);
  const autoProject = options.autoProject !== false;
  const dur = Math.max(1, doc.durationMs);
  const clampedMs = doc.looping
    ? ((Number(rawTimeMs || 0) % dur) + dur) % dur
    : Math.min(dur, Math.max(0, Number(rawTimeMs || 0)));

  let camera = null;
  const skeletal = [];
  const transforms = [];
  const activeBeat =
    (doc.beats ?? []).find((b) => clampedMs >= b.startMs && clampedMs <= b.endMs) ??
    doc.beats?.[0] ??
    null;

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
          const projection = autoProject
            ? projectTrajectorySection(sec, doc.obstacles ?? [])
            : {
                projectedSection: sec,
                violated: false,
                violationReasons: [],
                l2Perturbation: 0,
                ikSolution: sec.ikChain
                  ? solveTwoBoneIkChain(sec.ikChain.shoulderPos, sec.toPos, sec.ikChain)
                  : null,
                liftedEdges: [],
              };
          const effSec = projection.projectedSection;
          const spanMs = Math.max(1, effSec.endMs - effSec.startMs);
          const rawT = (clampedMs - effSec.startMs) / spanMs;
          const eased = sampleSequenceCurve(effSec.curve, rawT, spanMs / 1000);
          const pos = lerp3(effSec.fromPos ?? [0, 0, 0], effSec.toPos ?? [0, 0, 0], eased);
          const lift = 4 * rawT * (1 - rawT) * Number(effSec.arcLift ?? 0);
          pos[1] += lift;

          const ikPose = effSec.ikChain
            ? solveTwoBoneIkChain(effSec.ikChain.shoulderPos, pos, effSec.ikChain)
            : null;

          transforms.push({
            trackId: track.id,
            target: track.target,
            sectionId: sec.id,
            position: pos,
            eulerDeg: lerp3(effSec.fromEulerDeg ?? [0, 0, 0], effSec.toEulerDeg ?? [0, 0, 0], eased),
            scale: lerp3(effSec.fromScale ?? [1, 1, 1], effSec.toScale ?? [1, 1, 1], eased),
            ikPose,
            constraintProjected: projection.violated,
            l2Perturbation: projection.l2Perturbation,
            liftedEdges: projection.liftedEdges,
          });
        }
      }
    }
  }

  return {
    timeMs: clampedMs,
    frame: Math.floor((clampedMs / 1000) * doc.fps),
    activeBeat,
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

export function buildTransformTargetOps(trackIndex, sectionIndex, toPos, arcLift) {
  const ops = [
    {
      set: ['tracks', String(trackIndex), 'sections', String(sectionIndex), 'toPos'],
      value: toPos.map((v) => Number(Number(v).toFixed(4))),
    },
  ];
  if (arcLift !== undefined) {
    ops.push({
      set: ['tracks', String(trackIndex), 'sections', String(sectionIndex), 'arcLift'],
      value: Number(Number(arcLift).toFixed(4)),
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
  let autoProjectConstraints = true;
  let lastConflictReport = null;
  let playbackSpeed = 1.0;
  let selectedItem = { trackIndex: 2, itemType: 'section', itemIndex: 0 };
  let active = true;
  let destroyed = false;
  const unsubscribers = [];

  const syncFromStudio = () => {
    const activeDoc = studioApi.getActiveDocument?.();
    if (
      activeDoc &&
      (String(activeDoc.path ?? activeDoc.id ?? '').endsWith('.seq.json') ||
        activeDoc.root?.schema === SEQUENCE_SCHEMA)
    ) {
      activeDocId = activeDoc.id ?? activeDoc.path;
      docData = normalizeSequenceDocument(activeDoc.root ?? activeDoc.data ?? activeDoc);
    }
  };

  const broadcastPlayhead = (prevMs, nextMs) => {
    const frame = evaluateSequenceAt(docData, nextMs, { autoProject: autoProjectConstraints });
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
    toolbar.appendChild(el('span', 'gb-seq-title', 'Spatio-Temporal Sequencer'));
    toolbar.appendChild(el('span', 'gb-seq-clock', msToFrameLabel(playheadMs, docData.fps)));

    // Academic Domain Preset Switcher (Default: Character Rigging & IK Lab)
    const presetSelect = el('select', 'gb-seq-select');
    presetSelect.dataset.role = 'seq-domain-preset';
    for (const preset of Object.values(SEQUENCER_DOMAIN_PRESETS)) {
      const opt = el('option', '', preset.label);
      opt.value = preset.id;
      if (preset.id === (docData.presetId ?? DEFAULT_ACADEMIC_SEQUENCER_PRESET)) {
        opt.selected = true;
      }
      presetSelect.appendChild(opt);
    }
    presetSelect.addEventListener('change', (ev) => {
      const nextPresetId = ev.target.value;
      docData = createPresetSequenceDocument(nextPresetId);
      playheadMs = 0;
      lastConflictReport = null;
      broadcastPlayhead(null, playheadMs);
      render();
    });
    toolbar.appendChild(presetSelect);

    const playBtn = el(
      'button',
      `gb-seq-btn${playing ? ' is-active' : ''}`,
      playing ? 'Pause' : 'Play',
    );
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

    const projBtn = el(
      'button',
      `gb-seq-btn${autoProjectConstraints ? ' is-active' : ''}`,
      'IK & Arc Project (τ̂→τ*)',
    );
    projBtn.dataset.role = 'seq-auto-project';
    projBtn.addEventListener('click', () => {
      autoProjectConstraints = !autoProjectConstraints;
      broadcastPlayhead(null, playheadMs);
      render();
    });
    toolbar.appendChild(projBtn);

    const conflictBtn = el('button', 'gb-seq-btn', '⚡ 2-Agent Rig Conflict');
    conflictBtn.dataset.role = 'seq-sim-conflict';
    conflictBtn.addEventListener('click', () => {
      const transformTrackIdx = docData.tracks.findIndex((t) => t.kind === 'transform');
      if (transformTrackIdx >= 0) {
        const sec = docData.tracks[transformTrackIdx].sections?.[0];
        if (sec) {
          const agentA = {
            id: 'agent_a_desk_relocate',
            startMs: sec.startMs,
            endMs: sec.endMs,
            fromPos: [0, 0.86, 0.44],
            toPos: [0.34, 0.62, 0.68],
          };
          const resolution = resolveTemporalAgentConflict(agentA, sec, docData.obstacles ?? []);
          lastConflictReport = resolution;
          commitOps(
            buildTransformTargetOps(
              transformTrackIdx,
              0,
              resolution.resolvedSection.toPos,
              resolution.resolvedSection.arcLift,
            ),
            'Resolve 2-Agent Rig & Trajectory Conflict',
          );
        }
      }
    });
    toolbar.appendChild(conflictBtn);

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
    rulerRow.appendChild(
      el('span', '', `${docData.name} (${docData.durationMs}ms @ ${docData.fps}fps)`),
    );
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
      const muteBtn = el(
        'button',
        `gb-seq-btn${track.muted ? ' is-active' : ''}`,
        track.muted ? 'M' : 'On',
      );
      muteBtn.dataset.role = `mute-track-${tIdx}`;
      muteBtn.addEventListener('click', () => {
        commitOps(buildTrackMuteOps(tIdx, !track.muted), `Toggle mute ${track.label}`);
      });
      header.appendChild(muteBtn);
      lane.appendChild(header);

      const laneCanvas = el('div', 'gb-seq-lane-canvas');
      (track.sections ?? []).forEach((sec, sIdx) => {
        const isSel =
          selectedItem.trackIndex === tIdx &&
          selectedItem.itemType === 'section' &&
          selectedItem.itemIndex === sIdx;
        const bar = el(
          'button',
          `gb-seq-section${isSel ? ' is-selected' : ''}`,
          sec.clip ?? sec.view ?? sec.id,
        );
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
        const isSel =
          selectedItem.trackIndex === tIdx &&
          selectedItem.itemType === 'trigger' &&
          selectedItem.itemIndex === trIdx;
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

    // Inspector + Neuro-Symbolic IK & Trajectory Diagnostics
    const inspector = el('div', 'gb-seq-inspector');
    const selTrack = docData.tracks[selectedItem.trackIndex] ?? docData.tracks[0];
    if (selTrack && selectedItem.itemType === 'section') {
      const sec = selTrack.sections?.[selectedItem.itemIndex] ?? selTrack.sections?.[0];
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
            buildSectionSpanOps(
              selectedItem.trackIndex,
              selectedItem.itemIndex,
              Number(ev.target.value),
              sec.endMs,
              docData.fps,
              snapToFrame,
            ),
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
            buildSectionSpanOps(
              selectedItem.trackIndex,
              selectedItem.itemIndex,
              sec.startMs,
              Number(ev.target.value),
              docData.fps,
              snapToFrame,
            ),
            `Move ${sec.id} end`,
          );
        });
        endRow.appendChild(endIn);
        inspector.appendChild(endRow);

        if (sec.curve) {
          const curveSelect = el('select', 'gb-seq-select');
          curveSelect.dataset.role = 'inspector-curve-type';
          const curType =
            typeof sec.curve === 'string' ? sec.curve : sec.curve.type ?? 'smoothstep';
          for (const kind of ['linear', 'smoothstep', 'easeOutQuart', 'cubicBezier', 'spring']) {
            const opt = el('option', '', kind);
            opt.value = kind;
            if (kind === curType) opt.selected = true;
            curveSelect.appendChild(opt);
          }
          curveSelect.addEventListener('change', (ev) => {
            const nextKind = ev.target.value;
            let nextCurve = { type: nextKind };
            if (nextKind === 'cubicBezier') {
              nextCurve = { type: 'cubicBezier', x1: 0.22, y1: 1.0, x2: 0.36, y2: 1.0 };
            }
            if (nextKind === 'spring') {
              nextCurve = { type: 'spring', stiffness: 260, damping: 24, mass: 1 };
            }
            commitOps(
              buildTransformCurveOps(
                selectedItem.trackIndex,
                selectedItem.itemIndex,
                nextCurve,
                sec.arcLift,
              ),
              `Change curve ${sec.id}`,
            );
          });
          inspector.appendChild(curveSelect);

          const proj = projectTrajectorySection(sec, docData.obstacles ?? []);

          // Curve + Trajectory Ghost SVG preview
          const svg = dom.createElementNS
            ? dom.createElementNS('http://www.w3.org/2000/svg', 'svg')
            : el('svg');
          svg.setAttribute?.('viewBox', '0 0 120 60');
          svg.setAttribute?.('class', 'gb-seq-curve-svg');

          if (proj.violated) {
            const ghostPoly = dom.createElementNS
              ? dom.createElementNS('http://www.w3.org/2000/svg', 'polyline')
              : el('polyline');
            const ghostPts = proj.ghostArcPoints.map((pt, idx) => {
              const u = idx / Math.max(1, proj.ghostArcPoints.length - 1);
              const px = (4 + u * 112).toFixed(1);
              const py = (54 - clamp((pt[1] - 0.5) / 1.1, 0, 1) * 44).toFixed(1);
              return `${px},${py}`;
            });
            ghostPoly.setAttribute?.('points', ghostPts.join(' '));
            ghostPoly.setAttribute?.('fill', 'none');
            ghostPoly.setAttribute?.('stroke', '#ff6b6b');
            ghostPoly.setAttribute?.('stroke-dasharray', '3 2');
            ghostPoly.setAttribute?.('stroke-width', '1.5');
            svg.appendChild(ghostPoly);
          }

          const poly = dom.createElementNS
            ? dom.createElementNS('http://www.w3.org/2000/svg', 'polyline')
            : el('polyline');
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

          // Semantic Beat Edges & IK Readout
          const semanticBox = el('div', 'gb-seq-semantic-box');
          semanticBox.dataset.role = 'seq-semantic-edges';
          const edgeLabels = proj.liftedEdges
            .map((e) => `${e.from} --[${e.rel}]--> ${e.to}`)
            .join(' | ');
          semanticBox.textContent =
            edgeLabels || 'wrist_hand_R --[reaching_for]--> artifact_orb';
          inspector.appendChild(semanticBox);

          if (proj.ikSolution) {
            const ikBadge = el(
              'div',
              'gb-seq-clock',
              `IK Elbow: ${proj.ikSolution.elbowBendAngleDeg}° · Δx*: ${proj.l2Perturbation}m`,
            );
            ikBadge.dataset.role = 'seq-ik-readout';
            inspector.appendChild(ikBadge);
          }

          if (lastConflictReport) {
            const conflictBadge = el(
              'div',
              'gb-seq-clock',
              `2-Agent Resolved: overlap ${lastConflictReport.temporalOverlapMs}ms · Δ=${lastConflictReport.l2Perturbation}m`,
            );
            conflictBadge.dataset.role = 'seq-conflict-readout';
            inspector.appendChild(conflictBadge);
          }
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
            buildTriggerTimingOps(
              selectedItem.trackIndex,
              selectedItem.itemIndex,
              Number(ev.target.value),
              docData.fps,
              snapToFrame,
            ),
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
