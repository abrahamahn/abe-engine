// Live studio previews and binary metadata extractors for Ganbate Studio's Asset Library.
// Pure binary parsers (`parseGlbPreview`, `extractWavPeaks`) avoid DOM globals so Node tests
// can exercise them directly.

const MAX_PREVIEW_TRIS = 1800;
const MAX_FULL_MESH_TRIS = 250000;

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

function normalMat3(m) {
  const a00 = m[0], a01 = m[4], a02 = m[8];
  const a10 = m[1], a11 = m[5], a12 = m[9];
  const a20 = m[2], a21 = m[6], a22 = m[10];
  const b01 = a22 * a11 - a12 * a21;
  const b11 = -a22 * a10 + a12 * a20;
  const b21 = a21 * a10 - a11 * a20;
  const det = a00 * b01 + a01 * b11 + a02 * b21;
  if (Math.abs(det) < 1e-12) return [1, 0, 0, 0, 1, 0, 0, 0, 1];
  const inv = 1 / det;
  // Inverse-transpose in column-major 3x3
  return [
    b01 * inv,
    b11 * inv,
    b21 * inv,
    (-a22 * a01 + a02 * a21) * inv,
    (a22 * a00 - a02 * a20) * inv,
    (-a21 * a00 + a01 * a20) * inv,
    (a12 * a01 - a02 * a11) * inv,
    (-a12 * a00 + a02 * a10) * inv,
    (a11 * a00 - a01 * a10) * inv,
  ];
}

function transformDirMat3(m3, x, y, z) {
  const nx = m3[0] * x + m3[3] * y + m3[6] * z;
  const ny = m3[1] * x + m3[4] * y + m3[7] * z;
  const nz = m3[2] * x + m3[5] * y + m3[8] * z;
  const len = Math.hypot(nx, ny, nz);
  return len > 1e-9 ? [nx / len, ny / len, nz / len] : [0, 1, 0];
}

function transformLinearDir(m4, x, y, z) {
  const tx = m4[0] * x + m4[4] * y + m4[8] * z;
  const ty = m4[1] * x + m4[5] * y + m4[9] * z;
  const tz = m4[2] * x + m4[6] * y + m4[10] * z;
  const len = Math.hypot(tx, ty, tz);
  return len > 1e-9 ? [tx / len, ty / len, tz / len] : [1, 0, 0];
}

function parseEmbeddedImageDims(slice) {
  if (!slice || slice.byteLength < 24) return null;
  const dv = new DataView(slice.buffer, slice.byteOffset, slice.byteLength);
  // PNG: 89 50 4E 47 0D 0A 1A 0A + IHDR
  if (dv.getUint32(0, false) === 0x89504e47 && dv.getUint32(12, false) === 0x49484452) {
    const w = dv.getUint32(16, false);
    const h = dv.getUint32(20, false);
    if (w > 0 && h > 0) return { width: w, height: h };
  }
  // JPEG: FF D8 ... SOF0/SOF1/SOF2
  if (slice[0] === 0xff && slice[1] === 0xd8) {
    let off = 2;
    while (off + 9 < slice.byteLength) {
      if (slice[off] !== 0xff) { off++; continue; }
      const marker = slice[off + 1];
      if (marker === 0xff) { off++; continue; }
      if (marker === 0xd8 || marker === 0xd9) { off += 2; continue; }
      const segLen = dv.getUint16(off + 2, false);
      if (segLen < 2) break;
      if (marker === 0xc0 || marker === 0xc1 || marker === 0xc2) {
        const h = dv.getUint16(off + 5, false);
        const w = dv.getUint16(off + 7, false);
        if (w > 0 && h > 0) return { width: w, height: h };
      }
      off += 2 + segLen;
    }
  }
  // WebP: RIFF....WEBP
  if (dv.getUint32(0, false) === 0x52494646 && dv.getUint32(8, false) === 0x57454250 && slice.byteLength >= 30) {
    const chunk = dv.getUint32(12, false);
    if (chunk === 0x56503858) { // VP8X
      const w = 1 + (slice[24] | (slice[25] << 8) | (slice[26] << 16));
      const h = 1 + (slice[27] | (slice[28] << 8) | (slice[29] << 16));
      return { width: w, height: h };
    }
    if (chunk === 0x5650384c && slice.byteLength >= 25) { // VP8L
      const bits = dv.getUint32(21, true);
      return { width: (bits & 0x3fff) + 1, height: ((bits >> 14) & 0x3fff) + 1 };
    }
  }
  return null;
}

/** Parses a binary glTF 2.0 (`.glb`) buffer to extract geometry counts, world bounds, a sampled
 * triangle set for live 3D canvas thumbnails, and full mesh attributes / budget metrics for
 * Model Studio LookDev. Returns `null` if invalid. */
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
  const skins = Array.isArray(doc.skins) ? doc.skins : [];
  const textures = Array.isArray(doc.textures) ? doc.textures : [];
  const images = Array.isArray(doc.images) ? doc.images : [];

  const jointSet = new Set();
  for (const sk of skins) {
    if (Array.isArray(sk?.joints)) {
      for (const j of sk.joints) if (typeof j === 'number') jointSet.add(j);
    }
  }
  const bones = jointSet.size;

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
  const parentByNode = new Array(nodes.length).fill(null);
  for (let ni = 0; ni < nodes.length; ni++) {
    const kids = nodes[ni]?.children;
    if (Array.isArray(kids)) {
      for (const c of kids) if (typeof c === 'number' && c >= 0 && c < nodes.length) parentByNode[c] = ni;
    }
  }
  if (!rootIndices.length && nodes.length) {
    rootIndices = nodes.map((_, i) => i).filter((i) => parentByNode[i] === null);
  }

  const nodeHierarchy = nodes.map((n, index) => ({
    index,
    name: typeof n?.name === 'string' && n.name ? n.name : `Node_${index}`,
    parent: parentByNode[index],
    mesh: typeof n?.mesh === 'number' ? n.mesh : null,
    translation: Array.isArray(n?.translation) ? n.translation.slice(0, 3) : [0, 0, 0],
    rotation: Array.isArray(n?.rotation) ? n.rotation.slice(0, 4) : [0, 0, 0, 1],
    scale: Array.isArray(n?.scale) ? n.scale.slice(0, 3) : [1, 1, 1],
  }));

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
      instances.push({ meshIndex: node.mesh, world, nMat: normalMat3(world) });
    }
    if (Array.isArray(node.children)) {
      for (const child of node.children) walkNode(child, world);
    }
  };
  for (const r of rootIndices) walkNode(r, null);
  if (!instances.length && meshes.length) {
    const idMat = trsMat4();
    const idN = [1, 0, 0, 0, 1, 0, 0, 0, 1];
    for (let i = 0; i < meshes.length; i++) instances.push({ meshIndex: i, world: idMat, nMat: idN });
  }

  const readFloatAccessor = (accIdx, arity) => {
    if (typeof accIdx !== 'number') return null;
    const acc = accessors[accIdx];
    const expectedType = arity === 2 ? 'VEC2' : arity === 3 ? 'VEC3' : 'VEC4';
    if (!acc || acc.type !== expectedType || !bin) return null;
    const bv = views[acc.bufferView];
    if (!bv) return null;
    const count = acc.count >>> 0;
    const ct = acc.componentType;
    const compBytes = ct === 5126 ? 4 : (ct === 5123 || ct === 5122) ? 2 : (ct === 5121 || ct === 5120) ? 1 : 0;
    if (!compBytes) return null;
    const elemBytes = compBytes * arity;
    const stride = bv.byteStride || elemBytes;
    const start = (bv.byteOffset || 0) + (acc.byteOffset || 0);
    if (start + (count > 0 ? (count - 1) * stride + elemBytes : 0) > bin.byteLength) return null;
    const viewDv = new DataView(bin.buffer, bin.byteOffset + start, bin.byteLength - start);
    const readComp = (byteOff) => {
      if (ct === 5126) return viewDv.getFloat32(byteOff, true);
      if (ct === 5121) return viewDv.getUint8(byteOff) / 255;
      if (ct === 5123) return viewDv.getUint16(byteOff, true) / 65535;
      if (ct === 5120) return Math.max(-1, viewDv.getInt8(byteOff) / 127);
      return Math.max(-1, viewDv.getInt16(byteOff, true) / 32767);
    };
    return {
      count,
      at(i) {
        const o = i * stride;
        if (arity === 2) return [readComp(o), readComp(o + compBytes)];
        if (arity === 3) return [readComp(o), readComp(o + compBytes), readComp(o + compBytes * 2)];
        return [readComp(o), readComp(o + compBytes), readComp(o + compBytes * 2), readComp(o + compBytes * 3)];
      },
    };
  };

  const readPositions = (accIdx) => {
    const acc = accessors[accIdx];
    if (!acc || acc.componentType !== 5126) return null;
    return readFloatAccessor(accIdx, 3);
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
  let hasNormals = false;
  let hasTangents = false;
  let hasUvs = false;
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
      const norm = readFloatAccessor(prim.attributes?.NORMAL, 3);
      const tan = readFloatAccessor(prim.attributes?.TANGENT, 4);
      const uv = readFloatAccessor(prim.attributes?.TEXCOORD_0, 2);
      const vcol = readFloatAccessor(prim.attributes?.COLOR_0, 4) || readFloatAccessor(prim.attributes?.COLOR_0, 3);
      if (norm && norm.count === pos.count) hasNormals = true;
      if (tan && tan.count === pos.count) hasTangents = true;
      if (uv && uv.count === pos.count) hasUvs = true;
      totalVerts += pos.count;
      totalTris += triCount;
      const color = typeof prim.material === 'number' && matColors[prim.material]
        ? matColors[prim.material]
        : [0.64, 0.73, 0.9];
      rawPrims.push({
        pos,
        norm: norm && norm.count === pos.count ? norm : null,
        tan: tan && tan.count === pos.count ? tan : null,
        uv: uv && uv.count === pos.count ? uv : null,
        vcol: vcol && vcol.count === pos.count ? vcol : null,
        idx,
        triCount,
        world: inst.world,
        nMat: inst.nMat,
        color,
      });
    }
  }

  const min = [Infinity, Infinity, Infinity];
  const max = [-Infinity, -Infinity, -Infinity];
  const sampleTris = [];
  const step = Math.max(1, Math.ceil(totalTris / MAX_PREVIEW_TRIS));
  let globalTri = 0;

  const buildFull = totalTris > 0 && totalTris <= MAX_FULL_MESH_TRIS;
  const fullPos = buildFull ? new Float32Array(totalVerts * 3) : new Float32Array(0);
  const fullNorm = buildFull ? new Float32Array(totalVerts * 3) : new Float32Array(0);
  const fullTan = buildFull ? new Float32Array(totalVerts * 4) : new Float32Array(0);
  const fullUv = buildFull ? new Float32Array(totalVerts * 2) : new Float32Array(0);
  const fullCol = buildFull ? new Float32Array(totalVerts * 3) : new Float32Array(0);
  const fullIdx = buildFull ? new Uint32Array(totalTris * 3) : new Uint32Array(0);
  let vertBase = 0;
  let idxWrite = 0;

  for (const p of rawPrims) {
    for (let v = 0; v < p.pos.count; v++) {
      const [lx, ly, lz] = p.pos.at(v);
      const [wx, wy, wz] = transformVec3(p.world, lx, ly, lz);
      if (wx < min[0]) min[0] = wx;
      if (wy < min[1]) min[1] = wy;
      if (wz < min[2]) min[2] = wz;
      if (wx > max[0]) max[0] = wx;
      if (wy > max[1]) max[1] = wy;
      if (wz > max[2]) max[2] = wz;

      if (buildFull) {
        const gv = vertBase + v;
        fullPos[gv * 3] = wx;
        fullPos[gv * 3 + 1] = wy;
        fullPos[gv * 3 + 2] = wz;
        if (p.norm) {
          const [nx, ny, nz] = p.norm.at(v);
          const wn = transformDirMat3(p.nMat, nx, ny, nz);
          fullNorm[gv * 3] = wn[0];
          fullNorm[gv * 3 + 1] = wn[1];
          fullNorm[gv * 3 + 2] = wn[2];
        }
        if (p.uv) {
          const [u, tv] = p.uv.at(v);
          fullUv[gv * 2] = u;
          fullUv[gv * 2 + 1] = tv;
        } else {
          // Triplanar-friendly fallback UV in world meters for UV-checker inspection
          fullUv[gv * 2] = wx * 0.5 + wz * 0.5;
          fullUv[gv * 2 + 1] = wy * 0.5;
        }
        if (p.tan) {
          const [tx, ty, tz, tw] = p.tan.at(v);
          const wt = transformLinearDir(p.world, tx, ty, tz);
          fullTan[gv * 4] = wt[0];
          fullTan[gv * 4 + 1] = wt[1];
          fullTan[gv * 4 + 2] = wt[2];
          fullTan[gv * 4 + 3] = tw < 0 ? -1 : 1;
        }
        if (p.vcol) {
          const vc = p.vcol.at(v);
          fullCol[gv * 3] = p.color[0] * vc[0];
          fullCol[gv * 3 + 1] = p.color[1] * vc[1];
          fullCol[gv * 3 + 2] = p.color[2] * vc[2];
        } else {
          fullCol[gv * 3] = p.color[0];
          fullCol[gv * 3 + 1] = p.color[1];
          fullCol[gv * 3 + 2] = p.color[2];
        }
      }
    }

    const needSynthNorm = buildFull && !p.norm;
    const needSynthTan = buildFull && !p.tan;
    for (let t = 0; t < p.triCount; t++, globalTri++) {
      const i0 = p.idx ? p.idx.at(t * 3) : t * 3;
      const i1 = p.idx ? p.idx.at(t * 3 + 1) : t * 3 + 1;
      const i2 = p.idx ? p.idx.at(t * 3 + 2) : t * 3 + 2;
      if (i0 >= p.pos.count || i1 >= p.pos.count || i2 >= p.pos.count) continue;

      if (buildFull) {
        const g0 = vertBase + i0;
        const g1 = vertBase + i1;
        const g2 = vertBase + i2;
        fullIdx[idxWrite++] = g0;
        fullIdx[idxWrite++] = g1;
        fullIdx[idxWrite++] = g2;

        if (needSynthNorm || needSynthTan) {
          const ax = fullPos[g0 * 3], ay = fullPos[g0 * 3 + 1], az = fullPos[g0 * 3 + 2];
          const bx = fullPos[g1 * 3], by = fullPos[g1 * 3 + 1], bz = fullPos[g1 * 3 + 2];
          const cx = fullPos[g2 * 3], cy = fullPos[g2 * 3 + 1], cz = fullPos[g2 * 3 + 2];
          const e1x = bx - ax, e1y = by - ay, e1z = bz - az;
          const e2x = cx - ax, e2y = cy - ay, e2z = cz - az;
          if (needSynthNorm) {
            const fnx = e1y * e2z - e1z * e2y;
            const fny = e1z * e2x - e1x * e2z;
            const fnz = e1x * e2y - e1y * e2x;
            for (const gv of [g0, g1, g2]) {
              fullNorm[gv * 3] += fnx;
              fullNorm[gv * 3 + 1] += fny;
              fullNorm[gv * 3 + 2] += fnz;
            }
          }
          if (needSynthTan) {
            const du1 = fullUv[g1 * 2] - fullUv[g0 * 2];
            const dv1 = fullUv[g1 * 2 + 1] - fullUv[g0 * 2 + 1];
            const du2 = fullUv[g2 * 2] - fullUv[g0 * 2];
            const dv2 = fullUv[g2 * 2 + 1] - fullUv[g0 * 2 + 1];
            const denom = du1 * dv2 - du2 * dv1;
            let tx = 1, ty = 0, tz = 0;
            if (Math.abs(denom) > 1e-8) {
              const r = 1 / denom;
              tx = (e1x * dv2 - e2x * dv1) * r;
              ty = (e1y * dv2 - e2y * dv1) * r;
              tz = (e1z * dv2 - e2z * dv1) * r;
            } else {
              tx = e1x; ty = e1y; tz = e1z;
            }
            for (const gv of [g0, g1, g2]) {
              fullTan[gv * 4] += tx;
              fullTan[gv * 4 + 1] += ty;
              fullTan[gv * 4 + 2] += tz;
              fullTan[gv * 4 + 3] = 1;
            }
          }
        }
      }

      if (globalTri % step !== 0) continue;
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

    if (buildFull) {
      if (needSynthNorm) {
        for (let v = 0; v < p.pos.count; v++) {
          const gv = vertBase + v;
          const l = Math.hypot(fullNorm[gv * 3], fullNorm[gv * 3 + 1], fullNorm[gv * 3 + 2]);
          if (l > 1e-9) {
            fullNorm[gv * 3] /= l;
            fullNorm[gv * 3 + 1] /= l;
            fullNorm[gv * 3 + 2] /= l;
          } else {
            fullNorm[gv * 3 + 1] = 1;
          }
        }
      }
      if (needSynthTan) {
        for (let v = 0; v < p.pos.count; v++) {
          const gv = vertBase + v;
          const nx = fullNorm[gv * 3], ny = fullNorm[gv * 3 + 1], nz = fullNorm[gv * 3 + 2];
          let tx = fullTan[gv * 4], ty = fullTan[gv * 4 + 1], tz = fullTan[gv * 4 + 2];
          // Gram-Schmidt orthogonalize tangent against normal
          const dot = tx * nx + ty * ny + tz * nz;
          tx -= nx * dot;
          ty -= ny * dot;
          tz -= nz * dot;
          const l = Math.hypot(tx, ty, tz);
          if (l > 1e-9) {
            fullTan[gv * 4] = tx / l;
            fullTan[gv * 4 + 1] = ty / l;
            fullTan[gv * 4 + 2] = tz / l;
          } else if (Math.abs(ny) < 0.9) {
            const cl = Math.hypot(nz, -nx);
            fullTan[gv * 4] = nz / cl;
            fullTan[gv * 4 + 1] = 0;
            fullTan[gv * 4 + 2] = -nx / cl;
          } else {
            fullTan[gv * 4] = 1;
            fullTan[gv * 4 + 1] = 0;
            fullTan[gv * 4 + 2] = 0;
          }
          fullTan[gv * 4 + 3] = 1;
        }
      }
      vertBase += p.pos.count;
    }
  }

  let textureVramBytes = 0;
  for (const img of images) {
    if (bin && typeof img?.bufferView === 'number' && views[img.bufferView]) {
      const bv = views[img.bufferView];
      const start = bv.byteOffset || 0;
      const len = bv.byteLength || 0;
      if (start + len <= bin.byteLength && len > 0) {
        const dims = parseEmbeddedImageDims(bin.subarray(start, start + len));
        if (dims) {
          textureVramBytes += Math.round(dims.width * dims.height * 4 * (4 / 3));
          continue;
        }
        textureVramBytes += Math.max(4096, len * 4);
      }
    } else {
      textureVramBytes += 256 * 256 * 4;
    }
  }
  const geomVramBytes = totalVerts * 48 + totalTris * 12;
  const estimatedVramBytes = geomVramBytes + textureVramBytes;

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
    bones,
    textures: textures.length,
    estimatedVramBytes,
    hasNormals,
    hasTangents,
    hasUvs,
    nodeHierarchy,
    bounds,
    size,
    sampleTris,
    fullMesh: {
      positions: fullPos,
      normals: fullNorm,
      tangents: fullTan,
      uvs: fullUv,
      colors: fullCol,
      indices: idxWrite === fullIdx.length ? fullIdx : fullIdx.subarray(0, idxWrite),
    },
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
