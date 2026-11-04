// Live studio previews and binary metadata extractors for Ganbate Studio's Asset Library.
// Pure binary parsers (`parseGlbPreview`, `extractWavPeaks`) avoid DOM globals so Node tests
// can exercise them directly.

const MAX_PREVIEW_TRIS = 1800;

function mulMat4(a, b) {
  const out = new Float32Array(16);
  for (let col = 0; col < 4; col++) {
    const b0 = b[col * 4];
    const b1 = b[col * 4 + 1];
    const b2 = b[col * 4 + 2];
    const b3 = b[col * 4 + 3];
    for (let row = 0; row < 4; row++) {
      out[col * 4 + row] =
        a[row] * b0 +
        a[4 + row] * b1 +
        a[8 + row] * b2 +
        a[12 + row] * b3;
    }
  }
  return out;
}

function trsMat4(t = [0, 0, 0], q = [0, 0, 0, 1], s = [1, 1, 1]) {
  const [x, y, z, w] = q;
  const x2 = x + x, y2 = y + y, z2 = z + z;
  const xx = x * x2, xy = x * y2, xz = x * z2;
  const yy = y * y2, yz = y * z2, zz = z * z2;
  const wx = w * x2, wy = w * y2, wz = w * z2;
  const [sx, sy, sz] = s;
  return new Float32Array([
    (1 - (yy + zz)) * sx, (xy + wz) * sx, (xz - wy) * sx, 0,
    (xy - wz) * sy, (1 - (xx + zz)) * sy, (yz + wx) * sy, 0,
    (xz + wy) * sz, (yz - wx) * sz, (1 - (xx + yy)) * sz, 0,
    t[0] || 0, t[1] || 0, t[2] || 0, 1,
  ]);
}

function transformVec3(m, x, y, z) {
  return [
    m[0] * x + m[4] * y + m[8] * z + m[12],
    m[1] * x + m[5] * y + m[9] * z + m[13],
    m[2] * x + m[6] * y + m[10] * z + m[14],
  ];
}

/** Parses a binary glTF 2.0 (`.glb`) buffer to extract geometry counts, world bounds, and a
 * sampled triangle set for live 3D canvas thumbnails. Returns `null` if invalid. */
export function parseGlbPreview(input) {
  const bytes = input instanceof Uint8Array
    ? input
    : input instanceof ArrayBuffer
      ? new Uint8Array(input)
      : null;
  if (!bytes || bytes.byteLength < 20) return null;
  const dv = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (dv.getUint32(0, true) !== 0x46546c67 || dv.getUint32(4, true) !== 2) return null;
  const declaredLen = dv.getUint32(8, true);
  if (declaredLen > bytes.byteLength) return null;

  let offset = 12;
  let doc = null;
  let bin = null;
  const decoder = new TextDecoder('utf-8');
  while (offset + 8 <= declaredLen) {
    const chunkLen = dv.getUint32(offset, true);
    const chunkType = dv.getUint32(offset + 4, true);
    offset += 8;
    if (offset + chunkLen > declaredLen) return null;
    if (chunkType === 0x4e4f534a && !doc) {
      try {
        doc = JSON.parse(decoder.decode(bytes.subarray(offset, offset + chunkLen)));
      } catch {
        return null;
      }
    } else if (chunkType === 0x004e4942 && !bin) {
      bin = bytes.subarray(offset, offset + chunkLen);
    }
    offset += chunkLen;
  }
  if (!doc || typeof doc !== 'object') return null;

  const nodes = Array.isArray(doc.nodes) ? doc.nodes : [];
  const meshes = Array.isArray(doc.meshes) ? doc.meshes : [];
  const accessors = Array.isArray(doc.accessors) ? doc.accessors : [];
  const views = Array.isArray(doc.bufferViews) ? doc.bufferViews : [];
  const materials = Array.isArray(doc.materials) ? doc.materials : [];

  const matColors = materials.map((mat) => {
    const f = mat?.pbrMetallicRoughness?.baseColorFactor;
    if (Array.isArray(f) && f.length >= 3) {
      return [
        Math.max(0, Math.min(1, Number(f[0]) || 0)),
        Math.max(0, Math.min(1, Number(f[1]) || 0)),
        Math.max(0, Math.min(1, Number(f[2]) || 0)),
      ];
    }
    return [0.64, 0.73, 0.9];
  });

  // Determine root nodes from the default scene (or parentless nodes).
  let rootIndices = [];
  if (Array.isArray(doc.scenes) && doc.scenes.length) {
    const sc = doc.scenes[doc.scene ?? 0] || doc.scenes[0];
    if (Array.isArray(sc?.nodes)) rootIndices = sc.nodes;
  }
  if (!rootIndices.length && nodes.length) {
    const childSet = new Set();
    for (const n of nodes) {
      if (Array.isArray(n?.children)) for (const c of n.children) childSet.add(c);
    }
    rootIndices = nodes.map((_, i) => i).filter((i) => !childSet.has(i));
  }

  const instances = [];
  const visited = new Set();
  const walkNode = (idx, parentMat) => {
    if (visited.has(idx) || idx < 0 || idx >= nodes.length) return;
    visited.add(idx);
    const node = nodes[idx] || {};
    const local = Array.isArray(node.matrix) && node.matrix.length === 16
      ? new Float32Array(node.matrix)
      : trsMat4(node.translation, node.rotation, node.scale);
    const world = parentMat ? mulMat4(parentMat, local) : local;
    if (typeof node.mesh === 'number' && meshes[node.mesh]) {
      instances.push({ meshIndex: node.mesh, world });
    }
    if (Array.isArray(node.children)) {
      for (const child of node.children) walkNode(child, world);
    }
  };
  for (const r of rootIndices) walkNode(r, null);
  if (!instances.length && meshes.length) {
    const idMat = trsMat4();
    for (let i = 0; i < meshes.length; i++) instances.push({ meshIndex: i, world: idMat });
  }

  const readPositions = (accIdx) => {
    const acc = accessors[accIdx];
    if (!acc || acc.type !== 'VEC3' || acc.componentType !== 5126 || !bin) return null;
    const bv = views[acc.bufferView];
    if (!bv) return null;
    const count = acc.count >>> 0;
    const stride = bv.byteStride || 12;
    const start = (bv.byteOffset || 0) + (acc.byteOffset || 0);
    if (start + (count > 0 ? (count - 1) * stride + 12 : 0) > bin.byteLength) return null;
    const viewDv = new DataView(bin.buffer, bin.byteOffset + start, bin.byteLength - start);
    return {
      count,
      at(i) {
        const o = i * stride;
        return [viewDv.getFloat32(o, true), viewDv.getFloat32(o + 4, true), viewDv.getFloat32(o + 8, true)];
      },
    };
  };

  const readIndices = (accIdx) => {
    const acc = accessors[accIdx];
    if (!acc || acc.type !== 'SCALAR' || !bin) return null;
    const bv = views[acc.bufferView];
    if (!bv) return null;
    const count = acc.count >>> 0;
    const elemSize = acc.componentType === 5121 ? 1 : acc.componentType === 5123 ? 2 : acc.componentType === 5125 ? 4 : 0;
    if (!elemSize) return null;
    const stride = bv.byteStride || elemSize;
    const start = (bv.byteOffset || 0) + (acc.byteOffset || 0);
    if (start + (count > 0 ? (count - 1) * stride + elemSize : 0) > bin.byteLength) return null;
    const viewDv = new DataView(bin.buffer, bin.byteOffset + start, bin.byteLength - start);
    return {
      count,
      at(i) {
        const o = i * stride;
        if (elemSize === 1) return viewDv.getUint8(o);
        if (elemSize === 2) return viewDv.getUint16(o, true);
        return viewDv.getUint32(o, true);
      },
    };
  };

  let totalVerts = 0;
  let totalTris = 0;
  const rawPrims = [];
  for (const inst of instances) {
    const mesh = meshes[inst.meshIndex];
    for (const prim of mesh?.primitives || []) {
      if (prim.mode !== undefined && prim.mode !== 4) continue;
      const posIdx = prim.attributes?.POSITION;
      if (typeof posIdx !== 'number') continue;
      const pos = readPositions(posIdx);
      if (!pos || pos.count < 3) continue;
      const idx = typeof prim.indices === 'number' ? readIndices(prim.indices) : null;
      const triCount = idx ? Math.floor(idx.count / 3) : Math.floor(pos.count / 3);
      if (triCount <= 0) continue;
      totalVerts += pos.count;
      totalTris += triCount;
      const color = typeof prim.material === 'number' && matColors[prim.material]
        ? matColors[prim.material]
        : [0.64, 0.73, 0.9];
      rawPrims.push({ pos, idx, triCount, world: inst.world, color });
    }
  }

  const min = [Infinity, Infinity, Infinity];
  const max = [-Infinity, -Infinity, -Infinity];
  const sampleTris = [];
  const step = Math.max(1, Math.ceil(totalTris / MAX_PREVIEW_TRIS));
  let globalTri = 0;

  for (const p of rawPrims) {
    // Expand bounds across all vertices of this primitive.
    for (let v = 0; v < p.pos.count; v++) {
      const [lx, ly, lz] = p.pos.at(v);
      const [wx, wy, wz] = transformVec3(p.world, lx, ly, lz);
      if (wx < min[0]) min[0] = wx;
      if (wy < min[1]) min[1] = wy;
      if (wz < min[2]) min[2] = wz;
      if (wx > max[0]) max[0] = wx;
      if (wy > max[1]) max[1] = wy;
      if (wz > max[2]) max[2] = wz;
    }
    for (let t = 0; t < p.triCount; t++, globalTri++) {
      if (globalTri % step !== 0) continue;
      const i0 = p.idx ? p.idx.at(t * 3) : t * 3;
      const i1 = p.idx ? p.idx.at(t * 3 + 1) : t * 3 + 1;
      const i2 = p.idx ? p.idx.at(t * 3 + 2) : t * 3 + 2;
      if (i0 >= p.pos.count || i1 >= p.pos.count || i2 >= p.pos.count) continue;
      const a = transformVec3(p.world, ...p.pos.at(i0));
      const b = transformVec3(p.world, ...p.pos.at(i1));
      const c = transformVec3(p.world, ...p.pos.at(i2));
      sampleTris.push([
        a[0], a[1], a[2],
        b[0], b[1], b[2],
        c[0], c[1], c[2],
        p.color[0], p.color[1], p.color[2],
      ]);
    }
  }

  const hasBounds = min[0] <= max[0];
  const bounds = hasBounds ? [min, max] : [[0, 0, 0], [0, 0, 0]];
  const size = hasBounds
    ? [max[0] - min[0], max[1] - min[1], max[2] - min[2]]
    : [0, 0, 0];

  return {
    vertices: totalVerts,
    triangles: totalTris,
    meshes: meshes.length,
    nodes: nodes.length,
    materials: materials.length,
    bounds,
    size,
    sampleTris,
  };
}

/** Extracts duration and normalized amplitude peaks from an uncompressed PCM/float RIFF `.wav`. */
export function extractWavPeaks(input, bucketCount = 44) {
  const bytes = input instanceof Uint8Array
    ? input
    : input instanceof ArrayBuffer
      ? new Uint8Array(input)
      : null;
  if (!bytes || bytes.byteLength < 44) return null;
  const dv = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (dv.getUint32(0, false) !== 0x52494646 || dv.getUint32(8, false) !== 0x57415645) return null;

  let offset = 12;
  let fmt = null;
  let dataStart = 0;
  let dataLen = 0;
  while (offset + 8 <= bytes.byteLength) {
    const id = dv.getUint32(offset, false);
    const sz = dv.getUint32(offset + 4, true);
    const body = offset + 8;
    if (body > bytes.byteLength) break;
    if (id === 0x666d7420 && sz >= 16 && body + 16 <= bytes.byteLength) {
      fmt = {
        audioFormat: dv.getUint16(body, true),
        channels: Math.max(1, dv.getUint16(body + 2, true)),
        sampleRate: Math.max(1, dv.getUint32(body + 4, true)),
        blockAlign: Math.max(1, dv.getUint16(body + 12, true)),
        bitsPerSample: dv.getUint16(body + 14, true),
      };
    } else if (id === 0x64617461) {
      dataStart = body;
      dataLen = Math.min(sz, bytes.byteLength - body);
      break;
    }
    offset = body + sz + (sz & 1);
  }
  if (!fmt || !dataStart) return null;
  const frames = Math.floor(dataLen / fmt.blockAlign);
  const durationSec = frames / fmt.sampleRate;
  const peaks = new Float32Array(bucketCount);
  if (frames <= 0) return { durationSec: 0, sampleRate: fmt.sampleRate, channels: fmt.channels, peaks };

  const bytesPerSample = Math.max(1, fmt.bitsPerSample >> 3);
  const sampleAt = (frame) => {
    const pos = dataStart + frame * fmt.blockAlign;
    if (pos + bytesPerSample > bytes.byteLength) return 0;
    if (fmt.audioFormat === 3 && fmt.bitsPerSample === 32) {
      return Math.abs(dv.getFloat32(pos, true));
    }
    if (fmt.bitsPerSample === 8) return Math.abs((dv.getUint8(pos) - 128) / 128);
    if (fmt.bitsPerSample === 16) return Math.abs(dv.getInt16(pos, true) / 32768);
    if (fmt.bitsPerSample === 24 && pos + 3 <= bytes.byteLength) {
      const v = (bytes[pos] | (bytes[pos + 1] << 8) | (bytes[pos + 2] << 16)) << 8 >> 8;
      return Math.abs(v / 8388608);
    }
    if (fmt.bitsPerSample === 32) return Math.abs(dv.getInt32(pos, true) / 2147483648);
    return 0;
  };

  for (let b = 0; b < bucketCount; b++) {
    const f0 = Math.floor((b * frames) / bucketCount);
    const f1 = Math.max(f0 + 1, Math.floor(((b + 1) * frames) / bucketCount));
    const stride = Math.max(1, Math.floor((f1 - f0) / 32));
    let peak = 0;
    for (let f = f0; f < f1; f += stride) {
      const amp = sampleAt(f);
      if (amp > peak) peak = amp;
    }
    peaks[b] = Math.min(1, peak);
  }
  return { durationSec, sampleRate: fmt.sampleRate, channels: fmt.channels, peaks };
}

// ---- Canvas Renderers -------------------------------------------------------------------------

function prepCanvas(canvas, cssWidth, cssHeight) {
  const dpr = typeof window !== 'undefined' ? Math.min(2, window.devicePixelRatio || 1) : 1;
  const w = Math.max(16, Math.round(cssWidth * dpr));
  const h = Math.max(16, Math.round(cssHeight * dpr));
  if (canvas.width !== w || canvas.height !== h) {
    canvas.width = w;
    canvas.height = h;
  }
  const ctx = canvas.getContext('2d');
  if (ctx) ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  return { ctx, w: cssWidth, h: cssHeight };
}

export function drawGlbPreviewCanvas(canvas, info, { width = 180, height = 100, yaw = 0.68, pitch = 0.42 } = {}) {
  const { ctx, w, h } = prepCanvas(canvas, width, height);
  if (!ctx) return;

  const bg = ctx.createLinearGradient(0, 0, 0, h);
  bg.addColorStop(0, '#1e2430');
  bg.addColorStop(1, '#12151b');
  ctx.fillStyle = bg;
  ctx.fillRect(0, 0, w, h);

  // Subtle floor shadow disc
  ctx.save();
  ctx.fillStyle = 'rgba(0, 0, 0, 0.34)';
  ctx.beginPath();
  ctx.ellipse(w * 0.5, h * 0.78, w * 0.28, h * 0.09, 0, 0, Math.PI * 2);
  ctx.fill();
  ctx.restore();

  if (!info || !info.sampleTris?.length) return;

  const [min, max] = info.bounds;
  const cx = (min[0] + max[0]) * 0.5;
  const cy = (min[1] + max[1]) * 0.5;
  const cz = (min[2] + max[2]) * 0.5;
  const span = Math.max(max[0] - min[0], max[1] - min[1], max[2] - min[2], 1e-4);

  const cyaw = Math.cos(yaw), syaw = Math.sin(yaw);
  const cp = Math.cos(pitch), sp = Math.sin(pitch);
  const right = [cyaw, 0, -syaw];
  const up = [syaw * sp, cp, cyaw * sp];
  const fwd = [syaw * cp, -sp, cyaw * cp]; // from target toward eye

  const camView = (vx, vy, vz) => {
    const dx = vx - cx, dy = vy - cy, dz = vz - cz;
    return [
      dx * right[0] + dy * right[1] + dz * right[2],
      dx * up[0] + dy * up[1] + dz * up[2],
      dx * fwd[0] + dy * fwd[1] + dz * fwd[2],
    ];
  };

  // Fit projected camera-space bounds nicely inside the canvas.
  let xLo = Infinity, xHi = -Infinity, yLo = Infinity, yHi = -Infinity;
  for (let mask = 0; mask < 8; mask++) {
    const bx = mask & 1 ? max[0] : min[0];
    const by = mask & 2 ? max[1] : min[1];
    const bz = mask & 4 ? max[2] : min[2];
    const [px, py] = camView(bx, by, bz);
    if (px < xLo) xLo = px;
    if (px > xHi) xHi = px;
    if (py < yLo) yLo = py;
    if (py > yHi) yHi = py;
  }
  const fitScale = Math.min(
    (w * 0.76) / Math.max(xHi - xLo, span * 0.25),
    (h * 0.76) / Math.max(yHi - yLo, span * 0.25),
  );
  const midX = (xLo + xHi) * 0.5;
  const midY = (yLo + yHi) * 0.5;

  const light = [0.46, 0.76, 0.45]; // key light in world space
  const projected = [];
  for (const tri of info.sampleTris) {
    const va = camView(tri[0], tri[1], tri[2]);
    const vb = camView(tri[3], tri[4], tri[5]);
    const vc = camView(tri[6], tri[7], tri[8]);

    // World normal for hemisphere + sun lighting.
    const ux = tri[3] - tri[0], uy = tri[4] - tri[1], uz = tri[5] - tri[2];
    const vx = tri[6] - tri[0], vy = tri[7] - tri[1], vz = tri[8] - tri[2];
    let nx = uy * vz - uz * vy;
    let ny = uz * vx - ux * vz;
    let nz = ux * vy - uy * vx;
    const nLen = Math.hypot(nx, ny, nz);
    if (nLen < 1e-9) continue;
    nx /= nLen; ny /= nLen; nz /= nLen;

    // Flip normal toward camera if back-facing so thin double-sided cards still shade cleanly.
    const viewDot = nx * fwd[0] + ny * fwd[1] + nz * fwd[2];
    if (viewDot < 0) { nx = -nx; ny = -ny; nz = -nz; }

    const ndl = Math.max(0, nx * light[0] + ny * light[1] + nz * light[2]);
    const hemi = 0.34 + 0.16 * (ny * 0.5 + 0.5);
    const rim = Math.pow(1 - Math.min(1, Math.abs(viewDot)), 2.2) * 0.14;
    const shade = Math.min(1.18, hemi + ndl * 0.62 + rim);

    const r = Math.min(255, Math.round(Math.pow(tri[9], 0.85) * shade * 235 + 12));
    const g = Math.min(255, Math.round(Math.pow(tri[10], 0.85) * shade * 238 + 14));
    const b = Math.min(255, Math.round(Math.pow(tri[11], 0.85) * shade * 245 + 18));

    const sx0 = w * 0.5 + (va[0] - midX) * fitScale;
    const sy0 = h * 0.52 - (va[1] - midY) * fitScale;
    const sx1 = w * 0.5 + (vb[0] - midX) * fitScale;
    const sy1 = h * 0.52 - (vb[1] - midY) * fitScale;
    const sx2 = w * 0.5 + (vc[0] - midX) * fitScale;
    const sy2 = h * 0.52 - (vc[1] - midY) * fitScale;
    const depth = (va[2] + vb[2] + vc[2]) / 3;
    projected.push({ sx0, sy0, sx1, sy1, sx2, sy2, depth, fill: `rgb(${r},${g},${b})` });
  }

  projected.sort((a, b) => a.depth - b.depth);
  ctx.lineJoin = 'round';
  ctx.lineWidth = 0.55;
  for (const p of projected) {
    ctx.beginPath();
    ctx.moveTo(p.sx0, p.sy0);
    ctx.lineTo(p.sx1, p.sy1);
    ctx.lineTo(p.sx2, p.sy2);
    ctx.closePath();
    ctx.fillStyle = p.fill;
    ctx.fill();
    ctx.strokeStyle = p.fill;
    ctx.stroke();
  }
}

function rgbCss(arr, fallback) {
  if (!Array.isArray(arr) || arr.length < 3) return fallback;
  const to255 = (v) => Math.max(0, Math.min(255, Math.round(Math.pow(Math.max(0, Number(v) || 0), 1 / 2.2) * 255)));
  return `rgb(${to255(arr[0])},${to255(arr[1])},${to255(arr[2])})`;
}

export function drawLookPreviewCanvas(canvas, look = {}, { width = 180, height = 100 } = {}) {
  const { ctx, w, h } = prepCanvas(canvas, width, height);
  if (!ctx) return;
  const hours = Number(look?.time?.hours ?? 16.5);
  const dayFactor = Math.max(0, Math.sin(((hours - 6) / 12) * Math.PI));
  const preset = String(look?.weather?.preset || 'clear');
  const overcast = preset === 'overcast' || preset === 'rain' || preset === 'snow';

  const defaultZenith = dayFactor > 0.25
    ? (overcast ? [0.22, 0.26, 0.32] : [0.09, 0.24, 0.58])
    : [0.02, 0.03, 0.09];
  const defaultHorizon = dayFactor > 0.05 && dayFactor < 0.42
    ? [0.78, 0.36, 0.16]
    : dayFactor >= 0.42
      ? (overcast ? [0.42, 0.46, 0.52] : [0.42, 0.62, 0.86])
      : [0.08, 0.1, 0.2];

  const zenith = rgbCss(look?.sky?.zenith_color || look?.sky?.zenith, rgbCss(defaultZenith, '#1c3f7a'));
  const horizon = rgbCss(look?.sky?.horizon_color || look?.sky?.horizon, rgbCss(defaultHorizon, '#7fa8d4'));
  const ground = rgbCss(look?.terrain?.base_color || look?.grass?.tip_color, dayFactor > 0.2 ? '#293528' : '#151922');

  const horizonY = h * 0.66;
  const skyGrad = ctx.createLinearGradient(0, 0, 0, horizonY);
  skyGrad.addColorStop(0, zenith);
  skyGrad.addColorStop(1, horizon);
  ctx.fillStyle = skyGrad;
  ctx.fillRect(0, 0, w, horizonY + 1);

  // Sun / moon glow
  const sunX = w * (0.2 + 0.6 * ((hours % 24) / 24));
  const sunY = horizonY - (dayFactor * h * 0.42 + 4);
  if (!overcast) {
    const glow = ctx.createRadialGradient(sunX, sunY, 1, sunX, sunY, w * 0.22);
    glow.addColorStop(0, dayFactor > 0.08 ? 'rgba(255, 236, 175, 0.95)' : 'rgba(195, 214, 255, 0.75)');
    glow.addColorStop(0.3, dayFactor > 0.08 ? 'rgba(255, 190, 110, 0.32)' : 'rgba(140, 170, 235, 0.2)');
    glow.addColorStop(1, 'rgba(255, 190, 110, 0)');
    ctx.fillStyle = glow;
    ctx.fillRect(0, 0, w, horizonY + 8);
  }

  // Silhouette hills + ground
  const gndGrad = ctx.createLinearGradient(0, horizonY, 0, h);
  gndGrad.addColorStop(0, ground);
  gndGrad.addColorStop(1, '#111419');
  ctx.fillStyle = gndGrad;
  ctx.beginPath();
  ctx.moveTo(0, horizonY);
  ctx.quadraticCurveTo(w * 0.28, horizonY - 7, w * 0.56, horizonY - 1);
  ctx.quadraticCurveTo(w * 0.82, horizonY + 4, w, horizonY - 4);
  ctx.lineTo(w, h);
  ctx.lineTo(0, h);
  ctx.closePath();
  ctx.fill();
}

export function drawMaterialPreviewCanvas(canvas, mat = {}, { width = 180, height = 100 } = {}) {
  const { ctx, w, h } = prepCanvas(canvas, width, height);
  if (!ctx) return;

  ctx.fillStyle = '#15181e';
  ctx.fillRect(0, 0, w, h);

  const rawColor = mat?.base_color || mat?.baseColor || mat?.color || [0.76, 0.46, 0.52];
  const r0 = Math.max(0.06, Math.min(1, Number(rawColor[0]) || 0.72));
  const g0 = Math.max(0.06, Math.min(1, Number(rawColor[1]) || 0.48));
  const b0 = Math.max(0.06, Math.min(1, Number(rawColor[2]) || 0.54));
  const roughness = Math.max(0.05, Math.min(1, Number(mat?.roughness ?? 0.42)));
  const metallic = Math.max(0, Math.min(1, Number(mat?.metallic ?? 0.1)));

  const cx = w * 0.5;
  const cy = h * 0.52;
  const radius = Math.min(w, h) * 0.34;

  ctx.fillStyle = 'rgba(0,0,0,0.42)';
  ctx.beginPath();
  ctx.ellipse(cx + 2, cy + radius * 0.92, radius * 0.85, radius * 0.24, 0, 0, Math.PI * 2);
  ctx.fill();

  const toRgb = (m) => `rgb(${Math.min(255, Math.round(r0 * m * 255))},${Math.min(255, Math.round(g0 * m * 255))},${Math.min(255, Math.round(b0 * m * 255))})`;
  const sphere = ctx.createRadialGradient(
    cx - radius * 0.34,
    cy - radius * 0.36,
    radius * 0.06,
    cx,
    cy,
    radius,
  );
  const specBoost = (1 - roughness * 0.7) * (0.55 + metallic * 0.45);
  sphere.addColorStop(0, metallic > 0.5 ? toRgb(1.35) : `rgba(255,255,255,${(0.35 + specBoost * 0.55).toFixed(2)})`);
  sphere.addColorStop(0.18 + roughness * 0.22, toRgb(1.05));
  sphere.addColorStop(0.78, toRgb(0.56 - metallic * 0.18));
  sphere.addColorStop(1, toRgb(0.24));

  ctx.beginPath();
  ctx.arc(cx, cy, radius, 0, Math.PI * 2);
  ctx.fillStyle = sphere;
  ctx.fill();
}

export function drawWaveformCanvas(canvas, wave, { width = 180, height = 100, playing = false } = {}) {
  const { ctx, w, h } = prepCanvas(canvas, width, height);
  if (!ctx) return;

  ctx.fillStyle = '#14171d';
  ctx.fillRect(0, 0, w, h);

  ctx.strokeStyle = 'rgba(127, 209, 127, 0.16)';
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.moveTo(8, h * 0.5);
  ctx.lineTo(w - 8, h * 0.5);
  ctx.stroke();

  const peaks = wave?.peaks;
  if (!peaks || !peaks.length) return;

  let maxPeak = 0.02;
  for (let i = 0; i < peaks.length; i++) if (peaks[i] > maxPeak) maxPeak = peaks[i];

  const padX = 10;
  const usableW = Math.max(20, w - padX * 2);
  const step = usableW / peaks.length;
  const barW = Math.max(1.5, step * 0.64);
  ctx.fillStyle = playing ? '#a8f0a8' : '#7fd17f';
  for (let i = 0; i < peaks.length; i++) {
    const norm = Math.max(0.08, peaks[i] / maxPeak);
    const barH = norm * (h * 0.64);
    const x = padX + i * step;
    const y = (h - barH) * 0.5;
    ctx.fillRect(x, y, barW, barH);
  }
}

// ---- Cached Async Preview Loader --------------------------------------------------------------

export function createPreviewManager({ urlForAsset, onMetadata }) {
  const cache = new Map();
  const inflight = new Map();

  function keyOf(asset) {
    return `${asset.id}:${asset.bytes ?? 0}:${asset.path}`;
  }

  function getMeta(asset) {
    return cache.get(keyOf(asset)) || null;
  }

  async function loadEntry(asset) {
    const key = keyOf(asset);
    if (cache.has(key)) return cache.get(key);
    if (inflight.has(key)) return inflight.get(key);

    const task = (async () => {
      try {
        const res = await fetch(urlForAsset(asset.path));
        if (!res.ok) return null;
        if (asset.kind === 'model') {
          const buf = await res.arrayBuffer();
          const parsed = parseGlbPreview(buf);
          if (parsed) {
            cache.set(key, parsed);
            onMetadata?.(asset.id, parsed);
          }
          return parsed;
        }
        if (asset.kind === 'look' || asset.kind === 'material') {
          const json = await res.json();
          const entry = { json };
          cache.set(key, entry);
          onMetadata?.(asset.id, entry);
          return entry;
        }
        if (asset.kind === 'sound') {
          const buf = await res.arrayBuffer();
          let wave = extractWavPeaks(buf, 44);
          if (!wave && typeof window !== 'undefined' && (window.AudioContext || window.webkitAudioContext)) {
            const Ctx = window.AudioContext || window.webkitAudioContext;
            const actx = new Ctx();
            try {
              const decoded = await actx.decodeAudioData(buf.slice(0));
              const chan = decoded.getChannelData(0);
              const buckets = 44;
              const peaks = new Float32Array(buckets);
              for (let b = 0; b < buckets; b++) {
                const i0 = Math.floor((b * chan.length) / buckets);
                const i1 = Math.max(i0 + 1, Math.floor(((b + 1) * chan.length) / buckets));
                const stride = Math.max(1, Math.floor((i1 - i0) / 32));
                let pk = 0;
                for (let i = i0; i < i1; i += stride) {
                  const v = Math.abs(chan[i]);
                  if (v > pk) pk = v;
                }
                peaks[b] = Math.min(1, pk);
              }
              wave = { durationSec: decoded.duration, sampleRate: decoded.sampleRate, channels: decoded.numberOfChannels, peaks };
            } catch {
              wave = null;
            } finally {
              actx.close?.().catch(() => {});
            }
          }
          if (wave) {
            cache.set(key, wave);
            onMetadata?.(asset.id, wave);
          }
          return wave;
        }
      } catch {
        return null;
      } finally {
        inflight.delete(key);
      }
      return null;
    })();
    inflight.set(key, task);
    return task;
  }

  function recordTextureDimensions(asset, width, height) {
    const key = keyOf(asset);
    const prev = cache.get(key);
    if (prev && prev.width === width && prev.height === height) return;
    const meta = { width, height };
    cache.set(key, meta);
    onMetadata?.(asset.id, meta);
  }

  return { getMeta, loadEntry, recordTextureDimensions };
}
