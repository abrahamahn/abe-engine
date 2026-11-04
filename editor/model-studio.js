// Ganbate Studio — In-Studio 3D Mesh, LookDev, Non-Destructive Fixup & Kitbash Workbench.
// Pure geometry, sidecar normalization, LOD decimation, collider fitting, socket math, and
// procedural glTF 2.0 (.glb) builders stay free of DOM globals so Node unit tests exercise them
// directly; `mountModelStudio(container, studioApi)` implements `StudioWorkbenchInstance`.

import { formatBytes, assetBasename, assetStem } from './asset-library.js';
import {
  drawGlbPreviewCanvas,
  hasWebGlPreviewSupport,
  parseGlbPreview,
  parseGlbPreviewAsync,
} from './asset-previews.js';

export const WORKBENCH_ID = 'model-studio';
export const DEFAULT_LOD_RATIOS = Object.freeze([1.0, 0.5, 0.25, 0.125]);
export const PRIMITIVE_KINDS = Object.freeze(['box', 'cylinder', 'plane', 'ramp', 'arch']);
export const SOCKET_PRESETS = Object.freeze(['ground', 'top_surface', 'hand_r', 'muzzle']);

export const LIGHTING_PRESETS = Object.freeze({
  studio: {
    id: 'studio',
    label: 'Studio',
    bgTop: '#1d222d',
    bgBottom: '#101319',
    keyDir: [0.48, 0.74, 0.47],
    keyColor: [1.0, 0.98, 0.95],
    fillDir: [-0.62, 0.35, -0.7],
    fillColor: [0.42, 0.52, 0.72],
    hemiSky: [0.34, 0.38, 0.46],
    hemiGround: [0.14, 0.14, 0.16],
    rimStrength: 0.2,
  },
  day: {
    id: 'day',
    label: 'Day',
    bgTop: '#233858',
    bgBottom: '#18212b',
    keyDir: [0.36, 0.85, 0.38],
    keyColor: [1.06, 0.99, 0.88],
    fillDir: [-0.55, 0.48, -0.68],
    fillColor: [0.48, 0.66, 0.92],
    hemiSky: [0.45, 0.58, 0.78],
    hemiGround: [0.22, 0.2, 0.16],
    rimStrength: 0.14,
  },
  night: {
    id: 'night',
    label: 'Night',
    bgTop: '#0d1324',
    bgBottom: '#080a11',
    keyDir: [-0.42, 0.78, 0.46],
    keyColor: [0.56, 0.72, 1.05],
    fillDir: [0.68, 0.24, -0.69],
    fillColor: [0.32, 0.22, 0.44],
    hemiSky: [0.14, 0.2, 0.36],
    hemiGround: [0.06, 0.07, 0.1],
    rimStrength: 0.28,
  },
});

export const SHADING_MODES = Object.freeze([
  { id: 'shaded', label: 'Shaded' },
  { id: 'wireframe', label: 'Wire + Shaded' },
  { id: 'uv_checker', label: 'UV Checker' },
  { id: 'normals', label: 'Normals' },
  { id: 'tangents', label: 'Tangents' },
]);

function roundNum(v, digits = 4) {
  const n = Number(v);
  if (!Number.isFinite(n)) return 0;
  const f = 10 ** digits;
  const r = Math.round(n * f) / f;
  return Math.abs(r) < 1e-7 ? 0 : r;
}

function clamp(v, lo, hi) {
  return Math.max(lo, Math.min(hi, v));
}

function cleanVec3(arr, fallback = [0, 0, 0]) {
  if (!Array.isArray(arr) || arr.length < 3) return [...fallback];
  return [roundNum(arr[0], 4), roundNum(arr[1], 4), roundNum(arr[2], 4)];
}

function normalizeQuat(arr) {
  if (!Array.isArray(arr) || arr.length < 4) return [0, 0, 0, 1];
  const x = Number(arr[0]) || 0;
  const y = Number(arr[1]) || 0;
  const z = Number(arr[2]) || 0;
  const w = Number.isFinite(Number(arr[3])) ? Number(arr[3]) : 1;
  const len = Math.hypot(x, y, z, w);
  if (len < 1e-8) return [0, 0, 0, 1];
  return [roundNum(x / len, 5), roundNum(y / len, 5), roundNum(z / len, 5), roundNum(w / len, 5)];
}

/** Converts XYZ intrinsic Euler angles in degrees to a unit quaternion `[x, y, z, w]`. */
export function eulerDegToQuat(eulerDeg = [0, 0, 0]) {
  const rx = ((Number(eulerDeg[0]) || 0) * Math.PI) / 360;
  const ry = ((Number(eulerDeg[1]) || 0) * Math.PI) / 360;
  const rz = ((Number(eulerDeg[2]) || 0) * Math.PI) / 360;
  const cx = Math.cos(rx), sx = Math.sin(rx);
  const cy = Math.cos(ry), sy = Math.sin(ry);
  const cz = Math.cos(rz), sz = Math.sin(rz);
  return normalizeQuat([
    sx * cy * cz + cx * sy * sz,
    cx * sy * cz - sx * cy * sz,
    cx * cy * sz + sx * sy * cz,
    cx * cy * cz - sx * sy * sz,
  ]);
}

/** Converts a unit quaternion `[x, y, z, w]` back to XYZ Euler angles in degrees. */
export function quatToEulerDeg(q = [0, 0, 0, 1]) {
  const [x, y, z, w] = normalizeQuat(q);
  const sinrCosp = 2 * (w * x + y * z);
  const cosrCosp = 1 - 2 * (x * x + y * y);
  const rx = Math.atan2(sinrCosp, cosrCosp);

  const sinp = clamp(2 * (w * y - z * x), -1, 1);
  const ry = Math.asin(sinp);

  const sinyCosp = 2 * (w * z + x * y);
  const cosyCosp = 1 - 2 * (y * y + z * z);
  const rz = Math.atan2(sinyCosp, cosyCosp);

  const k = 180 / Math.PI;
  return [roundNum(rx * k, 2), roundNum(ry * k, 2), roundNum(rz * k, 2)];
}

function rotateByQuat(v, q) {
  const [qx, qy, qz, qw] = normalizeQuat(q);
  const [vx, vy, vz] = v;
  const tx = 2 * (qy * vz - qz * vy);
  const ty = 2 * (qz * vx - qx * vz);
  const tz = 2 * (qx * vy - qy * vx);
  return [
    vx + qw * tx + (qy * tz - qz * ty),
    vy + qw * ty + (qz * tx - qx * tz),
    vz + qw * tz + (qx * ty - qy * tx),
  ];
}

/** Injects `model-studio.css` once into `doc.head` when running in a browser DOM. */
export function ensureModelStudioStyles(doc = typeof document !== 'undefined' ? document : null) {
  if (!doc || !doc.head || typeof doc.createElement !== 'function') return;
  if (doc.querySelector?.('link[data-ganbate-model-studio]')) return;
  const link = doc.createElement('link');
  link.rel = 'stylesheet';
  link.setAttribute('data-ganbate-model-studio', '1');
  try {
    link.href = new URL('./model-studio.css', import.meta.url).href;
  } catch {
    link.href = '/editor/model-studio.css';
  }
  doc.head.appendChild(link);
}

/** Normalizes raw sidecar / `meta-get` JSON into canonical Model Studio authoring state. */
export function normalizeModelMeta(raw = {}) {
  const src = raw && typeof raw === 'object' ? raw : {};
  const imp = src.import && typeof src.import === 'object' ? src.import : {};
  const rawPivot = String(imp.pivot || 'origin').trim().toLowerCase().replace(/-/g, '_');
  const pivot = rawPivot === 'bottom_center' || rawPivot === 'bottom' || rawPivot === 'ground'
    ? 'bottom_center'
    : rawPivot === 'centroid' || rawPivot === 'center'
      ? 'centroid'
      : 'origin';
  const scaleRaw = Number(imp.scale ?? 1);
  const scale = Number.isFinite(scaleRaw) && scaleRaw > 0 ? clamp(scaleRaw, 1e-5, 1e5) : 1;
  const thRaw = imp.target_height ?? imp.targetHeight;
  const target_height = thRaw !== null && thRaw !== undefined && thRaw !== '' && Number(thRaw) > 0
    ? clamp(Number(thRaw), 1e-4, 1e5)
    : null;
  const activeLodRaw = Number(imp.active_lod ?? imp.activeLod ?? src.active_lod ?? 0);
  const active_lod = Number.isInteger(activeLodRaw) ? clamp(activeLodRaw, 0, 3) : 0;

  let lods = [...DEFAULT_LOD_RATIOS];
  if (Array.isArray(src.lods) && src.lods.length >= 1) {
    lods = DEFAULT_LOD_RATIOS.map((def, idx) => {
      const r = Number(src.lods[idx] ?? def);
      return Number.isFinite(r) ? roundNum(clamp(r, 0.01, 1.0), 4) : def;
    });
  }

  let collider = null;
  if (src.collider && typeof src.collider === 'object') {
    const sh = String(src.collider.shape || 'box').trim().toLowerCase().replace(/-/g, '_');
    const shape = sh === 'sphere'
      ? 'sphere'
      : sh === 'capsule'
        ? 'capsule'
        : sh === 'convex_hull' || sh === 'hull'
          ? 'convex_hull'
          : 'box';
    collider = {
      shape,
      center: cleanVec3(src.collider.center, [0, 0, 0]),
      half_extents: cleanVec3(src.collider.half_extents ?? src.collider.halfExtents, [0.5, 0.5, 0.5]).map((v) => Math.max(0.001, v)),
      radius: roundNum(Math.max(0.001, Number(src.collider.radius ?? 0.5) || 0.5), 4),
      height: roundNum(Math.max(0.002, Number(src.collider.height ?? 1.0) || 1.0), 4),
      hull_points: Array.isArray(src.collider.hull_points)
        ? src.collider.hull_points.slice(0, 64).map((pt) => cleanVec3(pt))
        : [],
    };
  }

  const sockets = {};
  if (src.sockets && typeof src.sockets === 'object' && !Array.isArray(src.sockets)) {
    for (const [name, def] of Object.entries(src.sockets)) {
      const cleanName = sanitizeSocketName(name);
      if (!cleanName || !def || typeof def !== 'object') continue;
      sockets[cleanName] = {
        translation: cleanVec3(def.translation ?? def.position, [0, 0, 0]),
        rotation: normalizeQuat(def.rotation),
      };
    }
  } else if (Array.isArray(src.sockets)) {
    for (const item of src.sockets) {
      const cleanName = sanitizeSocketName(item?.name || '');
      if (!cleanName) continue;
      sockets[cleanName] = {
        translation: cleanVec3(item.translation ?? item.position, [0, 0, 0]),
        rotation: normalizeQuat(item.rotation),
      };
    }
  }

  return {
    import: {
      pivot,
      offset: cleanVec3(imp.offset, [0, 0, 0]),
      scale: roundNum(scale, 5),
      target_height: target_height !== null ? roundNum(target_height, 4) : null,
      active_lod,
    },
    lods,
    collider,
    sockets,
  };
}

export function sanitizeSocketName(raw) {
  return String(raw || '')
    .trim()
    .replace(/[^A-Za-z0-9_.-]+/g, '_')
    .replace(/^_+|_+$/g, '')
    .slice(0, 64);
}

/** Serializes canonical Model Studio metadata to match `ganbate_pipeline::ddc::AssetMeta`. */
export function serializeModelMeta(meta) {
  const norm = normalizeModelMeta(meta);
  const imp = {
    pivot: norm.import.pivot,
    offset: norm.import.offset,
    scale: norm.import.scale,
    active_lod: norm.import.active_lod,
  };
  if (norm.import.target_height !== null) {
    imp.target_height = norm.import.target_height;
  }
  const out = {
    import: imp,
    lods: norm.lods,
  };
  if (norm.collider) {
    const c = norm.collider;
    if (c.shape === 'box') {
      out.collider = { shape: 'box', center: c.center, half_extents: c.half_extents };
    } else if (c.shape === 'sphere') {
      out.collider = { shape: 'sphere', center: c.center, radius: c.radius };
    } else if (c.shape === 'capsule') {
      out.collider = { shape: 'capsule', center: c.center, radius: c.radius, height: c.height };
    } else {
      out.collider = {
        shape: 'convex_hull',
        center: c.center,
        half_extents: c.half_extents,
        hull_points: c.hull_points || [],
      };
    }
  }
  const sockEntries = Object.entries(norm.sockets);
  if (sockEntries.length) {
    out.sockets = {};
    for (const [name, s] of sockEntries) {
      out.sockets[name] = {
        translation: cleanVec3(s.translation),
        rotation: normalizeQuat(s.rotation),
      };
    }
  }
  return out;
}

/** Computes the exact non-destructive scale + pivot shift applied by `ddc::prepare_node_world`. */
export function computeImportFixup(bounds = [[0, 0, 0], [0, 0, 0]], importSettings = {}) {
  const lo = Array.isArray(bounds?.[0]) ? bounds[0] : [0, 0, 0];
  const hi = Array.isArray(bounds?.[1]) ? bounds[1] : [0, 0, 0];
  const rawSize = [
    Math.max(0, (Number(hi[0]) || 0) - (Number(lo[0]) || 0)),
    Math.max(0, (Number(hi[1]) || 0) - (Number(lo[1]) || 0)),
    Math.max(0, (Number(hi[2]) || 0) - (Number(lo[2]) || 0)),
  ];
  const norm = normalizeModelMeta({ import: importSettings }).import;
  const effScale = norm.target_height !== null && norm.target_height > 0 && rawSize[1] > 1e-6
    ? clamp(norm.target_height / rawSize[1], 1e-5, 1e5)
    : clamp(norm.scale, 1e-5, 1e5);

  const cx = (lo[0] + hi[0]) * 0.5 * effScale;
  const cy = (lo[1] + hi[1]) * 0.5 * effScale;
  const cz = (lo[2] + hi[2]) * 0.5 * effScale;

  let pivotShift = [0, 0, 0];
  if (norm.pivot === 'bottom_center') {
    pivotShift = [-cx, -lo[1] * effScale, -cz];
  } else if (norm.pivot === 'centroid') {
    pivotShift = [-cx, -cy, -cz];
  }

  const translation = [
    roundNum(pivotShift[0] + norm.offset[0], 5),
    roundNum(pivotShift[1] + norm.offset[1], 5),
    roundNum(pivotShift[2] + norm.offset[2], 5),
  ];

  const transformedBounds = [
    [
      roundNum(lo[0] * effScale + translation[0], 5),
      roundNum(lo[1] * effScale + translation[1], 5),
      roundNum(lo[2] * effScale + translation[2], 5),
    ],
    [
      roundNum(hi[0] * effScale + translation[0], 5),
      roundNum(hi[1] * effScale + translation[1], 5),
      roundNum(hi[2] * effScale + translation[2], 5),
    ],
  ];
  const transformedSize = [
    roundNum(rawSize[0] * effScale, 5),
    roundNum(rawSize[1] * effScale, 5),
    roundNum(rawSize[2] * effScale, 5),
  ];

  return {
    effectiveScale: roundNum(effScale, 6),
    pivotShift: pivotShift.map((v) => roundNum(v, 5)),
    translation,
    transformedBounds,
    transformedSize,
    applyPoint(p) {
      return [
        p[0] * effScale + translation[0],
        p[1] * effScale + translation[1],
        p[2] * effScale + translation[2],
      ];
    },
  };
}

/** Spatial-grid LOD mesh simplifier matching `ganbate_assets::mesh_opt::decimate_mesh`. */
export function decimatePreviewMesh(fullMesh, targetRatio = 1.0) {
  if (!fullMesh || !fullMesh.positions || !fullMesh.indices) {
    return {
      positions: new Float32Array(0),
      normals: new Float32Array(0),
      tangents: new Float32Array(0),
      uvs: new Float32Array(0),
      colors: new Float32Array(0),
      indices: new Uint32Array(0),
      vertices: 0,
      triangles: 0,
      achievedRatio: 1,
    };
  }
  const origVerts = Math.floor(fullMesh.positions.length / 3);
  const origTris = Math.floor(fullMesh.indices.length / 3);
  const ratio = clamp(Number(targetRatio) || 1, 0.01, 1.0);
  if (ratio >= 0.999 || origTris <= 4) {
    return {
      ...fullMesh,
      vertices: origVerts,
      triangles: origTris,
      achievedRatio: 1,
    };
  }

  let minX = Infinity, minY = Infinity, minZ = Infinity;
  let maxX = -Infinity, maxY = -Infinity, maxZ = -Infinity;
  const pos = fullMesh.positions;
  for (let i = 0; i < origVerts; i++) {
    const x = pos[i * 3], y = pos[i * 3 + 1], z = pos[i * 3 + 2];
    if (x < minX) minX = x;
    if (y < minY) minY = y;
    if (z < minZ) minZ = z;
    if (x > maxX) maxX = x;
    if (y > maxY) maxY = y;
    if (z > maxZ) maxZ = z;
  }
  const spanX = Math.max(1e-5, maxX - minX);
  const spanY = Math.max(1e-5, maxY - minY);
  const spanZ = Math.max(1e-5, maxZ - minZ);
  const maxSpan = Math.max(spanX, spanY, spanZ);
  const targetTris = Math.max(1, Math.round(origTris * ratio));

  const clusterAt = (res) => {
    const cell = maxSpan / res;
    const cellMap = new Map();
    const remap = new Int32Array(origVerts);
    const clusters = [];
    for (let v = 0; v < origVerts; v++) {
      const x = pos[v * 3], y = pos[v * 3 + 1], z = pos[v * 3 + 2];
      const gx = Math.floor((x - minX) / cell);
      const gy = Math.floor((y - minY) / cell);
      const gz = Math.floor((z - minZ) / cell);
      const key = `${gx}|${gy}|${gz}`;
      let ci = cellMap.get(key);
      if (ci === undefined) {
        ci = clusters.length;
        cellMap.set(key, ci);
        clusters.push({
          count: 0,
          px: 0, py: 0, pz: 0,
          nx: 0, ny: 0, nz: 0,
          tx: 0, ty: 0, tz: 0, tw: fullMesh.tangents?.[v * 4 + 3] || 1,
          u: 0, uvV: 0,
          cr: 0, cg: 0, cb: 0,
        });
      }
      remap[v] = ci;
      const c = clusters[ci];
      c.count += 1;
      c.px += x; c.py += y; c.pz += z;
      if (fullMesh.normals?.length >= (v + 1) * 3) {
        c.nx += fullMesh.normals[v * 3];
        c.ny += fullMesh.normals[v * 3 + 1];
        c.nz += fullMesh.normals[v * 3 + 2];
      }
      if (fullMesh.tangents?.length >= (v + 1) * 4) {
        c.tx += fullMesh.tangents[v * 4];
        c.ty += fullMesh.tangents[v * 4 + 1];
        c.tz += fullMesh.tangents[v * 4 + 2];
      }
      if (fullMesh.uvs?.length >= (v + 1) * 2) {
        c.u += fullMesh.uvs[v * 2];
        c.uvV += fullMesh.uvs[v * 2 + 1];
      }
      if (fullMesh.colors?.length >= (v + 1) * 3) {
        c.cr += fullMesh.colors[v * 3];
        c.cg += fullMesh.colors[v * 3 + 1];
        c.cb += fullMesh.colors[v * 3 + 2];
      }
    }

    const outIdx = [];
    const srcIdx = fullMesh.indices;
    for (let t = 0; t < origTris; t++) {
      const a = remap[srcIdx[t * 3]];
      const b = remap[srcIdx[t * 3 + 1]];
      const c = remap[srcIdx[t * 3 + 2]];
      if (a === b || b === c || a === c) continue;
      outIdx.push(a, b, c);
    }
    return { clusters, outIdx, triCount: outIdx.length / 3 };
  };

  let lo = 2;
  let hi = 96;
  let best = null;
  for (let iter = 0; iter < 9 && lo <= hi; iter++) {
    const mid = (lo + hi) >> 1;
    const cand = clusterAt(mid);
    if (cand.triCount > 0) {
      if (!best || Math.abs(cand.triCount - targetTris) < Math.abs(best.triCount - targetTris)) {
        best = cand;
      }
    }
    if (cand.triCount < targetTris) lo = mid + 1;
    else if (cand.triCount > targetTris) hi = mid - 1;
    else break;
  }
  if (!best || best.triCount === 0) {
    return { ...fullMesh, vertices: origVerts, triangles: origTris, achievedRatio: 1 };
  }

  const vCount = best.clusters.length;
  const outPos = new Float32Array(vCount * 3);
  const outNorm = new Float32Array(vCount * 3);
  const outTan = new Float32Array(vCount * 4);
  const outUv = new Float32Array(vCount * 2);
  const outCol = new Float32Array(vCount * 3);
  for (let i = 0; i < vCount; i++) {
    const c = best.clusters[i];
    const inv = 1 / Math.max(1, c.count);
    outPos[i * 3] = c.px * inv;
    outPos[i * 3 + 1] = c.py * inv;
    outPos[i * 3 + 2] = c.pz * inv;
    const nl = Math.hypot(c.nx, c.ny, c.nz) || 1;
    outNorm[i * 3] = c.nx / nl;
    outNorm[i * 3 + 1] = c.ny / nl;
    outNorm[i * 3 + 2] = c.nz / nl;
    const tl = Math.hypot(c.tx, c.ty, c.tz) || 1;
    outTan[i * 4] = c.tx / tl;
    outTan[i * 4 + 1] = c.ty / tl;
    outTan[i * 4 + 2] = c.tz / tl;
    outTan[i * 4 + 3] = c.tw;
    outUv[i * 2] = c.u * inv;
    outUv[i * 2 + 1] = c.uvV * inv;
    outCol[i * 3] = c.cr * inv;
    outCol[i * 3 + 1] = c.cg * inv;
    outCol[i * 3 + 2] = c.cb * inv;
  }

  return {
    positions: outPos,
    normals: outNorm,
    tangents: outTan,
    uvs: outUv,
    colors: outCol,
    indices: new Uint32Array(best.outIdx),
    vertices: vCount,
    triangles: best.triCount,
    achievedRatio: roundNum(best.triCount / origTris, 4),
  };
}

/** Fits a `box`, `sphere`, `capsule`, or `convex_hull` physics collider to post-fixup vertices. */
export function fitPhysicsCollider(fullMesh, shape = 'box', fixup = null) {
  const pos = fullMesh?.positions;
  const vertCount = pos ? Math.floor(pos.length / 3) : 0;
  const apply = fixup?.applyPoint || ((p) => p);
  if (vertCount === 0) {
    return normalizeModelMeta({ collider: { shape } }).collider;
  }

  let minX = Infinity, minY = Infinity, minZ = Infinity;
  let maxX = -Infinity, maxY = -Infinity, maxZ = -Infinity;
  const pts = new Array(vertCount);
  for (let i = 0; i < vertCount; i++) {
    const p = apply([pos[i * 3], pos[i * 3 + 1], pos[i * 3 + 2]]);
    pts[i] = p;
    if (p[0] < minX) minX = p[0];
    if (p[1] < minY) minY = p[1];
    if (p[2] < minZ) minZ = p[2];
    if (p[0] > maxX) maxX = p[0];
    if (p[1] > maxY) maxY = p[1];
    if (p[2] > maxZ) maxZ = p[2];
  }

  const center = [
    roundNum((minX + maxX) * 0.5, 4),
    roundNum((minY + maxY) * 0.5, 4),
    roundNum((minZ + maxZ) * 0.5, 4),
  ];
  const half_extents = [
    roundNum(Math.max(0.01, (maxX - minX) * 0.5), 4),
    roundNum(Math.max(0.01, (maxY - minY) * 0.5), 4),
    roundNum(Math.max(0.01, (maxZ - minZ) * 0.5), 4),
  ];

  if (shape === 'sphere') {
    let maxR = 0.01;
    for (const p of pts) {
      const d = Math.hypot(p[0] - center[0], p[1] - center[1], p[2] - center[2]);
      if (d > maxR) maxR = d;
    }
    return {
      shape: 'sphere',
      center,
      half_extents,
      radius: roundNum(maxR, 4),
      height: roundNum(maxR * 2, 4),
      hull_points: [],
    };
  }

  if (shape === 'capsule') {
    const radius = roundNum(Math.max(0.01, Math.max(half_extents[0], half_extents[2])), 4);
    const height = roundNum(Math.max(radius * 2, maxY - minY), 4);
    return {
      shape: 'capsule',
      center,
      half_extents,
      radius,
      height,
      hull_points: [],
    };
  }

  if (shape === 'convex_hull' || shape === 'hull') {
    const hull_points = [];
    const seen = new Set();
    for (let dx = -1; dx <= 1; dx++) {
      for (let dy = -1; dy <= 1; dy++) {
        for (let dz = -1; dz <= 1; dz++) {
          if (dx === 0 && dy === 0 && dz === 0) continue;
          let bestPt = pts[0];
          let bestDot = -Infinity;
          for (const p of pts) {
            const dot = p[0] * dx + p[1] * dy + p[2] * dz;
            if (dot > bestDot) {
              bestDot = dot;
              bestPt = p;
            }
          }
          const clean = cleanVec3(bestPt);
          const key = clean.join(',');
          if (!seen.has(key)) {
            seen.add(key);
            hull_points.push(clean);
          }
        }
      }
    }
    return {
      shape: 'convex_hull',
      center,
      half_extents,
      radius: roundNum(Math.hypot(...half_extents), 4),
      height: roundNum(maxY - minY, 4),
      hull_points,
    };
  }

  return {
    shape: 'box',
    center,
    half_extents,
    radius: roundNum(Math.hypot(...half_extents), 4),
    height: roundNum(maxY - minY, 4),
    hull_points: [],
  };
}

/** Builds schema-valid `ganbate-scene` edit ops for a Prefab containing the root mesh, optional
 * `collider` component, and child anchor entities for every named socket in `meta.sockets`. */
export function buildModelPrefabOps(modelAsset, meta, entityIds = []) {
  const norm = normalizeModelMeta(meta);
  const sockEntries = Object.entries(norm.sockets);
  const needed = 1 + sockEntries.length;
  const ids = [...entityIds];
  for (let i = ids.length; i < needed; i++) {
    ids.push((i + 1).toString(16).padStart(16, '0'));
  }

  const stem = assetStem(modelAsset?.path || 'model') || 'Model';
  const rootId = ids[0];
  const rootComponents = {
    transform: {},
    mesh: { model: modelAsset.id },
  };

  if (norm.collider) {
    const c = norm.collider;
    if (c.shape === 'box') {
      rootComponents.collider = {
        shape: 'box',
        center: c.center,
        half_extents: c.half_extents,
      };
    } else if (c.shape === 'sphere') {
      rootComponents.collider = {
        shape: 'sphere',
        center: c.center,
        radius: c.radius,
      };
    } else if (c.shape === 'capsule') {
      rootComponents.collider = {
        shape: 'capsule',
        center: c.center,
        radius: c.radius,
        half_height: roundNum(Math.max(0, (c.height - 2 * c.radius) * 0.5), 4),
      };
    } else if (c.shape === 'convex_hull') {
      rootComponents.collider = {
        shape: 'hull',
        center: c.center,
      };
    }
  }

  const ops = [
    { set: ['name'], value: stem },
    {
      set: ['entities', rootId],
      value: {
        name: stem,
        components: rootComponents,
      },
    },
  ];

  sockEntries.forEach(([sockName, sock], idx) => {
    const childId = ids[idx + 1];
    const transform = {};
    if (sock.translation.some((v) => Math.abs(v) >= 1e-5)) {
      transform.translation = sock.translation;
    }
    const q = normalizeQuat(sock.rotation);
    const isIdQuat = Math.abs(q[0]) < 1e-5 && Math.abs(q[1]) < 1e-5 && Math.abs(q[2]) < 1e-5 && Math.abs(q[3] - 1) < 1e-5;
    if (!isIdQuat) {
      transform.rotation = q;
    }
    ops.push({
      set: ['entities', childId],
      value: {
        name: sockName,
        parent: rootId,
        components: { transform },
      },
    });
  });

  return ops;
}

// ---- Procedural Graybox / CSG Primitive Builder (.glb) ----------------------------------------

function createMeshBuilder() {
  const positions = [];
  const normals = [];
  const uvs = [];
  const indices = [];

  function pushVertex(p, n, uv) {
    const idx = positions.length / 3;
    positions.push(p[0], p[1], p[2]);
    normals.push(n[0], n[1], n[2]);
    uvs.push(uv[0], uv[1]);
    return idx;
  }

  function addQuad(p0, p1, p2, p3, n, uvScale = 1) {
    let nx = n?.[0] ?? 0, ny = n?.[1] ?? 0, nz = n?.[2] ?? 0;
    if (!n) {
      const e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
      const e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
      nx = e1[1] * e2[2] - e1[2] * e2[1];
      ny = e1[2] * e2[0] - e1[0] * e2[2];
      nz = e1[0] * e2[1] - e1[1] * e2[0];
      const l = Math.hypot(nx, ny, nz) || 1;
      nx /= l; ny /= l; nz /= l;
    }
    const uLen = Math.hypot(p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]) * uvScale;
    const vLen = Math.hypot(p3[0] - p0[0], p3[1] - p0[1], p3[2] - p0[2]) * uvScale;
    const i0 = pushVertex(p0, [nx, ny, nz], [0, 0]);
    const i1 = pushVertex(p1, [nx, ny, nz], [uLen, 0]);
    const i2 = pushVertex(p2, [nx, ny, nz], [uLen, vLen]);
    const i3 = pushVertex(p3, [nx, ny, nz], [0, vLen]);
    indices.push(i0, i1, i2, i0, i2, i3);
  }

  function addTri(p0, p1, p2, n) {
    let nx = n?.[0] ?? 0, ny = n?.[1] ?? 0, nz = n?.[2] ?? 0;
    if (!n) {
      const e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
      const e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
      nx = e1[1] * e2[2] - e1[2] * e2[1];
      ny = e1[2] * e2[0] - e1[0] * e2[2];
      nz = e1[0] * e2[1] - e1[1] * e2[0];
      const l = Math.hypot(nx, ny, nz) || 1;
      nx /= l; ny /= l; nz /= l;
    }
    const projectUv = (pt) => {
      if (Math.abs(nx) > 0.7) return [pt[2], pt[1]];
      if (Math.abs(ny) > 0.7) return [pt[0], pt[2]];
      return [pt[0], pt[1]];
    };
    const i0 = pushVertex(p0, [nx, ny, nz], projectUv(p0));
    const i1 = pushVertex(p1, [nx, ny, nz], projectUv(p1));
    const i2 = pushVertex(p2, [nx, ny, nz], projectUv(p2));
    indices.push(i0, i1, i2);
  }

  function addBoxRegion(x0, y0, z0, x1, y1, z1) {
    addQuad([x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1], [0, 0, 1]);
    addQuad([x1, y0, z0], [x0, y0, z0], [x0, y1, z0], [x1, y1, z0], [0, 0, -1]);
    addQuad([x1, y0, z1], [x1, y0, z0], [x1, y1, z0], [x1, y1, z1], [1, 0, 0]);
    addQuad([x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0], [-1, 0, 0]);
    addQuad([x0, y1, z1], [x1, y1, z1], [x1, y1, z0], [x0, y1, z0], [0, 1, 0]);
    addQuad([x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1], [0, -1, 0]);
  }

  return { positions, normals, uvs, indices, pushVertex, addQuad, addTri, addBoxRegion };
}

/** Builds procedural graybox blockout geometry (`box`, `cylinder`, `plane`, `ramp`, `arch`)
 * sitting cleanly on `Y = 0` with meter-scaled UVs and unit normals. */
export function buildPrimitiveMesh(kind = 'box', params = {}) {
  const b = createMeshBuilder();
  const w = Math.max(0.05, Number(params.width ?? 2) || 2);
  const h = Math.max(0.05, Number(params.height ?? 2) || 2);
  const d = Math.max(0.05, Number(params.depth ?? 2) || 2);
  const radius = Math.max(0.05, Number(params.radius ?? w * 0.5) || 0.75);
  const segs = clamp(Math.round(Number(params.segments ?? 20) || 20), 6, 96);
  const color = Array.isArray(params.color) && params.color.length >= 3
    ? params.color.slice(0, 3).map((c) => clamp(Number(c) || 0.7, 0.02, 1))
    : [0.68, 0.74, 0.84];

  if (kind === 'plane') {
    const subs = clamp(Math.round(Number(params.subdivisions ?? 2) || 2), 1, 32);
    const x0 = -w * 0.5, z0 = -d * 0.5;
    for (let iz = 0; iz < subs; iz++) {
      for (let ix = 0; ix < subs; ix++) {
        const ax0 = x0 + (ix / subs) * w;
        const ax1 = x0 + ((ix + 1) / subs) * w;
        const az0 = z0 + (iz / subs) * d;
        const az1 = z0 + ((iz + 1) / subs) * d;
        b.addQuad([ax0, 0, az1], [ax1, 0, az1], [ax1, 0, az0], [ax0, 0, az0], [0, 1, 0]);
      }
    }
  } else if (kind === 'cylinder') {
    for (let i = 0; i < segs; i++) {
      const a0 = (i / segs) * Math.PI * 2;
      const a1 = ((i + 1) / segs) * Math.PI * 2;
      const c0 = Math.cos(a0), s0 = Math.sin(a0);
      const c1 = Math.cos(a1), s1 = Math.sin(a1);
      const x0 = c0 * radius, z0 = s0 * radius;
      const x1 = c1 * radius, z1 = s1 * radius;

      const u0 = (i / segs) * (2 * Math.PI * radius);
      const u1 = ((i + 1) / segs) * (2 * Math.PI * radius);
      const v00 = b.pushVertex([x0, 0, z0], [c0, 0, s0], [u0, 0]);
      const v10 = b.pushVertex([x1, 0, z1], [c1, 0, s1], [u1, 0]);
      const v11 = b.pushVertex([x1, h, z1], [c1, 0, s1], [u1, h]);
      const v01 = b.pushVertex([x0, h, z0], [c0, 0, s0], [u0, h]);
      b.indices.push(v00, v01, v11, v00, v11, v10);

      // Top cap (+Y) and bottom cap (-Y)
      b.addTri([0, h, 0], [x0, h, z0], [x1, h, z1], [0, 1, 0]);
      b.addTri([0, 0, 0], [x1, 0, z1], [x0, 0, z0], [0, -1, 0]);
    }
  } else if (kind === 'ramp') {
    const hx = w * 0.5, hz = d * 0.5;
    // Rises from y=0 at +hz to y=h at -hz
    const fl = [-hx, 0, hz], fr = [hx, 0, hz];
    const bl = [-hx, 0, -hz], br = [hx, 0, -hz];
    const tl = [-hx, h, -hz], tr = [hx, h, -hz];
    b.addQuad(fl, fr, tr, tl, null); // Sloped deck
    b.addQuad(br, bl, tl, tr, [0, 0, -1]); // Back vertical face
    b.addQuad(bl, br, fr, fl, [0, -1, 0]); // Bottom face
    b.addTri(bl, fl, tl, [-1, 0, 0]); // Left sidewall
    b.addTri(fr, br, tr, [1, 0, 0]); // Right sidewall
  } else if (kind === 'arch') {
    const thick = clamp(Number(params.thickness ?? w * 0.18) || 0.45, 0.05, w * 0.42);
    const outerR = w * 0.5;
    const innerR = Math.max(0.05, outerR - thick);
    const springY = Math.max(0.1, h - outerR);
    const hz = d * 0.5;

    // Two piers from y=0 to y=springY
    b.addBoxRegion(-outerR, 0, -hz, -innerR, springY, hz);
    b.addBoxRegion(innerR, 0, -hz, outerR, springY, hz);

    // Semicircular arch voussoirs from angle 0..PI (even segment count hits exact keystone y = h)
    const archSegs = Math.max(8, Math.round(segs * 0.4) * 2);
    for (let i = 0; i < archSegs; i++) {
      const a0 = (i / archSegs) * Math.PI;
      const a1 = ((i + 1) / archSegs) * Math.PI;
      const ox0 = Math.cos(a0) * outerR, oy0 = springY + Math.sin(a0) * outerR;
      const ox1 = Math.cos(a1) * outerR, oy1 = springY + Math.sin(a1) * outerR;
      const ix0 = Math.cos(a0) * innerR, iy0 = springY + Math.sin(a0) * innerR;
      const ix1 = Math.cos(a1) * innerR, iy1 = springY + Math.sin(a1) * innerR;

      // Front (+Z) and Back (-Z)
      b.addQuad([ix0, iy0, hz], [ox0, oy0, hz], [ox1, oy1, hz], [ix1, iy1, hz], [0, 0, 1]);
      b.addQuad([ox0, oy0, -hz], [ix0, iy0, -hz], [ix1, iy1, -hz], [ox1, oy1, -hz], [0, 0, -1]);
      // Extrados (outer curve) & Intrados (inner ceiling curve)
      b.addQuad([ox0, oy0, hz], [ox0, oy0, -hz], [ox1, oy1, -hz], [ox1, oy1, hz], null);
      b.addQuad([ix1, iy1, hz], [ix1, iy1, -hz], [ix0, iy0, -hz], [ix0, iy0, hz], null);
    }
  } else {
    b.addBoxRegion(-w * 0.5, 0, -d * 0.5, w * 0.5, h, d * 0.5);
  }

  return {
    kind,
    color,
    positions: new Float32Array(b.positions),
    normals: new Float32Array(b.normals),
    uvs: new Float32Array(b.uvs),
    indices: new Uint32Array(b.indices),
  };
}

/** Encodes a procedural graybox primitive into a strict binary glTF 2.0 `.glb` `Uint8Array`
 * ready for `studioApi.importAsset(path, bytes)` and `ganbate_assets::glb::import_nodes`. */
export function buildPrimitiveGlb(kind = 'box', params = {}) {
  const mesh = buildPrimitiveMesh(kind, params);
  const vCount = mesh.positions.length / 3;
  const iCount = mesh.indices.length;
  const useU32 = vCount > 65535;
  const idxArray = useU32 ? new Uint32Array(mesh.indices) : new Uint16Array(mesh.indices);

  const minPos = [Infinity, Infinity, Infinity];
  const maxPos = [-Infinity, -Infinity, -Infinity];
  for (let i = 0; i < vCount; i++) {
    for (let k = 0; k < 3; k++) {
      const val = mesh.positions[i * 3 + k];
      if (val < minPos[k]) minPos[k] = val;
      if (val > maxPos[k]) maxPos[k] = val;
    }
  }

  const posBytes = new Uint8Array(mesh.positions.buffer, mesh.positions.byteOffset, mesh.positions.byteLength);
  const normBytes = new Uint8Array(mesh.normals.buffer, mesh.normals.byteOffset, mesh.normals.byteLength);
  const uvBytes = new Uint8Array(mesh.uvs.buffer, mesh.uvs.byteOffset, mesh.uvs.byteLength);
  const idxBytes = new Uint8Array(idxArray.buffer, idxArray.byteOffset, idxArray.byteLength);

  const pad4 = (n) => (4 - (n % 4)) % 4;
  const posOff = 0;
  const normOff = posOff + posBytes.byteLength;
  const uvOff = normOff + normBytes.byteLength;
  const idxOff = uvOff + uvBytes.byteLength;
  const rawBinLen = idxOff + idxBytes.byteLength;
  const binLen = rawBinLen + pad4(rawBinLen);

  const binChunk = new Uint8Array(binLen);
  binChunk.set(posBytes, posOff);
  binChunk.set(normBytes, normOff);
  binChunk.set(uvBytes, uvOff);
  binChunk.set(idxBytes, idxOff);

  const nodeName = String(params.name || `Graybox_${kind}`);
  const gltf = {
    asset: { version: '2.0', generator: 'Ganbate Model Studio' },
    scene: 0,
    scenes: [{ nodes: [0] }],
    nodes: [{ name: nodeName, mesh: 0 }],
    materials: [
      {
        name: 'GrayboxGrid',
        pbrMetallicRoughness: {
          baseColorFactor: [mesh.color[0], mesh.color[1], mesh.color[2], 1],
          metallicFactor: 0.05,
          roughnessFactor: 0.78,
        },
      },
    ],
    meshes: [
      {
        name: nodeName,
        primitives: [
          {
            attributes: { POSITION: 0, NORMAL: 1, TEXCOORD_0: 2 },
            indices: 3,
            material: 0,
          },
        ],
      },
    ],
    bufferViews: [
      { buffer: 0, byteOffset: posOff, byteLength: posBytes.byteLength, target: 34962 },
      { buffer: 0, byteOffset: normOff, byteLength: normBytes.byteLength, target: 34962 },
      { buffer: 0, byteOffset: uvOff, byteLength: uvBytes.byteLength, target: 34962 },
      { buffer: 0, byteOffset: idxOff, byteLength: idxBytes.byteLength, target: 34963 },
    ],
    accessors: [
      {
        bufferView: 0,
        componentType: 5126,
        count: vCount,
        type: 'VEC3',
        min: minPos.map((v) => Number(v.toFixed(6))),
        max: maxPos.map((v) => Number(v.toFixed(6))),
      },
      { bufferView: 1, componentType: 5126, count: vCount, type: 'VEC3' },
      { bufferView: 2, componentType: 5126, count: vCount, type: 'VEC2' },
      { bufferView: 3, componentType: useU32 ? 5125 : 5123, count: iCount, type: 'SCALAR' },
    ],
    buffers: [{ byteLength: binLen }],
  };

  const encoder = new TextEncoder();
  const jsonRaw = encoder.encode(JSON.stringify(gltf));
  const jsonPad = pad4(jsonRaw.byteLength);
  const jsonLen = jsonRaw.byteLength + jsonPad;

  const totalLen = 12 + 8 + jsonLen + 8 + binLen;
  const out = new Uint8Array(totalLen);
  const dv = new DataView(out.buffer);
  dv.setUint32(0, 0x46546c67, true); // 'glTF'
  dv.setUint32(4, 2, true);
  dv.setUint32(8, totalLen, true);

  dv.setUint32(12, jsonLen, true);
  dv.setUint32(16, 0x4e4f534a, true); // 'JSON'
  out.set(jsonRaw, 20);
  out.fill(0x20, 20 + jsonRaw.byteLength, 20 + jsonLen);

  const binHdrOff = 20 + jsonLen;
  dv.setUint32(binHdrOff, binLen, true);
  dv.setUint32(binHdrOff + 4, 0x004e4942, true); // 'BIN\0'
  out.set(binChunk, binHdrOff + 8);
  return out;
}

// ---- Interactive 3D LookDev Canvas Renderer ---------------------------------------------------

function normalize3(v) {
  const l = Math.hypot(v[0], v[1], v[2]) || 1;
  return [v[0] / l, v[1] / l, v[2] / l];
}

function base64ToBytes(b64) {
  if (typeof atob === 'function') {
    const bin = atob(b64);
    const out = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
    return out;
  }
  return new Uint8Array(Buffer.from(b64, 'base64'));
}

export function drawLookDevViewport(canvas, state) {
  if (!canvas || typeof canvas.getContext !== 'function') return;
  const dpr = typeof window !== 'undefined' ? Math.min(2, window.devicePixelRatio || 1) : 1;
  const cssW = Math.max(240, canvas.clientWidth || 640);
  const cssH = Math.max(180, canvas.clientHeight || 420);
  const pxW = Math.round(cssW * dpr);
  const pxH = Math.round(cssH * dpr);
  if (canvas.width !== pxW || canvas.height !== pxH) {
    canvas.width = pxW;
    canvas.height = pxH;
  }
  const ctx = canvas.getContext('2d');
  if (!ctx) return;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);

  const preset = LIGHTING_PRESETS[state.lighting] || LIGHTING_PRESETS.studio;
  const grad = ctx.createLinearGradient(0, 0, 0, cssH);
  grad.addColorStop(0, preset.bgTop);
  grad.addColorStop(1, preset.bgBottom);
  ctx.fillStyle = grad;
  ctx.fillRect(0, 0, cssW, cssH);

  const cam = state.camera;
  const cyaw = Math.cos(cam.yaw), syaw = Math.sin(cam.yaw);
  const cp = Math.cos(cam.pitch), sp = Math.sin(cam.pitch);
  const right = [cyaw, 0, -syaw];
  const up = [-syaw * sp, cp, -cyaw * sp];
  const back = [syaw * cp, sp, cyaw * cp]; // from target toward eye
  const dist = Math.max(0.25, cam.distance);
  const eye = [
    cam.target[0] + back[0] * dist,
    cam.target[1] + back[1] * dist,
    cam.target[2] + back[2] * dist,
  ];
  const fovRad = ((cam.fov || 46) * Math.PI) / 180;
  const focal = (cssH * 0.5) / Math.tan(fovRad * 0.5);

  const projectPoint = (wx, wy, wz) => {
    const dx = wx - eye[0], dy = wy - eye[1], dz = wz - eye[2];
    const vx = dx * right[0] + dy * right[1] + dz * right[2];
    const vy = dx * up[0] + dy * up[1] + dz * up[2];
    const vz = -(dx * back[0] + dy * back[1] + dz * back[2]);
    if (vz <= 0.04) return null;
    const scale = focal / vz;
    return [cssW * 0.5 + vx * scale, cssH * 0.5 - vy * scale, vz];
  };

  const drawSegment3D = (a, b, stroke, width = 1, dash = null) => {
    const pa = projectPoint(a[0], a[1], a[2]);
    const pb = projectPoint(b[0], b[1], b[2]);
    if (!pa || !pb) return;
    ctx.save();
    ctx.strokeStyle = stroke;
    ctx.lineWidth = width;
    if (dash) ctx.setLineDash(dash);
    ctx.beginPath();
    ctx.moveTo(pa[0], pa[1]);
    ctx.lineTo(pb[0], pb[1]);
    ctx.stroke();
    ctx.restore();
  };

  // 1. Metric Y=0 Ground Grid (1m cells) + World Origin Triad
  if (state.showGrid) {
    const gridRadius = Math.max(4, Math.min(20, Math.ceil(dist * 0.85)));
    for (let i = -gridRadius; i <= gridRadius; i++) {
      const major = i === 0 ? null : i % 5 === 0;
      const col = i === 0
        ? 'rgba(255,255,255,0.22)'
        : major
          ? 'rgba(160,175,205,0.16)'
          : 'rgba(130,145,175,0.08)';
      drawSegment3D([i, 0, -gridRadius], [i, 0, gridRadius], col, i === 0 ? 1.2 : 0.8);
      drawSegment3D([-gridRadius, 0, i], [gridRadius, 0, i], col, i === 0 ? 1.2 : 0.8);
    }
    const axisLen = Math.max(0.35, dist * 0.12);
    drawSegment3D([0, 0.002, 0], [axisLen, 0.002, 0], '#ff6b6b', 2);
    drawSegment3D([0, 0, 0], [0, axisLen, 0], '#7fd17f', 2);
    drawSegment3D([0, 0.002, 0], [0, 0.002, axisLen], '#7aa2f7', 2);
  }

  // 2. Render Active LOD Mesh with Non-Destructive Fixup Applied
  const mesh = state.renderMesh;
  const fixup = state.fixup;
  const canUseWebGl =
    hasWebGlPreviewSupport() &&
    mesh &&
    mesh.indices?.length >= 3 &&
    fixup &&
    (state.shading === 'shaded' || state.shading === 'wireframe' || state.shading === 'normals');

  if (canUseWebGl) {
    const invScale = 1 / Math.max(1e-6, fixup.effectiveScale || 1);
    const tr = fixup.translation || [0, 0, 0];
    const rawEye = [
      (eye[0] - tr[0]) * invScale,
      (eye[1] - tr[1]) * invScale,
      (eye[2] - tr[2]) * invScale,
    ];
    const rawTarget = [
      (cam.target[0] - tr[0]) * invScale,
      (cam.target[1] - tr[1]) * invScale,
      (cam.target[2] - tr[2]) * invScale,
    ];
    const activeLod = state.meta?.import?.active_lod || 0;
    let glInfo = state.parsed;
    if (activeLod > 0 || !glInfo?.fullMesh?.indices?.length) {
      if (!state._lodGlInfo || state._lodGlMesh !== mesh) {
        state._lodGlMesh = mesh;
        state._lodGlInfo = {
          bounds: state.parsed?.bounds || [[-0.5, 0, -0.5], [0.5, 1, 0.5]],
          embeddedImages: state.parsed?.embeddedImages || [],
          submeshes: [
            {
              indexOffset: 0,
              indexCount: mesh.indices.length,
              imageIndex: state.parsed?.submeshes?.find((s) => typeof s.imageIndex === 'number')?.imageIndex ?? null,
              alphaCutoff: 0,
              baseColor: [0.85, 0.88, 0.94],
            },
          ],
          fullMesh: mesh,
        };
      }
      glInfo = state._lodGlInfo;
    }
    drawGlbPreviewCanvas(canvas, glInfo, {
      width: cssW,
      height: cssH,
      shading: state.shading === 'shaded' ? 'textured' : state.shading,
      skipBackground: true,
      clearTarget: false,
      camera: { eye: rawEye, target: rawTarget, fovRad },
      onTextureLoaded: state.onTextureLoaded || null,
    });
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  } else if (mesh && mesh.indices?.length >= 3 && fixup) {
    const pos = mesh.positions;
    const norm = mesh.normals;
    const tan = mesh.tangents;
    const uvs = mesh.uvs;
    const cols = mesh.colors;
    const idx = mesh.indices;
    const triTotal = Math.floor(idx.length / 3);
    const maxDrawTris = 6500;
    const triStep = Math.max(1, Math.ceil(triTotal / maxDrawTris));

    const keyDir = normalize3(preset.keyDir);
    const fillDir = normalize3(preset.fillDir);
    const mode = state.shading;
    const drawTris = [];

    for (let t = 0; t < triTotal; t += triStep) {
      const i0 = idx[t * 3], i1 = idx[t * 3 + 1], i2 = idx[t * 3 + 2];
      const w0 = fixup.applyPoint([pos[i0 * 3], pos[i0 * 3 + 1], pos[i0 * 3 + 2]]);
      const w1 = fixup.applyPoint([pos[i1 * 3], pos[i1 * 3 + 1], pos[i1 * 3 + 2]]);
      const w2 = fixup.applyPoint([pos[i2 * 3], pos[i2 * 3 + 1], pos[i2 * 3 + 2]]);

      const p0 = projectPoint(w0[0], w0[1], w0[2]);
      const p1 = projectPoint(w1[0], w1[1], w1[2]);
      const p2 = projectPoint(w2[0], w2[1], w2[2]);
      if (!p0 || !p1 || !p2) continue;

      let nx = (norm[i0 * 3] + norm[i1 * 3] + norm[i2 * 3]) / 3;
      let ny = (norm[i0 * 3 + 1] + norm[i1 * 3 + 1] + norm[i2 * 3 + 1]) / 3;
      let nz = (norm[i0 * 3 + 2] + norm[i1 * 3 + 2] + norm[i2 * 3 + 2]) / 3;
      const nl = Math.hypot(nx, ny, nz);
      if (nl > 1e-6) {
        nx /= nl; ny /= nl; nz /= nl;
      } else {
        const ux = w1[0] - w0[0], uy = w1[1] - w0[1], uz = w1[2] - w0[2];
        const vx = w2[0] - w0[0], vy = w2[1] - w0[1], vz = w2[2] - w0[2];
        const fn = normalize3([uy * vz - uz * vy, uz * vx - ux * vz, ux * vy - uy * vx]);
        nx = fn[0]; ny = fn[1]; nz = fn[2];
      }

      let r = 180, g = 190, b = 210;
      if (mode === 'normals') {
        r = Math.round((nx * 0.5 + 0.5) * 255);
        g = Math.round((ny * 0.5 + 0.5) * 255);
        b = Math.round((nz * 0.5 + 0.5) * 255);
      } else if (mode === 'tangents') {
        const tx = (tan[i0 * 4] + tan[i1 * 4] + tan[i2 * 4]) / 3;
        const ty = (tan[i0 * 4 + 1] + tan[i1 * 4 + 1] + tan[i2 * 4 + 1]) / 3;
        const tz = (tan[i0 * 4 + 2] + tan[i1 * 4 + 2] + tan[i2 * 4 + 2]) / 3;
        const tn = normalize3([tx, ty, tz]);
        r = Math.round((tn[0] * 0.5 + 0.5) * 255);
        g = Math.round((tn[1] * 0.5 + 0.5) * 255);
        b = Math.round((tn[2] * 0.5 + 0.5) * 255);
      } else {
        let baseR = (cols[i0 * 3] + cols[i1 * 3] + cols[i2 * 3]) / 3 || 0.68;
        let baseG = (cols[i0 * 3 + 1] + cols[i1 * 3 + 1] + cols[i2 * 3 + 1]) / 3 || 0.74;
        let baseB = (cols[i0 * 3 + 2] + cols[i1 * 3 + 2] + cols[i2 * 3 + 2]) / 3 || 0.84;

        if (mode === 'uv_checker' && uvs?.length >= (i2 + 1) * 2) {
          const uAvg = (uvs[i0 * 2] + uvs[i1 * 2] + uvs[i2 * 2]) / 3;
          const vAvg = (uvs[i0 * 2 + 1] + uvs[i1 * 2 + 1] + uvs[i2 * 2 + 1]) / 3;
          const cu = Math.floor(uAvg * 8);
          const cv = Math.floor(vAvg * 8);
          const odd = ((cu + cv) & 1) !== 0;
          const tintU = ((uAvg % 1) + 1) % 1;
          const tintV = ((vAvg % 1) + 1) % 1;
          baseR = odd ? 0.86 : 0.22 + tintU * 0.22;
          baseG = odd ? 0.88 : 0.26 + tintV * 0.22;
          baseB = odd ? 0.92 : 0.34;
        }

        const viewDot = nx * back[0] + ny * back[1] + nz * back[2];
        const snx = viewDot < 0 ? -nx : nx;
        const sny = viewDot < 0 ? -ny : ny;
        const snz = viewDot < 0 ? -nz : nz;

        const ndlKey = Math.max(0, snx * keyDir[0] + sny * keyDir[1] + snz * keyDir[2]);
        const ndlFill = Math.max(0, snx * fillDir[0] + sny * fillDir[1] + snz * fillDir[2]);
        const hemiMix = sny * 0.5 + 0.5;
        const rim = Math.pow(1 - Math.min(1, Math.abs(viewDot)), 2.2) * preset.rimStrength;

        const lr = preset.hemiGround[0] * (1 - hemiMix) + preset.hemiSky[0] * hemiMix + ndlKey * preset.keyColor[0] * 0.68 + ndlFill * preset.fillColor[0] * 0.28 + rim;
        const lg = preset.hemiGround[1] * (1 - hemiMix) + preset.hemiSky[1] * hemiMix + ndlKey * preset.keyColor[1] * 0.68 + ndlFill * preset.fillColor[1] * 0.28 + rim;
        const lb = preset.hemiGround[2] * (1 - hemiMix) + preset.hemiSky[2] * hemiMix + ndlKey * preset.keyColor[2] * 0.68 + ndlFill * preset.fillColor[2] * 0.28 + rim;

        r = clamp(Math.round(Math.pow(baseR * lr, 1 / 2.1) * 255), 0, 255);
        g = clamp(Math.round(Math.pow(baseG * lg, 1 / 2.1) * 255), 0, 255);
        b = clamp(Math.round(Math.pow(baseB * lb, 1 / 2.1) * 255), 0, 255);
      }

      drawTris.push({
        p0, p1, p2,
        depth: (p0[2] + p1[2] + p2[2]) / 3,
        fill: `rgb(${r},${g},${b})`,
      });
    }

    // Painter's sort back-to-front (largest positive camera Z first)
    drawTris.sort((a, b) => b.depth - a.depth);
    const wire = mode === 'wireframe';
    ctx.lineJoin = 'round';
    ctx.lineWidth = wire ? 0.85 : 0.45;
    for (const tri of drawTris) {
      ctx.beginPath();
      ctx.moveTo(tri.p0[0], tri.p0[1]);
      ctx.lineTo(tri.p1[0], tri.p1[1]);
      ctx.lineTo(tri.p2[0], tri.p2[1]);
      ctx.closePath();
      ctx.fillStyle = tri.fill;
      ctx.fill();
      ctx.strokeStyle = wire ? 'rgba(232, 180, 74, 0.72)' : tri.fill;
      ctx.stroke();
    }
  } else {
    ctx.fillStyle = 'rgba(217, 220, 227, 0.45)';
    ctx.font = '13px system-ui, sans-serif';
    ctx.textAlign = 'center';
    ctx.fillText(state.emptyLabel || 'Select a .glb model, drop a file, or spawn a Graybox primitive', cssW * 0.5, cssH * 0.5);
  }

  // 3. Bounds Box Overlay
  if (state.showBounds && fixup && state.parsed?.triangles > 0) {
    const [bMin, bMax] = fixup.transformedBounds;
    const corners = [
      [bMin[0], bMin[1], bMin[2]], [bMax[0], bMin[1], bMin[2]],
      [bMax[0], bMin[1], bMax[2]], [bMin[0], bMin[1], bMax[2]],
      [bMin[0], bMax[1], bMin[2]], [bMax[0], bMax[1], bMin[2]],
      [bMax[0], bMax[1], bMax[2]], [bMin[0], bMax[1], bMax[2]],
    ];
    const edges = [
      [0, 1], [1, 2], [2, 3], [3, 0],
      [4, 5], [5, 6], [6, 7], [7, 4],
      [0, 4], [1, 5], [2, 6], [3, 7],
    ];
    for (const [ea, eb] of edges) {
      drawSegment3D(corners[ea], corners[eb], 'rgba(122, 162, 247, 0.42)', 1, [3, 3]);
    }
  }

  // 4. Physics Collider Wireframe Overlay
  if (state.showCollider && state.meta?.collider) {
    const col = state.meta.collider;
    const stroke = '#6bff95';
    const [cx, cy, cz] = col.center;
    if (col.shape === 'box') {
      const [hx, hy, hz] = col.half_extents;
      const c = [
        [cx - hx, cy - hy, cz - hz], [cx + hx, cy - hy, cz - hz],
        [cx + hx, cy - hy, cz + hz], [cx - hx, cy - hy, cz + hz],
        [cx - hx, cy + hy, cz - hz], [cx + hx, cy + hy, cz - hz],
        [cx + hx, cy + hy, cz + hz], [cx - hx, cy + hy, cz + hz],
      ];
      for (const [ea, eb] of [[0, 1], [1, 2], [2, 3], [3, 0], [4, 5], [5, 6], [6, 7], [7, 4], [0, 4], [1, 5], [2, 6], [3, 7]]) {
        drawSegment3D(c[ea], c[eb], stroke, 1.5);
      }
    } else if (col.shape === 'sphere' || col.shape === 'capsule') {
      const r = col.radius;
      const halfStraight = col.shape === 'capsule' ? Math.max(0, (col.height - 2 * r) * 0.5) : 0;
      const ringSegs = 24;
      for (const yOff of halfStraight > 0 ? [-halfStraight, halfStraight] : [0]) {
        for (let i = 0; i < ringSegs; i++) {
          const a0 = (i / ringSegs) * Math.PI * 2;
          const a1 = ((i + 1) / ringSegs) * Math.PI * 2;
          drawSegment3D(
            [cx + Math.cos(a0) * r, cy + yOff, cz + Math.sin(a0) * r],
            [cx + Math.cos(a1) * r, cy + yOff, cz + Math.sin(a1) * r],
            stroke,
            1.4,
          );
        }
      }
      for (let i = 0; i < ringSegs; i++) {
        const a0 = (i / ringSegs) * Math.PI * 2;
        const a1 = ((i + 1) / ringSegs) * Math.PI * 2;
        const y0 = Math.sin(a0) * r + (Math.sin(a0) >= 0 ? halfStraight : -halfStraight);
        const y1 = Math.sin(a1) * r + (Math.sin(a1) >= 0 ? halfStraight : -halfStraight);
        drawSegment3D([cx + Math.cos(a0) * r, cy + y0, cz], [cx + Math.cos(a1) * r, cy + y1, cz], stroke, 1.4);
        drawSegment3D([cx, cy + y0, cz + Math.cos(a0) * r], [cx, cy + y1, cz + Math.cos(a1) * r], stroke, 1.4);
      }
    } else if (col.shape === 'convex_hull' && col.hull_points?.length) {
      const pts = col.hull_points;
      for (let i = 0; i < pts.length; i++) {
        const next = pts[(i + 1) % pts.length];
        drawSegment3D(pts[i], next, stroke, 1.3);
        drawSegment3D(col.center, pts[i], 'rgba(107, 255, 149, 0.25)', 0.9);
      }
    }
  }

  // 5. Named Socket 3D Orientation Gizmos & Labels
  if (state.showSockets && state.meta?.sockets) {
    const gizmoLen = Math.max(0.14, dist * 0.065);
    for (const [sockName, sock] of Object.entries(state.meta.sockets)) {
      const o = sock.translation;
      const axX = rotateByQuat([gizmoLen, 0, 0], sock.rotation);
      const axY = rotateByQuat([0, gizmoLen, 0], sock.rotation);
      const axZ = rotateByQuat([0, 0, gizmoLen], sock.rotation);
      drawSegment3D(o, [o[0] + axX[0], o[1] + axX[1], o[2] + axX[2]], '#ff6b6b', 2);
      drawSegment3D(o, [o[0] + axY[0], o[1] + axY[1], o[2] + axY[2]], '#7fd17f', 2);
      drawSegment3D(o, [o[0] + axZ[0], o[1] + axZ[1], o[2] + axZ[2]], '#5cc9f5', 2);
      const sp = projectPoint(o[0], o[1], o[2]);
      if (sp) {
        ctx.fillStyle = '#5cc9f5';
        ctx.beginPath();
        ctx.arc(sp[0], sp[1], 3.2, 0, Math.PI * 2);
        ctx.fill();
        ctx.font = '600 11px system-ui, sans-serif';
        ctx.textAlign = 'left';
        ctx.fillStyle = '#e6f6ff';
        ctx.fillText(sockName, sp[0] + 6, sp[1] - 4);
      }
    }
  }
}

// ---- StudioWorkbenchInstance Mount ------------------------------------------------------------

/** Mounts the unified 3D Mesh, LookDev & Kitbash Visualizer inside `container` using Lane `[04]`'s
 * canonical `StudioWorkbenchApi` contract. Returns a `StudioWorkbenchInstance`. */
export function mountModelStudio(container, studioApi = {}) {
  if (!container) throw new Error('mountModelStudio requires a container element');
  const docRef = container.ownerDocument || (typeof document !== 'undefined' ? document : null);
  ensureModelStudioStyles(docRef);

  const tr = (key, fallback, ...args) => {
    if (typeof studioApi.t === 'function') {
      const out = studioApi.t(key, ...args);
      if (out && out !== key) return out;
    }
    let text = fallback || key;
    args.forEach((a, idx) => { text = text.replace(`{${idx}}`, String(a)); });
    return text;
  };

  const h = (tag, attrs = {}, ...children) => {
    const node = docRef.createElement(tag);
    for (const [k, v] of Object.entries(attrs)) {
      if (v === null || v === undefined || v === false) continue;
      if (k === 'class') node.className = v;
      else if (k.startsWith('on') && typeof v === 'function') node.addEventListener(k.slice(2).toLowerCase(), v);
      else if (v === true) node.setAttribute(k, '');
      else node.setAttribute(k, String(v));
    }
    for (const c of children.flat()) {
      if (c === null || c === undefined || c === false) continue;
      if (typeof c === 'string' || typeof c === 'number') node.appendChild(docRef.createTextNode(String(c)));
      else node.appendChild(c);
    }
    return node;
  };

  const state = {
    active: true,
    destroyed: false,
    models: [],
    selectedAssetId: null,
    selectedPath: '',
    sourceKind: 'none', // 'project' | 'intake' | 'primitive' | 'drop'
    rawBytes: null,
    parsed: null,
    renderMesh: null,
    fixup: null,
    meta: normalizeModelMeta({}),
    metaDirty: false,
    inspectorTab: 'fixup', // 'fixup' | 'lod_collider' | 'sockets'
    lighting: 'studio',
    shading: 'shaded',
    showGrid: true,
    showBounds: true,
    showCollider: true,
    showSockets: true,
    turntableOn: false,
    turntableRaf: 0,
    camera: {
      yaw: 0.65,
      pitch: 0.36,
      distance: 4.5,
      target: [0, 0.8, 0],
      fov: 46,
    },
    primitive: {
      kind: 'box',
      width: 2,
      height: 2,
      depth: 2,
      radius: 0.75,
      thickness: 0.45,
      segments: 20,
      targetFolder: 'content/models/graybox',
      name: 'graybox_box',
    },
    intake: {
      roots: [],
      items: [],
      selectedKey: '',
      normalizeOnIngest: true,
    },
    newSocketName: '',
    statusText: 'Ready',
    statusTone: '',
  };

  const unsubs = [];

  function setStatus(msg, tone = '') {
    state.statusText = msg;
    state.statusTone = tone;
    if (statusBar) {
      statusBar.textContent = msg;
      statusBar.className = `ms-status${tone ? ' ' + tone : ''}`;
    }
  }

  function listProjectModels() {
    if (typeof studioApi.listAssets === 'function') {
      const list = studioApi.listAssets({ kind: 'model' });
      if (Array.isArray(list)) return list;
    }
    const all = typeof studioApi.getAssets === 'function' ? studioApi.getAssets() : [];
    return Array.isArray(all) ? all.filter((a) => a.kind === 'model') : [];
  }

  function resolveAssetUrl(assetOrPath) {
    const rel = typeof assetOrPath === 'string' ? assetOrPath : assetOrPath?.path || '';
    if (typeof studioApi.assetUrl === 'function') return studioApi.assetUrl(rel);
    return '/project/' + rel.split('/').map(encodeURIComponent).join('/');
  }

  function recomputeGeometryAndFrame(frameCamera = false) {
    if (!state.parsed) {
      state.fixup = null;
      state.renderMesh = null;
      requestPaint();
      renderBudgetBar();
      return;
    }
    state.fixup = computeImportFixup(state.parsed.bounds, state.meta.import);
    const activeLod = state.meta.import.active_lod || 0;
    const lodRatio = activeLod === 0 ? 1.0 : (state.meta.lods[activeLod] ?? 1.0);
    state.renderMesh = decimatePreviewMesh(state.parsed.fullMesh, lodRatio);
    if (frameCamera) frameCurrentModel();
    requestPaint();
    renderBudgetBar();
  }

  function frameCurrentModel() {
    if (!state.fixup) return;
    const [bMin, bMax] = state.fixup.transformedBounds;
    const [sx, sy, sz] = state.fixup.transformedSize;
    const maxDim = Math.max(sx, sy, sz, 0.5);
    state.camera.target = [
      (bMin[0] + bMax[0]) * 0.5,
      (bMin[1] + bMax[1]) * 0.5,
      (bMin[2] + bMax[2]) * 0.5,
    ];
    state.camera.distance = clamp(maxDim * 2.1, 0.8, 250);
  }

  function applyParsedModel(u8, parsed, { label, sourceKind, assetId = null, meta = null }) {
    if (!parsed) {
      setStatus(`Invalid or unsupported .glb: ${label}`, 'err');
      return false;
    }
    state.rawBytes = u8;
    state.parsed = parsed;
    state._lodGlInfo = null;
    state._lodGlMesh = null;
    state.selectedAssetId = assetId;
    state.selectedPath = label;
    state.sourceKind = sourceKind;
    state.meta = normalizeModelMeta(meta || {});
    state.metaDirty = false;
    recomputeGeometryAndFrame(true);
    renderLeftRail();
    renderInspector();
    setStatus(`Loaded ${assetBasename(label)} (${parsed.triangles} tris)`, 'ok');
    return true;
  }

  function loadBytesIntoStage(bytes, opts) {
    const u8 = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
    const parsed = parseGlbPreview(u8);
    return applyParsedModel(u8, parsed, opts);
  }

  async function loadBytesIntoStageAsync(bytes, opts) {
    const u8 = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
    const parsed = (await parseGlbPreviewAsync(u8, { maxFullTris: 180000 })) || parseGlbPreview(u8);
    return applyParsedModel(u8, parsed, opts);
  }

  async function inspectProjectAsset(idOrPath) {
    if (!idOrPath) return false;
    const models = listProjectModels();
    state.models = models;
    const asset = models.find((a) => a.id === idOrPath || a.path === idOrPath);
    if (!asset) return false;
    try {
      const url = resolveAssetUrl(asset);
      if (typeof fetch !== 'function') return false;
      setStatus(`Loading ${assetBasename(asset.path)}…`, 'info');
      const res = await fetch(url);
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const buf = await res.arrayBuffer();
      let fetchedMeta = {};
      if (typeof studioApi.hostRequest === 'function') {
        try {
          const reply = await studioApi.hostRequest({
            type: 'asset-usage',
            op: 'meta-get',
            id: asset.id,
          });
          if (reply?.meta) fetchedMeta = reply.meta;
        } catch {
          // Pure static/cloud session without sidecar RPC uses default meta
        }
      }
      return await loadBytesIntoStageAsync(buf, {
        label: asset.path,
        sourceKind: 'project',
        assetId: asset.id,
        meta: fetchedMeta,
      });
    } catch (err) {
      setStatus(`Failed to load ${asset.path}: ${err?.message || err}`, 'err');
      return false;
    }
  }

  async function refreshIntakeCatalog() {
    if (typeof studioApi.hostRequest !== 'function') return;
    try {
      const reply = await studioApi.hostRequest({
        type: 'asset-usage',
        op: 'intake-list',
      });
      state.intake.roots = Array.isArray(reply?.roots) ? reply.roots : [];
      state.intake.items = Array.isArray(reply?.items) ? reply.items : [];
      renderLeftRail();
    } catch {
      state.intake.roots = [];
      state.intake.items = [];
    }
  }

  async function previewIntakeItem(item) {
    if (!item || typeof studioApi.hostRequest !== 'function') return;
    state.intake.selectedKey = `${item.intakeIndex}:${item.intakePath}`;
    try {
      const reply = await studioApi.hostRequest({
        type: 'asset-usage',
        op: 'intake-preview',
        intakeIndex: item.intakeIndex,
        intakePath: item.intakePath,
      });
      if (!reply?.dataBase64) return;
      const bytes = base64ToBytes(reply.dataBase64);
      if (item.kind === 'model') {
        const defaultMeta = state.intake.normalizeOnIngest
          ? { import: { pivot: 'bottom_center', scale: 1.0 } }
          : {};
        await loadBytesIntoStageAsync(bytes, {
          label: `${item.rootLabel}/${item.intakePath}`,
          sourceKind: 'intake',
          meta: defaultMeta,
        });
      } else {
        setStatus(`Intake ${item.kind}: ${item.name} (${formatBytes(item.bytes)}) ready to ingest`, 'ok');
      }
    } catch (err) {
      setStatus(`Intake preview failed: ${err?.message || err}`, 'err');
    }
  }

  async function ingestIntakeItem(item) {
    if (!item || typeof studioApi.hostRequest !== 'function') return;
    const project = typeof studioApi.getProject === 'function' ? studioApi.getProject() : null;
    const contentRoot = project?.content?.[0] || 'content';
    const subDir = item.kind === 'model' ? 'models/intake' : item.kind === 'texture' ? 'textures/intake' : 'audio/intake';
    const destPath = `${contentRoot}/${subDir}/${item.name}`;
    const initialMeta = item.kind === 'model'
      ? serializeModelMeta(
          state.sourceKind === 'intake'
            ? state.meta
            : { import: { pivot: state.intake.normalizeOnIngest ? 'bottom_center' : 'origin', scale: 1.0 } },
        )
      : undefined;
    try {
      const reply = await studioApi.hostRequest({
        type: 'asset-import',
        intakeIndex: item.intakeIndex,
        intakePath: item.intakePath,
        path: destPath,
        ...(initialMeta ? { meta: initialMeta } : {}),
      });
      setStatus(`Ingested ${destPath}`, 'ok');
      studioApi.notify?.('info', `Ingested ${destPath}`);
      if (reply?.id && item.kind === 'model') {
        await inspectProjectAsset(reply.id);
      }
    } catch (err) {
      setStatus(`Ingest failed: ${err?.message || err}`, 'err');
    }
  }

  function previewCurrentPrimitive() {
    const p = state.primitive;
    const glb = buildPrimitiveGlb(p.kind, p);
    loadBytesIntoStage(glb, {
      label: `${p.targetFolder}/${p.name}.glb`,
      sourceKind: 'primitive',
      meta: {
        import: { pivot: 'origin', scale: 1.0 },
        collider: fitPhysicsCollider(parseGlbPreview(glb)?.fullMesh, p.kind === 'cylinder' ? 'capsule' : 'box'),
      },
    });
  }

  async function importCurrentStageModel() {
    const p = state.primitive;
    const bytes = state.sourceKind === 'primitive' ? buildPrimitiveGlb(p.kind, p) : state.rawBytes;
    if (!bytes) return;
    const cleanFolder = (p.targetFolder || 'content/models/graybox').replace(/^\/+|\/+$/g, '');
    const stem = (p.name || `graybox_${p.kind}`).replace(/\.glb$/i, '').replace(/[^A-Za-z0-9_.-]+/g, '_');
    const targetPath = `${cleanFolder}/${stem}.glb`;
    try {
      let importedId = null;
      if (typeof studioApi.importAsset === 'function') {
        const res = await studioApi.importAsset(targetPath, bytes);
        importedId = res?.id || null;
      } else if (typeof studioApi.hostRequest === 'function') {
        let binStr = '';
        for (let i = 0; i < bytes.length; i++) binStr += String.fromCharCode(bytes[i]);
        const b64 = typeof btoa === 'function' ? btoa(binStr) : Buffer.from(bytes).toString('base64');
        const res = await studioApi.hostRequest({
          type: 'asset-import',
          path: targetPath,
          dataBase64: b64,
          meta: serializeModelMeta(state.meta),
        });
        importedId = res?.id || null;
      }
      if (importedId && typeof studioApi.hostRequest === 'function') {
        await studioApi.hostRequest({
          type: 'asset-usage',
          op: 'meta-set',
          id: importedId,
          meta: serializeModelMeta(state.meta),
        }).catch(() => {});
      }
      setStatus(`Imported ${targetPath}`, 'ok');
      studioApi.notify?.('info', `Imported ${targetPath}`);
      if (importedId) await inspectProjectAsset(importedId);
    } catch (err) {
      setStatus(`Import failed: ${err?.message || err}`, 'err');
    }
  }

  async function saveSidecarMeta() {
    if (!state.selectedAssetId || typeof studioApi.hostRequest !== 'function') {
      setStatus('Import model into project first to save .meta.json', 'err');
      return;
    }
    try {
      const payload = serializeModelMeta(state.meta);
      await studioApi.hostRequest({
        type: 'asset-usage',
        op: 'meta-set',
        id: state.selectedAssetId,
        meta: payload,
      });
      state.metaDirty = false;
      renderInspectorHeader();
      setStatus(`Saved ${state.selectedPath}.meta.json (DDC updated)`, 'ok');
      studioApi.notify?.('info', `Saved ${assetBasename(state.selectedPath)}.meta.json`);
    } catch (err) {
      setStatus(`Save failed: ${err?.message || err}`, 'err');
    }
  }

  async function generateModelPrefab() {
    if (!state.selectedAssetId) {
      setStatus('Select or import a project model before generating a prefab', 'err');
      return;
    }
    const modelAsset = state.models.find((m) => m.id === state.selectedAssetId) || {
      id: state.selectedAssetId,
      path: state.selectedPath,
    };
    const stem = assetStem(modelAsset.path) || 'prop';
    const project = typeof studioApi.getProject === 'function' ? studioApi.getProject() : null;
    const rootDir = project?.content?.[0] || 'content';
    const prefabPath = `${rootDir}/prefabs/${stem}.prefab.json`;
    const sockCount = Object.keys(state.meta.sockets || {}).length;

    try {
      let entityIds = [];
      if (typeof studioApi.hostRequest === 'function') {
        const idReply = await studioApi.hostRequest({ type: 'ids', count: 1 + sockCount });
        if (Array.isArray(idReply?.ids)) entityIds = idReply.ids;
      }
      const ops = buildModelPrefabOps(modelAsset, state.meta, entityIds);
      let docId = null;
      if (typeof studioApi.createDocument === 'function') {
        const created = await studioApi.createDocument({
          kind: 'prefab',
          path: prefabPath,
          name: stem,
        });
        docId = typeof created === 'string' ? created : created?.doc || created?.id;
      } else if (typeof studioApi.hostRequest === 'function') {
        const created = await studioApi.hostRequest({
          type: 'create',
          kind: 'prefab',
          path: prefabPath,
          name: stem,
        });
        docId = created?.doc;
      }
      if (docId) {
        if (typeof studioApi.openDocument === 'function') await studioApi.openDocument(docId);
        if (typeof studioApi.edit === 'function') {
          await studioApi.edit(docId, ops, 'Author Model Prefab');
        } else if (typeof studioApi.hostRequest === 'function') {
          await studioApi.hostRequest({ type: 'open', doc: docId });
          await studioApi.hostRequest({ type: 'edit', doc: docId, rev: 0, label: 'Author Model Prefab', ops });
        }
        if (typeof studioApi.save === 'function') await studioApi.save(docId);
      }
      setStatus(`Created ${prefabPath} (${sockCount} sockets)`, 'ok');
      studioApi.notify?.('info', `Created ${prefabPath}`);
    } catch (err) {
      setStatus(`Prefab creation failed: ${err?.message || err}`, 'err');
    }
  }

  // ---- Build DOM Skeleton ---------------------------------------------------------------------

  const leftRail = h('aside', { class: 'ms-rail' });
  const toolbar = h('div', { class: 'ms-toolbar' });
  const canvas = h('canvas', { class: 'ms-canvas' });
  const viewportWrap = h('div', { class: 'ms-viewport-wrap' }, canvas);
  const budgetBar = h('div', { class: 'ms-budget-bar' });
  const stage = h('section', { class: 'ms-stage' }, toolbar, viewportWrap, budgetBar);

  const inspectorHeader = h('div', { class: 'ms-inspector-header' });
  const inspectorTabs = h('div', { class: 'ms-tabs' });
  const inspectorBody = h('div', { class: 'ms-inspector-body' });
  const statusBar = h('div', { class: 'ms-status' }, state.statusText);
  const rightInspector = h('aside', { class: 'ms-inspector' }, inspectorHeader, inspectorTabs, inspectorBody, statusBar);

  const root = h('div', { class: 'ms-root', 'data-workbench': WORKBENCH_ID }, leftRail, stage, rightInspector);
  container.replaceChildren ? container.replaceChildren(root) : (container.innerHTML = '', container.appendChild(root));

  function requestPaint() {
    if (state.destroyed || !state.active) return;
    drawLookDevViewport(canvas, state);
  }
  state.onTextureLoaded = requestPaint;

  function stepTurntable() {
    if (state.destroyed || !state.turntableOn || !state.active) return;
    state.camera.yaw += 0.012;
    requestPaint();
    if (typeof requestAnimationFrame === 'function') {
      state.turntableRaf = requestAnimationFrame(stepTurntable);
    }
  }

  function renderToolbar() {
    const lightGroup = h('div', { class: 'ms-toolbar-group' },
      ...Object.values(LIGHTING_PRESETS).map((lp) => h('button', {
        type: 'button',
        class: `ms-chip${state.lighting === lp.id ? ' on' : ''}`,
        onclick: () => { state.lighting = lp.id; renderToolbar(); requestPaint(); },
      }, lp.label)),
    );

    const shadeGroup = h('div', { class: 'ms-toolbar-group' },
      ...SHADING_MODES.map((sm) => h('button', {
        type: 'button',
        class: `ms-chip${state.shading === sm.id ? ' on' : ''}`,
        onclick: () => { state.shading = sm.id; renderToolbar(); requestPaint(); },
      }, sm.label)),
    );

    const overlayGroup = h('div', { class: 'ms-toolbar-group' },
      h('button', {
        type: 'button',
        class: `ms-chip${state.showGrid ? ' on' : ''}`,
        onclick: () => { state.showGrid = !state.showGrid; renderToolbar(); requestPaint(); },
      }, 'Grid'),
      h('button', {
        type: 'button',
        class: `ms-chip${state.showCollider ? ' on' : ''}`,
        onclick: () => { state.showCollider = !state.showCollider; renderToolbar(); requestPaint(); },
      }, 'Collider'),
      h('button', {
        type: 'button',
        class: `ms-chip${state.showSockets ? ' on' : ''}`,
        onclick: () => { state.showSockets = !state.showSockets; renderToolbar(); requestPaint(); },
      }, 'Sockets'),
      h('button', {
        type: 'button',
        class: `ms-chip${state.turntableOn ? ' on' : ''}`,
        onclick: () => {
          state.turntableOn = !state.turntableOn;
          if (state.turntableOn) stepTurntable();
          renderToolbar();
        },
      }, 'Turntable'),
      h('button', {
        type: 'button',
        class: 'ms-chip',
        onclick: () => { frameCurrentModel(); requestPaint(); },
      }, tr('frame', 'Frame')),
    );

    toolbar.replaceChildren(
      lightGroup,
      h('span', { class: 'ms-toolbar-sep' }),
      shadeGroup,
      h('span', { class: 'ms-toolbar-sep' }),
      overlayGroup,
    );
  }

  function renderBudgetBar() {
    if (!state.parsed || !state.fixup) {
      budgetBar.replaceChildren(h('span', { class: 'ms-budget-pill' }, 'Drop or select a .glb to inspect metrics'));
      return;
    }
    const p = state.parsed;
    const dims = state.fixup.transformedSize.map((v) => v.toFixed(2)).join('×');
    const activeTris = state.renderMesh?.triangles ?? p.triangles;
    const activeVerts = state.renderMesh?.vertices ?? p.vertices;
    const vramWarn = p.estimatedVramBytes > 8 * 1024 * 1024;
    const triWarn = activeTris > 50000;

    budgetBar.replaceChildren(
      h('span', { class: 'ms-budget-pill' }, h('strong', {}, `${dims} m`), 'W×H×D'),
      h('span', { class: `ms-budget-pill${triWarn ? ' warn' : ''}` }, h('strong', {}, String(activeTris)), `tris (LOD${state.meta.import.active_lod})`),
      h('span', { class: 'ms-budget-pill' }, h('strong', {}, String(activeVerts)), 'verts'),
      h('span', { class: 'ms-budget-pill' }, h('strong', {}, String(p.bones || 0)), 'bones'),
      h('span', { class: `ms-budget-pill${vramWarn ? ' warn' : ''}` }, h('strong', {}, formatBytes(p.estimatedVramBytes)), 'VRAM est.'),
      h('span', { class: `ms-attr-badge${p.hasNormals ? '' : ' missing'}` }, 'NRM'),
      h('span', { class: `ms-attr-badge${p.hasTangents ? '' : ' missing'}` }, 'TAN'),
      h('span', { class: `ms-attr-badge${p.hasUvs ? '' : ' missing'}` }, 'UV0'),
    );
  }

  function renderLeftRail() {
    state.models = listProjectModels();
    const modelSection = h('div', { class: 'ms-section' },
      h('div', { class: 'ms-section-head' },
        h('span', {}, `Project Models (${state.models.length})`),
        h('button', {
          type: 'button',
          class: 'ms-chip',
          onclick: () => {
            studioApi.rescanAssets?.();
            renderLeftRail();
          },
        }, '↻'),
      ),
      h('div', { class: 'ms-list' },
        ...(state.models.length
          ? state.models.map((m) => h('button', {
              type: 'button',
              class: `ms-item${state.selectedAssetId === m.id ? ' active' : ''}`,
              title: m.path,
              onclick: () => inspectProjectAsset(m.id),
            },
              h('span', { class: 'ms-item-name' }, assetBasename(m.path)),
              h('span', { class: 'ms-item-sub' }, m.bytes ? formatBytes(m.bytes) : ''),
            ))
          : [h('div', { class: 'ms-item-sub' }, 'No .glb models in content/ yet')]),
      ),
    );

    const prim = state.primitive;
    const primSection = h('div', { class: 'ms-section' },
      h('div', { class: 'ms-section-head' }, h('span', {}, 'CSG / Graybox Primitives')),
      h('div', { class: 'ms-prim-chips' },
        ...PRIMITIVE_KINDS.map((kind) => h('button', {
          type: 'button',
          class: `ms-chip${prim.kind === kind ? ' on' : ''}`,
          onclick: () => {
            prim.kind = kind;
            prim.name = `graybox_${kind}`;
            previewCurrentPrimitive();
          },
        }, kind)),
      ),
      h('div', { class: 'ms-grid-2' },
        h('label', { class: 'ms-field' }, 'Width (m)', h('input', {
          type: 'number', step: '0.25', min: '0.1', value: prim.width,
          onchange: (ev) => { prim.width = Math.max(0.1, Number(ev.target.value) || 2); previewCurrentPrimitive(); },
        })),
        h('label', { class: 'ms-field' }, 'Height (m)', h('input', {
          type: 'number', step: '0.25', min: '0.1', value: prim.height,
          onchange: (ev) => { prim.height = Math.max(0.1, Number(ev.target.value) || 2); previewCurrentPrimitive(); },
        })),
        h('label', { class: 'ms-field' }, 'Depth (m)', h('input', {
          type: 'number', step: '0.25', min: '0.1', value: prim.depth,
          onchange: (ev) => { prim.depth = Math.max(0.1, Number(ev.target.value) || 2); previewCurrentPrimitive(); },
        })),
        h('label', { class: 'ms-field' }, 'Name', h('input', {
          type: 'text', value: prim.name,
          onchange: (ev) => { prim.name = ev.target.value.trim() || `graybox_${prim.kind}`; },
        })),
      ),
      h('div', { class: 'ms-btn-row' },
        h('button', { type: 'button', class: 'ms-chip', onclick: previewCurrentPrimitive }, 'Preview'),
        h('button', { type: 'button', class: 'ms-chip ms-btn-primary', onclick: importCurrentStageModel }, '+ Import .glb'),
      ),
    );

    const intakeItems = state.intake.items;
    const intakeSection = h('div', { class: 'ms-section' },
      h('div', { class: 'ms-section-head' },
        h('span', {}, `Art Intake (${intakeItems.length})`),
        h('button', { type: 'button', class: 'ms-chip', onclick: refreshIntakeCatalog }, 'Scan'),
      ),
      h('label', { class: 'ms-field', style: 'flex-direction:row;align-items:center;gap:5px;margin-bottom:5px' },
        h('input', {
          type: 'checkbox',
          checked: state.intake.normalizeOnIngest,
          onchange: (ev) => { state.intake.normalizeOnIngest = !!ev.target.checked; },
        }),
        'Bottom-center pivot on ingest',
      ),
      h('div', { class: 'ms-list' },
        ...(intakeItems.length
          ? intakeItems.slice(0, 60).map((it) => {
              const key = `${it.intakeIndex}:${it.intakePath}`;
              return h('div', {
                class: `ms-item${state.intake.selectedKey === key ? ' active' : ''}`,
                onclick: () => previewIntakeItem(it),
              },
                h('span', { class: 'ms-item-name', title: `${it.rootLabel}/${it.intakePath}` }, it.name),
                h('button', {
                  type: 'button',
                  class: 'ms-chip',
                  onclick: (ev) => { ev.stopPropagation(); ingestIntakeItem(it); },
                }, 'Ingest'),
              );
            })
          : [h('div', { class: 'ms-item-sub' }, 'Configure "intake" folders in ganbate.project.json for ganbate-art / tripo3d-queue')]),
      ),
    );

    leftRail.replaceChildren(modelSection, primSection, intakeSection);
  }

  function markMetaEdited(recompute = true) {
    state.metaDirty = true;
    if (recompute) recomputeGeometryAndFrame(false);
    else requestPaint();
    renderInspectorHeader();
  }

  function renderInspectorHeader() {
    const canSave = !!state.selectedAssetId;
    inspectorHeader.replaceChildren(
      h('div', { class: 'ms-title-wrap' },
        h('div', { class: 'ms-title' }, state.selectedPath ? assetBasename(state.selectedPath) : 'Model Authoring'),
        h('div', { class: 'ms-subtitle' }, state.selectedPath || 'Non-destructive .meta.json pipeline'),
      ),
      canSave
        ? h('button', {
            type: 'button',
            class: 'ms-chip ms-btn-primary',
            onclick: saveSidecarMeta,
          }, state.metaDirty ? 'Save .meta*' : 'Save .meta')
        : state.rawBytes
          ? h('button', {
              type: 'button',
              class: 'ms-chip ms-btn-primary',
              onclick: importCurrentStageModel,
            }, 'Save to Project')
          : null,
    );

    const tabs = [
      { id: 'fixup', label: 'Import Fixup' },
      { id: 'lod_collider', label: 'LOD & Collider' },
      { id: 'sockets', label: 'Sockets & Prefab' },
    ];
    inspectorTabs.replaceChildren(
      ...tabs.map((tb) => h('button', {
        type: 'button',
        class: `ms-tab${state.inspectorTab === tb.id ? ' on' : ''}`,
        onclick: () => { state.inspectorTab = tb.id; renderInspector(); },
      }, tb.label)),
    );
  }

  function renderInspector() {
    renderInspectorHeader();
    const imp = state.meta.import;

    if (state.inspectorTab === 'fixup') {
      const pivotSection = h('div', { class: 'ms-section' },
        h('div', { class: 'ms-section-head' }, h('span', {}, '1-Click Pivot Normalizer')),
        h('div', { class: 'ms-chip-row' },
          ...[
            { id: 'bottom_center', label: 'Bottom-Center (Y=0)' },
            { id: 'centroid', label: 'Centroid' },
            { id: 'origin', label: 'Source Origin' },
          ].map((p) => h('button', {
            type: 'button',
            class: `ms-chip${imp.pivot === p.id ? ' on' : ''}`,
            onclick: () => { imp.pivot = p.id; markMetaEdited(true); renderInspector(); },
          }, p.label)),
        ),
        h('div', { class: 'ms-grid-3', style: 'margin-top:8px' },
          ...['X', 'Y', 'Z'].map((axis, idx) => h('label', { class: 'ms-field' },
            `Offset ${axis} (m)`,
            h('input', {
              type: 'number', step: '0.05', value: imp.offset[idx],
              onchange: (ev) => { imp.offset[idx] = roundNum(ev.target.value, 4); markMetaEdited(true); },
            }),
          )),
        ),
      );

      const scaleSection = h('div', { class: 'ms-section' },
        h('div', { class: 'ms-section-head' }, h('span', {}, 'Unit Scale Normalizer')),
        h('div', { class: 'ms-chip-row' },
          ...[
            { s: 1.0, label: '1.0 (m)' },
            { s: 0.01, label: '0.01 (cm → m)' },
            { s: 0.001, label: '0.001 (mm → m)' },
          ].map((preset) => h('button', {
            type: 'button',
            class: `ms-chip${imp.target_height === null && Math.abs(imp.scale - preset.s) < 1e-6 ? ' on' : ''}`,
            onclick: () => {
              imp.scale = preset.s;
              imp.target_height = null;
              markMetaEdited(true);
              renderInspector();
            },
          }, preset.label)),
        ),
        h('div', { class: 'ms-grid-2', style: 'margin-top:8px' },
          h('label', { class: 'ms-field' }, 'Uniform Scale', h('input', {
            type: 'number', step: '0.01', min: '0.0001', value: imp.scale,
            onchange: (ev) => {
              imp.scale = clamp(Number(ev.target.value) || 1, 1e-5, 1e5);
              markMetaEdited(true);
              renderInspector();
            },
          })),
          h('label', { class: 'ms-field' }, 'Target Height (m)', h('input', {
            type: 'number', step: '0.1', min: '0', placeholder: 'Auto off',
            value: imp.target_height ?? '',
            onchange: (ev) => {
              const v = Number(ev.target.value);
              imp.target_height = Number.isFinite(v) && v > 0 ? roundNum(v, 4) : null;
              markMetaEdited(true);
              renderInspector();
            },
          })),
        ),
      );

      inspectorBody.replaceChildren(pivotSection, scaleSection);
      return;
    }

    if (state.inspectorTab === 'lod_collider') {
      const baseTris = state.parsed?.triangles || 0;
      const lodSection = h('div', { class: 'ms-section' },
        h('div', { class: 'ms-section-head' }, h('span', {}, 'Auto-LOD Preview & Ratios')),
        h('div', { class: 'ms-lod-grid' },
          ...[0, 1, 2, 3].map((lodIdx) => {
            const ratio = lodIdx === 0 ? 1.0 : (state.meta.lods[lodIdx] ?? DEFAULT_LOD_RATIOS[lodIdx]);
            const estTris = Math.max(baseTris > 0 ? 1 : 0, Math.round(baseTris * ratio));
            return h('div', {
              class: `ms-lod-card${imp.active_lod === lodIdx ? ' active' : ''}`,
              onclick: () => { imp.active_lod = lodIdx; markMetaEdited(true); renderInspector(); },
            },
              h('div', { class: 'ms-lod-head' },
                h('span', {}, `LOD${lodIdx}`),
                h('span', {}, `${Math.round(ratio * 100)}%`),
              ),
              h('div', { class: 'ms-item-sub' }, `~${estTris} tris`),
              lodIdx > 0
                ? h('input', {
                    type: 'range', min: '0.05', max: '0.95', step: '0.05', value: ratio,
                    onclick: (ev) => ev.stopPropagation(),
                    oninput: (ev) => {
                      state.meta.lods[lodIdx] = roundNum(ev.target.value, 2);
                      imp.active_lod = lodIdx;
                      markMetaEdited(true);
                      renderInspector();
                    },
                  })
                : null,
            );
          }),
        ),
      );

      const col = state.meta.collider;
      const colliderSection = h('div', { class: 'ms-section' },
        h('div', { class: 'ms-section-head' },
          h('span', {}, 'Physics Collider Builder'),
          col
            ? h('button', {
                type: 'button',
                class: 'ms-chip',
                onclick: () => { state.meta.collider = null; markMetaEdited(false); renderInspector(); },
              }, 'Clear')
            : null,
        ),
        h('div', { class: 'ms-chip-row' },
          ...['box', 'sphere', 'capsule', 'convex_hull'].map((sh) => h('button', {
            type: 'button',
            class: `ms-chip${col?.shape === sh ? ' on' : ''}`,
            onclick: () => {
              state.meta.collider = fitPhysicsCollider(state.parsed?.fullMesh, sh, state.fixup);
              markMetaEdited(false);
              renderInspector();
            },
          }, sh === 'convex_hull' ? 'Convex Hull' : sh[0].toUpperCase() + sh.slice(1))),
        ),
        col
          ? h('div', { style: 'margin-top:8px' },
              h('div', { class: 'ms-grid-3' },
                ...['X', 'Y', 'Z'].map((ax, idx) => h('label', { class: 'ms-field' },
                  `Center ${ax}`,
                  h('input', {
                    type: 'number', step: '0.05', value: col.center[idx],
                    onchange: (ev) => { col.center[idx] = roundNum(ev.target.value, 4); markMetaEdited(false); },
                  }),
                )),
              ),
              col.shape === 'box'
                ? h('div', { class: 'ms-grid-3', style: 'margin-top:6px' },
                    ...['HX', 'HY', 'HZ'].map((ax, idx) => h('label', { class: 'ms-field' },
                      `${ax} (m)`,
                      h('input', {
                        type: 'number', step: '0.05', min: '0.01', value: col.half_extents[idx],
                        onchange: (ev) => { col.half_extents[idx] = Math.max(0.01, roundNum(ev.target.value, 4)); markMetaEdited(false); },
                      }),
                    )),
                  )
                : col.shape === 'sphere' || col.shape === 'capsule'
                  ? h('div', { class: 'ms-grid-2', style: 'margin-top:6px' },
                      h('label', { class: 'ms-field' }, 'Radius (m)', h('input', {
                        type: 'number', step: '0.05', min: '0.01', value: col.radius,
                        onchange: (ev) => { col.radius = Math.max(0.01, roundNum(ev.target.value, 4)); markMetaEdited(false); },
                      })),
                      col.shape === 'capsule'
                        ? h('label', { class: 'ms-field' }, 'Height (m)', h('input', {
                            type: 'number', step: '0.05', min: '0.02', value: col.height,
                            onchange: (ev) => { col.height = Math.max(0.02, roundNum(ev.target.value, 4)); markMetaEdited(false); },
                          }))
                        : null,
                    )
                  : h('div', { class: 'ms-item-sub', style: 'margin-top:6px' }, `${col.hull_points?.length || 0} extreme hull vertices fitted`),
            )
          : h('div', { class: 'ms-item-sub', style: 'margin-top:6px' }, 'Click a shape above to auto-fit from post-fixup mesh bounds.'),
      );

      inspectorBody.replaceChildren(lodSection, colliderSection);
      return;
    }

    // Tab 3: Named Sockets & Prefab Generation
    const addPresetSocket = (rawName) => {
      const name = sanitizeSocketName(rawName);
      if (!name) return;
      const [bMin, bMax] = state.fixup?.transformedBounds || [[0, 0, 0], [0, 1, 0]];
      const midX = roundNum((bMin[0] + bMax[0]) * 0.5, 4);
      const midY = roundNum((bMin[1] + bMax[1]) * 0.5, 4);
      const midZ = roundNum((bMin[2] + bMax[2]) * 0.5, 4);
      const pos = name === 'top_surface'
        ? [midX, roundNum(bMax[1], 4), midZ]
        : name === 'ground'
          ? [midX, roundNum(bMin[1], 4), midZ]
          : name === 'muzzle'
            ? [midX, midY, roundNum(bMax[2], 4)]
            : [roundNum(bMax[0], 4), midY, midZ];
      state.meta.sockets[name] = { translation: pos, rotation: [0, 0, 0, 1] };
      markMetaEdited(false);
      renderInspector();
    };

    const sockEntries = Object.entries(state.meta.sockets);
    const socketSection = h('div', { class: 'ms-section' },
      h('div', { class: 'ms-section-head' }, h('span', {}, `Attachment Sockets (${sockEntries.length})`)),
      h('div', { class: 'ms-chip-row' },
        ...SOCKET_PRESETS.map((preset) => h('button', {
          type: 'button',
          class: 'ms-chip',
          onclick: () => addPresetSocket(preset),
        }, `+ ${preset}`)),
      ),
      h('div', { class: 'ms-btn-row' },
        h('input', {
          type: 'text',
          placeholder: 'Custom socket name…',
          value: state.newSocketName,
          style: 'flex:1',
          oninput: (ev) => { state.newSocketName = ev.target.value; },
        }),
        h('button', {
          type: 'button',
          class: 'ms-chip',
          onclick: () => {
            if (!state.newSocketName.trim()) return;
            addPresetSocket(state.newSocketName);
            state.newSocketName = '';
          },
        }, 'Add'),
      ),
      h('div', { class: 'ms-socket-list' },
        ...sockEntries.map(([name, sock]) => {
          const euler = quatToEulerDeg(sock.rotation);
          return h('div', { class: 'ms-socket-card' },
            h('div', { class: 'ms-socket-head' },
              h('span', {}, name),
              h('button', {
                type: 'button',
                class: 'ms-chip',
                onclick: () => { delete state.meta.sockets[name]; markMetaEdited(false); renderInspector(); },
              }, '×'),
            ),
            h('div', { class: 'ms-grid-3' },
              ...['X', 'Y', 'Z'].map((ax, idx) => h('label', { class: 'ms-field' },
                `Pos ${ax}`,
                h('input', {
                  type: 'number', step: '0.05', value: sock.translation[idx],
                  onchange: (ev) => { sock.translation[idx] = roundNum(ev.target.value, 4); markMetaEdited(false); },
                }),
              )),
            ),
            h('div', { class: 'ms-grid-3', style: 'margin-top:4px' },
              ...['RX°', 'RY°', 'RZ°'].map((ax, idx) => h('label', { class: 'ms-field' },
                ax,
                h('input', {
                  type: 'number', step: '5', value: euler[idx],
                  onchange: (ev) => {
                    const nextEuler = [...euler];
                    nextEuler[idx] = Number(ev.target.value) || 0;
                    sock.rotation = eulerDegToQuat(nextEuler);
                    markMetaEdited(false);
                  },
                }),
              )),
            ),
          );
        }),
      ),
    );

    const prefabSection = h('div', { class: 'ms-section' },
      h('div', { class: 'ms-section-head' }, h('span', {}, 'Prefab Export')),
      h('div', { class: 'ms-item-sub' }, 'Generates a .prefab.json with Mesh, fitted Collider, and child entities for all named sockets.'),
      h('div', { class: 'ms-btn-row' },
        h('button', {
          type: 'button',
          class: 'ms-chip ms-btn-primary',
          disabled: !state.selectedAssetId,
          onclick: generateModelPrefab,
        }, 'Generate Prefab with Sockets'),
      ),
    );

    inspectorBody.replaceChildren(socketSection, prefabSection);
  }

  // ---- Interactive Orbit / Pan / Zoom + Drag-and-Drop .glb Stage ------------------------------

  canvas.addEventListener('pointerdown', (ev) => {
    canvas.setPointerCapture?.(ev.pointerId);
    let lx = ev.clientX;
    let ly = ev.clientY;
    const panMode = ev.button === 1 || ev.button === 2 || ev.shiftKey;
    const onMove = (me) => {
      const dx = me.clientX - lx;
      const dy = me.clientY - ly;
      lx = me.clientX;
      ly = me.clientY;
      if (panMode) {
        const panScale = state.camera.distance * 0.0018;
        const cyaw = Math.cos(state.camera.yaw), syaw = Math.sin(state.camera.yaw);
        state.camera.target[0] -= dx * cyaw * panScale;
        state.camera.target[2] += dx * syaw * panScale;
        state.camera.target[1] += dy * panScale;
      } else {
        state.camera.yaw -= dx * 0.011;
        state.camera.pitch = clamp(state.camera.pitch + dy * 0.011, -1.35, 1.35);
      }
      requestPaint();
    };
    const onUp = () => {
      canvas.removeEventListener('pointermove', onMove);
      canvas.removeEventListener('pointerup', onUp);
      canvas.removeEventListener('pointercancel', onUp);
    };
    canvas.addEventListener('pointermove', onMove);
    canvas.addEventListener('pointerup', onUp);
    canvas.addEventListener('pointercancel', onUp);
  });

  canvas.addEventListener('contextmenu', (ev) => ev.preventDefault());
  canvas.addEventListener('wheel', (ev) => {
    ev.preventDefault();
    const factor = Math.exp(ev.deltaY * 0.0012);
    state.camera.distance = clamp(state.camera.distance * factor, 0.25, 400);
    requestPaint();
  }, { passive: false });

  viewportWrap.addEventListener('dragover', (ev) => {
    ev.preventDefault();
    if (ev.dataTransfer) ev.dataTransfer.dropEffect = 'copy';
  });
  viewportWrap.addEventListener('drop', async (ev) => {
    ev.preventDefault();
    const file = ev.dataTransfer?.files?.[0];
    if (file && file.name.toLowerCase().endsWith('.glb')) {
      const buf = await file.arrayBuffer();
      state.primitive.name = assetStem(file.name) || 'dropped_model';
      await loadBytesIntoStageAsync(buf, { label: file.name, sourceKind: 'drop' });
    }
  });

  // Subscribe to Studio events when available
  if (typeof studioApi.on === 'function') {
    const offAssets = studioApi.on('assets', () => {
      if (!state.destroyed) renderLeftRail();
    });
    if (typeof offAssets === 'function') unsubs.push(offAssets);
    const offSel = studioApi.on('selection', (sel) => {
      if (!state.destroyed && sel?.assetId && sel.assetId !== state.selectedAssetId) {
        const match = listProjectModels().find((m) => m.id === sel.assetId);
        if (match) inspectProjectAsset(match.id);
      }
    });
    if (typeof offSel === 'function') unsubs.push(offSel);
  }

  // Initial render & initial selection
  renderToolbar();
  renderLeftRail();
  renderInspector();
  renderBudgetBar();
  refreshIntakeCatalog();

  const initialSel = typeof studioApi.getSelection === 'function' ? studioApi.getSelection() : null;
  const firstModel = (initialSel?.assetId && state.models.find((m) => m.id === initialSel.assetId)) || state.models[0];
  if (firstModel) {
    inspectProjectAsset(firstModel.id);
  } else {
    previewCurrentPrimitive();
  }

  function destroy() {
    if (state.destroyed) return;
    state.destroyed = true;
    state.turntableOn = false;
    if (state.turntableRaf && typeof cancelAnimationFrame === 'function') {
      cancelAnimationFrame(state.turntableRaf);
    }
    for (const off of unsubs) {
      try { off(); } catch { /* ignore */ }
    }
    root.remove?.();
  }

  return {
    destroy,
    unmount: destroy,
    refresh() {
      if (state.destroyed) return;
      renderLeftRail();
      requestPaint();
    },
    setDocument(pathOrId) {
      if (state.destroyed || !pathOrId) return;
      inspectProjectAsset(pathOrId);
    },
    onActivate() {
      state.active = true;
      renderLeftRail();
      requestPaint();
    },
    onDeactivate() {
      state.active = false;
      state.turntableOn = false;
    },
    inspectAsset: inspectProjectAsset,
    previewBytes(bytes, label = 'preview.glb') {
      return loadBytesIntoStage(bytes, { label, sourceKind: 'drop' });
    },
    getState() {
      return state;
    },
  };
}

/** Opens Model Studio in a self-contained modal overlay when launched from the Content Browser
 * in environments where a docked workbench tab is not currently active. */
export function openModelStudioModal(studioApi = {}, initialAssetId = null) {
  if (typeof document === 'undefined') return null;
  ensureModelStudioStyles(document);
  document.getElementById('ganbate-model-studio-modal')?.remove();

  const backdrop = document.createElement('div');
  backdrop.id = 'ganbate-model-studio-modal';
  backdrop.className = 'ms-modal-backdrop';

  const shell = document.createElement('div');
  shell.className = 'ms-modal-shell';

  const bar = document.createElement('div');
  bar.className = 'ms-modal-bar';
  const title = document.createElement('strong');
  title.textContent = 'Ganbate 3D Model Studio — LookDev, Fixup & Colliders';
  const closeBtn = document.createElement('button');
  closeBtn.type = 'button';
  closeBtn.textContent = 'Close (Esc)';
  bar.append(title, closeBtn);

  const body = document.createElement('div');
  body.style.flex = '1';
  body.style.minHeight = '0';

  shell.append(bar, body);
  backdrop.appendChild(shell);
  document.body.appendChild(backdrop);

  const instance = mountModelStudio(body, studioApi);
  if (initialAssetId) instance.inspectAsset(initialAssetId);

  const close = () => {
    window.removeEventListener('keydown', onKey);
    instance.destroy();
    backdrop.remove();
  };
  const onKey = (ev) => {
    if (ev.key === 'Escape') close();
  };
  closeBtn.onclick = close;
  backdrop.addEventListener('pointerdown', (ev) => {
    if (ev.target === backdrop) close();
  });
  window.addEventListener('keydown', onKey);
  return { instance, close };
}
