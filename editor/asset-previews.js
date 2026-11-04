// Live studio previews and binary metadata extractors for Ganbate Studio's Asset Library.
// Pure binary parsers (`parseGlbPreview`, `extractWavPeaks`) avoid DOM globals so Node tests
// can exercise them directly.

const MAX_PREVIEW_TRIS = 9000;
const MAX_FULL_MESH_TRIS = 500000;

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

  const matInfos = materials.map((mat, index) => {
    const pbr = mat?.pbrMetallicRoughness || {};
    const f = pbr.baseColorFactor;
    const color = Array.isArray(f) && f.length >= 3
      ? [
          Math.max(0, Math.min(1, Number(f[0]) || 0)),
          Math.max(0, Math.min(1, Number(f[1]) || 0)),
          Math.max(0, Math.min(1, Number(f[2]) || 0)),
        ]
      : [0.76, 0.8, 0.88];
    const texIdx = typeof pbr.baseColorTexture?.index === 'number' ? pbr.baseColorTexture.index : null;
    const imgIdx = texIdx !== null && typeof textures[texIdx]?.source === 'number'
      ? textures[texIdx].source
      : null;
    return {
      index,
      name: typeof mat?.name === 'string' && mat.name ? mat.name : `Material_${index}`,
      color,
      imageIndex: imgIdx,
      doubleSided: Boolean(mat?.doubleSided),
    };
  });
  const matColors = matInfos.map((m) => m.color);

  const embeddedImages = images.map((img, index) => {
    if (!bin || typeof img?.bufferView !== 'number' || !views[img.bufferView]) return null;
    const bv = views[img.bufferView];
    const start = bv.byteOffset || 0;
    const len = bv.byteLength || 0;
    if (len <= 0 || start + len > bin.byteLength) return null;
    const slice = bin.slice(start, start + len);
    const mimeType = typeof img.mimeType === 'string' && img.mimeType
      ? img.mimeType
      : slice[0] === 0x89
        ? 'image/png'
        : slice[0] === 0xff
          ? 'image/jpeg'
          : 'image/webp';
    return { index, mimeType, bytes: slice };
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
    const norm = acc.normalized !== false;
    const readComp = (byteOff) => {
      if (ct === 5126) return viewDv.getFloat32(byteOff, true);
      if (ct === 5121) return norm ? viewDv.getUint8(byteOff) / 255 : viewDv.getUint8(byteOff);
      if (ct === 5123) return norm ? viewDv.getUint16(byteOff, true) / 65535 : viewDv.getUint16(byteOff, true);
      if (ct === 5120) return norm ? Math.max(-1, viewDv.getInt8(byteOff) / 127) : viewDv.getInt8(byteOff);
      return norm ? Math.max(-1, viewDv.getInt16(byteOff, true) / 32767) : viewDv.getInt16(byteOff, true);
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
    if (!acc) return null;
    if (acc.componentType === 5126) return readFloatAccessor(accIdx, 3);
    // Support KHR_mesh_quantization (BYTE/UBYTE/SHORT/USHORT positions)
    if ([5120, 5121, 5122, 5123].includes(acc.componentType)) {
      return readFloatAccessor(accIdx, 3);
    }
    return null;
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
  const dracoPrimitives = [];
  for (const inst of instances) {
    const mesh = meshes[inst.meshIndex];
    const meshName = typeof mesh?.name === 'string' && mesh.name ? mesh.name : `Mesh_${inst.meshIndex}`;
    for (const prim of mesh?.primitives || []) {
      if (prim.mode !== undefined && prim.mode !== 4) continue;
      const matInfo = typeof prim.material === 'number' && matInfos[prim.material]
        ? matInfos[prim.material]
        : { index: -1, name: 'Default', color: [0.76, 0.8, 0.88], imageIndex: null, doubleSided: false };
      const dracoExt = prim.extensions?.KHR_draco_mesh_compression;
      if (dracoExt && typeof dracoExt.bufferView === 'number' && views[dracoExt.bufferView] && bin) {
        const bv = views[dracoExt.bufferView];
        const start = bv.byteOffset || 0;
        const len = bv.byteLength || 0;
        if (len > 0 && start + len <= bin.byteLength) {
          dracoPrimitives.push({
            meshName,
            slice: bin.subarray(start, start + len),
            attributes: dracoExt.attributes || {},
            world: inst.world,
            nMat: inst.nMat,
            matInfo,
          });
        }
      }
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
      rawPrims.push({
        meshName,
        pos,
        norm: norm && norm.count === pos.count ? norm : null,
        tan: tan && tan.count === pos.count ? tan : null,
        uv: uv && uv.count === pos.count ? uv : null,
        vcol: vcol && vcol.count === pos.count ? vcol : null,
        idx,
        triCount,
        world: inst.world,
        nMat: inst.nMat,
        color: matInfo.color,
        matInfo,
      });
    }
  }

  return assembleParsedGlbResult({
    rawPrims,
    dracoPrimitives,
    totalVerts,
    totalTris,
    hasNormals,
    hasTangents,
    hasUvs,
    meshesCount: meshes.length,
    nodesCount: nodes.length,
    materialsCount: materials.length,
    bones,
    texturesCount: textures.length,
    images,
    views,
    bin,
    nodeHierarchy,
    embeddedImages,
  });
}

function assembleParsedGlbResult({
  rawPrims,
  dracoPrimitives = [],
  totalVerts,
  totalTris,
  hasNormals,
  hasTangents,
  hasUvs,
  meshesCount,
  nodesCount,
  materialsCount,
  bones,
  texturesCount,
  images,
  views,
  bin,
  nodeHierarchy,
  embeddedImages = [],
}) {
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
  const submeshes = [];
  let vertBase = 0;
  let idxWrite = 0;

  for (const p of rawPrims) {
    const pMin = [Infinity, Infinity, Infinity];
    const pMax = [-Infinity, -Infinity, -Infinity];
    const subIdxStart = idxWrite;
    for (let v = 0; v < p.pos.count; v++) {
      const [lx, ly, lz] = p.pos.at(v);
      const [wx, wy, wz] = transformVec3(p.world, lx, ly, lz);
      if (wx < min[0]) min[0] = wx;
      if (wy < min[1]) min[1] = wy;
      if (wz < min[2]) min[2] = wz;
      if (wx > max[0]) max[0] = wx;
      if (wy > max[1]) max[1] = wy;
      if (wz > max[2]) max[2] = wz;
      if (wx < pMin[0]) pMin[0] = wx;
      if (wy < pMin[1]) pMin[1] = wy;
      if (wz < pMin[2]) pMin[2] = wz;
      if (wx > pMax[0]) pMax[0] = wx;
      if (wy > pMax[1]) pMax[1] = wy;
      if (wz > pMax[2]) pMax[2] = wz;

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
      submeshes.push({
        name: p.meshName || 'Submesh',
        materialIndex: p.matInfo?.index ?? -1,
        materialName: p.matInfo?.name || 'Default',
        baseColor: p.color,
        imageIndex: typeof p.matInfo?.imageIndex === 'number' ? p.matInfo.imageIndex : null,
        doubleSided: Boolean(p.matInfo?.doubleSided),
        indexOffset: subIdxStart,
        indexCount: idxWrite - subIdxStart,
        triCount: Math.floor((idxWrite - subIdxStart) / 3),
        vertCount: p.pos.count,
        bounds: pMin[0] <= pMax[0] ? [pMin, pMax] : [[0, 0, 0], [0, 0, 0]],
      });
    } else if (pMin[0] <= pMax[0]) {
      submeshes.push({
        name: p.meshName || 'Submesh',
        materialIndex: p.matInfo?.index ?? -1,
        materialName: p.matInfo?.name || 'Default',
        baseColor: p.color,
        imageIndex: typeof p.matInfo?.imageIndex === 'number' ? p.matInfo.imageIndex : null,
        doubleSided: Boolean(p.matInfo?.doubleSided),
        indexOffset: 0,
        indexCount: 0,
        triCount: p.triCount,
        vertCount: p.pos.count,
        bounds: [pMin, pMax],
      });
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

  // Smart focus bounds: ignore tiny flat ground/shadow decal quads when framing characters/props
  let focusBounds = bounds;
  if (hasBounds && totalTris > 120 && submeshes.length > 1) {
    const overallSpan = Math.max(size[0], size[1], size[2], 1e-4);
    const fMin = [Infinity, Infinity, Infinity];
    const fMax = [-Infinity, -Infinity, -Infinity];
    for (const sm of submeshes) {
      const [sMin, sMax] = sm.bounds;
      const dy = sMax[1] - sMin[1];
      const isFlatDecal =
        (sm.vertCount <= 16 && sm.triCount <= 12) ||
        (dy < overallSpan * 0.012 && sm.triCount < totalTris * 0.04);
      if (isFlatDecal) continue;
      for (let k = 0; k < 3; k++) {
        if (sMin[k] < fMin[k]) fMin[k] = sMin[k];
        if (sMax[k] > fMax[k]) fMax[k] = sMax[k];
      }
    }
    if (fMin[0] <= fMax[0]) {
      focusBounds = [fMin, fMax];
    }
  }

  return {
    vertices: totalVerts,
    triangles: totalTris,
    meshes: meshesCount,
    nodes: nodesCount,
    materials: materialsCount,
    bones,
    textures: texturesCount,
    estimatedVramBytes,
    hasNormals,
    hasTangents,
    hasUvs,
    usesDraco: dracoPrimitives.length > 0,
    dracoPrimitives,
    embeddedImages,
    submeshes,
    nodeHierarchy,
    bounds,
    focusBounds,
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

let dracoDecoderPromise = null;

async function getDracoDecoderModule({
  scriptUrl = '/vendor/draco/draco_decoder_nodejs.js',
  wasmUrl = '/vendor/draco/draco_decoder.wasm',
  fetchImpl = globalThis.fetch,
} = {}) {
  if (dracoDecoderPromise) return dracoDecoderPromise;
  dracoDecoderPromise = (async () => {
    try {
      if (typeof fetchImpl !== 'function') return null;
      const [jsRes, wasmRes] = await Promise.all([fetchImpl(scriptUrl), fetchImpl(wasmUrl)]);
      if (!jsRes?.ok || !wasmRes?.ok) return null;
      const [jsCode, wasmBinary] = await Promise.all([jsRes.text(), wasmRes.arrayBuffer()]);
      // Instantiate Emscripten factory without polluting globals
      const factory = new Function(`${jsCode}\nreturn typeof DracoDecoderModule === 'function' ? DracoDecoderModule : null;`)();
      if (typeof factory !== 'function') return null;
      return await factory({ wasmBinary });
    } catch {
      return null;
    }
  })();
  const mod = await dracoDecoderPromise;
  if (!mod) dracoDecoderPromise = null;
  return mod;
}

/**
 * Async `.glb` parser that automatically decodes `KHR_draco_mesh_compression` primitives via
 * `/vendor/draco/draco_decoder.wasm` when raw uncompressed accessors are absent.
 */
export async function parseGlbPreviewAsync(input, options = {}) {
  const syncResult = parseGlbPreview(input);
  if (!syncResult || !syncResult.usesDraco || syncResult.triangles > 0) {
    return syncResult;
  }
  const draco = await getDracoDecoderModule(options);
  if (!draco) return syncResult;

  const decodedPrims = [];
  let totalVerts = 0;
  let totalTris = 0;
  let hasNormals = false;
  let hasUvs = false;

  for (const dp of syncResult.dracoPrimitives || []) {
    const decoder = new draco.Decoder();
    const buffer = new draco.DecoderBuffer();
    let mesh = null;
    try {
      buffer.Init(new Int8Array(dp.slice.buffer, dp.slice.byteOffset, dp.slice.byteLength), dp.slice.byteLength);
      const geomType = decoder.GetEncodedGeometryType(buffer);
      if (geomType !== draco.TRIANGULAR_MESH) continue;
      mesh = new draco.Mesh();
      const status = decoder.DecodeBufferToMesh(buffer, mesh);
      if (!status || !status.ok()) continue;
      const numPoints = mesh.num_points();
      const numFaces = mesh.num_faces();
      if (numPoints < 3 || numFaces < 1) continue;

      const readDracoAttr = (attrName, fallbackEnum, arity) => {
        let attr = null;
        if (typeof dp.attributes?.[attrName] === 'number') {
          attr = decoder.GetAttributeByUniqueId(mesh, dp.attributes[attrName]);
        } else if (fallbackEnum !== null) {
          const id = decoder.GetAttributeId(mesh, fallbackEnum);
          if (id >= 0) attr = decoder.GetAttribute(mesh, id);
        }
        if (!attr) return null;
        const arr = new draco.DracoFloat32Array();
        const ok = decoder.GetAttributeFloatForAllPoints(mesh, attr, arr);
        if (!ok) {
          draco.destroy(arr);
          return null;
        }
        const out = new Float32Array(numPoints * arity);
        for (let i = 0; i < out.length; i++) out[i] = arr.GetValue(i);
        draco.destroy(arr);
        return {
          count: numPoints,
          at(i) {
            const o = i * arity;
            if (arity === 2) return [out[o], out[o + 1]];
            if (arity === 3) return [out[o], out[o + 1], out[o + 2]];
            return [out[o], out[o + 1], out[o + 2], out[o + 3]];
          },
        };
      };

      const pos = readDracoAttr('POSITION', draco.POSITION, 3);
      if (!pos) continue;
      const norm = readDracoAttr('NORMAL', draco.NORMAL, 3);
      const uv = readDracoAttr('TEXCOORD_0', draco.TEX_COORD, 2);
      if (norm) hasNormals = true;
      if (uv) hasUvs = true;

      const ia = new draco.DracoInt32Array();
      const indices = new Uint32Array(numFaces * 3);
      for (let f = 0; f < numFaces; f++) {
        decoder.GetFaceFromMesh(mesh, f, ia);
        indices[f * 3] = ia.GetValue(0);
        indices[f * 3 + 1] = ia.GetValue(1);
        indices[f * 3 + 2] = ia.GetValue(2);
      }
      draco.destroy(ia);

      totalVerts += numPoints;
      totalTris += numFaces;
      decodedPrims.push({
        meshName: dp.meshName,
        pos,
        norm,
        tan: null,
        uv,
        vcol: null,
        idx: { count: indices.length, at: (k) => indices[k] },
        triCount: numFaces,
        world: dp.world,
        nMat: dp.nMat,
        color: dp.matInfo.color,
        matInfo: dp.matInfo,
      });
    } finally {
      if (mesh) draco.destroy(mesh);
      draco.destroy(buffer);
      draco.destroy(decoder);
    }
  }

  if (!decodedPrims.length) return syncResult;
  return assembleParsedGlbResult({
    rawPrims: decodedPrims,
    dracoPrimitives: syncResult.dracoPrimitives,
    totalVerts,
    totalTris,
    hasNormals,
    hasTangents: false,
    hasUvs,
    meshesCount: syncResult.meshes,
    nodesCount: syncResult.nodes,
    materialsCount: syncResult.materials,
    bones: syncResult.bones,
    texturesCount: syncResult.textures,
    images: [],
    views: [],
    bin: null,
    nodeHierarchy: syncResult.nodeHierarchy,
    embeddedImages: syncResult.embeddedImages,
  });
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
  if (!canvas || typeof canvas.getContext !== 'function') {
    return { ctx: null, w: cssWidth, h: cssHeight };
  }
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

const GLB_VS_300 = `#version 300 es
precision highp float;
layout(location = 0) in vec3 aPos;
layout(location = 1) in vec3 aNorm;
layout(location = 2) in vec2 aUv;
layout(location = 3) in vec3 aCol;

uniform mat4 uViewProj;
out vec3 vWorldPos;
out vec3 vNorm;
out vec2 vUv;
out vec3 vCol;

void main() {
  vWorldPos = aPos;
  vNorm = aNorm;
  vUv = aUv;
  vCol = aCol;
  gl_Position = uViewProj * vec4(aPos, 1.0);
}
`;

const GLB_FS_300 = `#version 300 es
precision highp float;
in vec3 vWorldPos;
in vec3 vNorm;
in vec2 vUv;
in vec3 vCol;

uniform vec3 uEye;
uniform int uMode; // 0=textured/shaded, 1=clay, 2=normals, 3=wireline
uniform int uHasTex;
uniform sampler2D uTex;
uniform vec3 uSubColor;

out vec4 fragColor;

void main() {
  if (uMode == 3) {
    fragColor = vec4(0.38, 0.78, 1.0, 0.72);
    return;
  }
  vec3 N = normalize(vNorm);
  if (length(vNorm) < 0.001) {
    vec3 dx = dFdx(vWorldPos);
    vec3 dy = dFdy(vWorldPos);
    N = normalize(cross(dx, dy));
  }
  vec3 V = normalize(uEye - vWorldPos);
  if (dot(N, V) < 0.0) N = -N;

  if (uMode == 2) {
    fragColor = vec4(N * 0.5 + 0.5, 1.0);
    return;
  }

  vec3 base = vCol;
  if (uMode == 1) {
    base = vec3(0.76, 0.74, 0.72);
  } else if (uHasTex == 1) {
    vec4 texel = texture(uTex, vUv);
    // Approximate sRGB -> linear
    vec3 texLin = pow(max(texel.rgb, vec3(0.0)), vec3(2.2));
    base = texLin * max(uSubColor, vec3(0.08));
  }

  // Studio 3-point lighting (Key + Cool Fill + Rim + Hemisphere)
  vec3 keyDir = normalize(vec3(0.48, 0.68, 0.55));
  vec3 fillDir = normalize(vec3(-0.65, 0.28, -0.45));
  float ndlKey = max(0.0, dot(N, keyDir));
  float ndlFill = max(0.0, dot(N, fillDir));
  float hemi = 0.28 + 0.22 * (N.y * 0.5 + 0.5);
  float rim = pow(1.0 - clamp(dot(N, V), 0.0, 1.0), 2.6) * 0.32;

  vec3 H = normalize(keyDir + V);
  float spec = pow(max(0.0, dot(N, H)), 28.0) * (uMode == 1 ? 0.18 : 0.12);

  vec3 lit = base * (
    vec3(hemi) +
    vec3(1.06, 1.01, 0.96) * (ndlKey * 0.82) +
    vec3(0.52, 0.66, 0.92) * (ndlFill * 0.34)
  ) + vec3(0.55, 0.72, 0.98) * rim + vec3(spec);

  // Subtle filmic tone curve + gamma 2.2
  lit = lit / (lit + vec3(0.85)) * 1.62;
  vec3 srgb = pow(clamp(lit, 0.0, 1.0), vec3(1.0 / 2.2));
  fragColor = vec4(srgb, 1.0);
}
`;

const BG_VS_300 = `#version 300 es
precision mediump float;
out vec2 vUv;
void main() {
  vec2 p = vec2((gl_VertexID << 1) & 2, gl_VertexID & 2);
  vUv = p;
  gl_Position = vec4(p * 2.0 - 1.0, 0.9999, 1.0);
}
`;

const BG_FS_300 = `#version 300 es
precision mediump float;
in vec2 vUv;
out vec4 fragColor;
void main() {
  vec3 top = vec3(0.13, 0.16, 0.23);
  vec3 mid = vec3(0.09, 0.11, 0.16);
  vec3 bot = vec3(0.06, 0.07, 0.10);
  float t = clamp(vUv.y, 0.0, 1.0);
  vec3 col = t > 0.45 ? mix(mid, top, (t - 0.45) / 0.55) : mix(bot, mid, t / 0.45);
  float radial = 1.0 - smoothstep(0.0, 0.85, length(vUv - vec2(0.5, 0.48)));
  col += vec3(0.035, 0.05, 0.085) * radial;
  fragColor = vec4(col, 1.0);
}
`;

function compileGlProgram(gl, vsSrc, fsSrc) {
  const compile = (type, src) => {
    const sh = gl.createShader(type);
    if (!sh) return null;
    gl.shaderSource(sh, src);
    gl.compileShader(sh);
    if (!gl.getShaderParameter(sh, gl.COMPILE_STATUS)) {
      gl.deleteShader(sh);
      return null;
    }
    return sh;
  };
  const vs = compile(gl.VERTEX_SHADER, vsSrc);
  const fs = compile(gl.FRAGMENT_SHADER, fsSrc);
  if (!vs || !fs) return null;
  const prog = gl.createProgram();
  if (!prog) return null;
  gl.attachShader(prog, vs);
  gl.attachShader(prog, fs);
  gl.linkProgram(prog);
  gl.deleteShader(vs);
  gl.deleteShader(fs);
  if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) {
    gl.deleteProgram(prog);
    return null;
  }
  return prog;
}

function buildPerspectiveViewProj(eye, target, upVec, fovRad, aspect, near, far) {
  let zx = eye[0] - target[0], zy = eye[1] - target[1], zz = eye[2] - target[2];
  const zl = Math.hypot(zx, zy, zz) || 1;
  zx /= zl; zy /= zl; zz /= zl;

  let xx = upVec[1] * zz - upVec[2] * zy;
  let xy = upVec[2] * zx - upVec[0] * zz;
  let xz = upVec[0] * zy - upVec[1] * zx;
  const xl = Math.hypot(xx, xy, xz) || 1;
  xx /= xl; xy /= xl; xz /= xl;

  const yx = zy * xz - zz * xy;
  const yy = zz * xx - zx * xz;
  const yz = zx * xy - zy * xx;

  const view = new Float32Array([
    xx, yx, zx, 0,
    xy, yy, zy, 0,
    xz, yz, zz, 0,
    -(xx * eye[0] + xy * eye[1] + xz * eye[2]),
    -(yx * eye[0] + yy * eye[1] + yz * eye[2]),
    -(zx * eye[0] + zy * eye[1] + zz * eye[2]),
    1,
  ]);

  const f = 1 / Math.tan(fovRad * 0.5);
  const nf = 1 / (near - far);
  const proj = new Float32Array([
    f / Math.max(0.1, aspect), 0, 0, 0,
    0, f, 0, 0,
    0, 0, (far + near) * nf, -1,
    0, 0, 2 * far * near * nf, 0,
  ]);
  return mulMat4(proj, view);
}

function getOrInitCanvasGl(canvas) {
  if (!canvas || typeof canvas.getContext !== 'function' || canvas.__ganbate2dLocked) return null;
  if (canvas.__ganbateGlState) return canvas.__ganbateGlState;
  let gl = null;
  try {
    gl = canvas.getContext('webgl2', {
      antialias: true,
      alpha: false,
      depth: true,
      premultipliedAlpha: false,
      preserveDrawingBuffer: false,
    });
  } catch {
    gl = null;
  }
  if (!gl) {
    canvas.__ganbate2dLocked = true;
    return null;
  }
  const meshProg = compileGlProgram(gl, GLB_VS_300, GLB_FS_300);
  const bgProg = compileGlProgram(gl, BG_VS_300, BG_FS_300);
  if (!meshProg || !bgProg) {
    canvas.__ganbate2dLocked = true;
    return null;
  }
  const bgVao = gl.createVertexArray();
  const state = {
    gl,
    meshProg,
    bgProg,
    bgVao,
    locs: {
      uViewProj: gl.getUniformLocation(meshProg, 'uViewProj'),
      uEye: gl.getUniformLocation(meshProg, 'uEye'),
      uMode: gl.getUniformLocation(meshProg, 'uMode'),
      uHasTex: gl.getUniformLocation(meshProg, 'uHasTex'),
      uTex: gl.getUniformLocation(meshProg, 'uTex'),
      uSubColor: gl.getUniformLocation(meshProg, 'uSubColor'),
    },
    uploadedInfo: null,
    vao: null,
    wireVao: null,
    wireCount: 0,
    texturesByImage: new Map(),
    lastDrawOpts: null,
  };
  canvas.__ganbateGlState = state;
  return state;
}

function uploadMeshToGl(glState, info, canvas) {
  const { gl } = glState;
  if (glState.uploadedInfo === info && glState.vao) return;
  if (glState.vao) gl.deleteVertexArray(glState.vao);
  if (glState.wireVao) gl.deleteVertexArray(glState.wireVao);
  for (const tex of glState.texturesByImage.values()) {
    if (tex) gl.deleteTexture(tex);
  }
  glState.texturesByImage.clear();
  glState.wireVao = null;
  glState.wireCount = 0;

  const fm = info?.fullMesh;
  if (!fm || !fm.indices?.length || !fm.positions?.length) {
    glState.uploadedInfo = info;
    glState.vao = null;
    return;
  }

  const vao = gl.createVertexArray();
  gl.bindVertexArray(vao);

  const makeBuf = (loc, arr, size) => {
    const b = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, b);
    gl.bufferData(gl.ARRAY_BUFFER, arr, gl.STATIC_DRAW);
    gl.enableVertexAttribArray(loc);
    gl.vertexAttribPointer(loc, size, gl.FLOAT, false, 0, 0);
  };
  makeBuf(0, fm.positions, 3);
  makeBuf(1, fm.normals, 3);
  makeBuf(2, fm.uvs, 2);
  makeBuf(3, fm.colors, 3);

  const ebo = gl.createBuffer();
  gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, ebo);
  gl.bufferData(gl.ELEMENT_ARRAY_BUFFER, fm.indices, gl.STATIC_DRAW);
  gl.bindVertexArray(null);

  glState.vao = vao;
  glState.posBuf = fm.positions;
  glState.uploadedInfo = info;

  // Decode embedded diffuse images asynchronously using browser ImageBitmap
  if (typeof createImageBitmap === 'function' && typeof Blob === 'function' && Array.isArray(info.embeddedImages)) {
    const needed = new Set(
      (info.submeshes || [])
        .map((sm) => sm.imageIndex)
        .filter((idx) => typeof idx === 'number' && idx >= 0),
    );
    for (const imgIdx of needed) {
      const entry = info.embeddedImages[imgIdx];
      if (!entry?.bytes) continue;
      glState.texturesByImage.set(imgIdx, null);
      const blob = new Blob([entry.bytes], { type: entry.mimeType || 'image/webp' });
      createImageBitmap(blob)
        .then((bmp) => {
          if (glState.uploadedInfo !== info) {
            bmp.close?.();
            return;
          }
          const tex = gl.createTexture();
          gl.bindTexture(gl.TEXTURE_2D, tex);
          gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, false);
          gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, bmp);
          gl.generateMipmap(gl.TEXTURE_2D);
          gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR_MIPMAP_LINEAR);
          gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
          gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.REPEAT);
          gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.REPEAT);
          bmp.close?.();
          glState.texturesByImage.set(imgIdx, tex);
          if (glState.lastDrawOpts) {
            drawGlbPreviewCanvas(canvas, info, glState.lastDrawOpts);
          }
        })
        .catch(() => {});
    }
  }
}

function ensureWireframeVao(glState, info) {
  if (glState.wireVao) return;
  const { gl } = glState;
  const fm = info?.fullMesh;
  if (!fm?.indices?.length) return;
  const triCount = Math.floor(fm.indices.length / 3);
  const stride = Math.max(1, Math.ceil(triCount / 45000));
  const sampled = Math.ceil(triCount / stride);
  const lineIdx = new Uint32Array(sampled * 6);
  let w = 0;
  for (let t = 0; t < triCount; t += stride) {
    const a = fm.indices[t * 3];
    const b = fm.indices[t * 3 + 1];
    const c = fm.indices[t * 3 + 2];
    lineIdx[w++] = a; lineIdx[w++] = b;
    lineIdx[w++] = b; lineIdx[w++] = c;
    lineIdx[w++] = c; lineIdx[w++] = a;
  }
  const vao = gl.createVertexArray();
  gl.bindVertexArray(vao);
  const posB = gl.createBuffer();
  gl.bindBuffer(gl.ARRAY_BUFFER, posB);
  gl.bufferData(gl.ARRAY_BUFFER, fm.positions, gl.STATIC_DRAW);
  gl.enableVertexAttribArray(0);
  gl.vertexAttribPointer(0, 3, gl.FLOAT, false, 0, 0);
  const ebo = gl.createBuffer();
  gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, ebo);
  gl.bufferData(gl.ELEMENT_ARRAY_BUFFER, lineIdx.subarray(0, w), gl.STATIC_DRAW);
  gl.bindVertexArray(null);
  glState.wireVao = vao;
  glState.wireCount = w;
}

export function drawGlbPreviewCanvas(
  canvas,
  info,
  {
    width = 180,
    height = 100,
    yaw = 0.52,
    pitch = 0.22,
    zoom = 1.0,
    panX = 0,
    panY = 0,
    shading = 'textured',
    focus = 'body',
  } = {},
) {
  const glState = getOrInitCanvasGl(canvas);
  if (glState) {
    const { gl, meshProg, bgProg, bgVao, locs } = glState;
    glState.lastDrawOpts = { width, height, yaw, pitch, zoom, panX, panY, shading, focus };
    const dpr = typeof window !== 'undefined' ? Math.min(2, window.devicePixelRatio || 1) : 1;
    const pxW = Math.max(16, Math.round(width * dpr));
    const pxH = Math.max(16, Math.round(height * dpr));
    if (canvas.width !== pxW || canvas.height !== pxH) {
      canvas.width = pxW;
      canvas.height = pxH;
    }
    gl.viewport(0, 0, pxW, pxH);
    gl.clearDepth(1.0);
    gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);

    // Draw studio gradient background
    gl.disable(gl.DEPTH_TEST);
    gl.useProgram(bgProg);
    gl.bindVertexArray(bgVao);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    gl.bindVertexArray(null);

    if (!info || !info.fullMesh?.indices?.length) return;
    uploadMeshToGl(glState, info, canvas);
    if (!glState.vao) return;

    const [bMin, bMax] = info.focusBounds || info.bounds;
    const spanX = Math.max(1e-4, bMax[0] - bMin[0]);
    const spanY = Math.max(1e-4, bMax[1] - bMin[1]);
    const spanZ = Math.max(1e-4, bMax[2] - bMin[2]);
    const maxSpan = Math.max(spanX, spanY, spanZ, 1e-4);

    let cx = (bMin[0] + bMax[0]) * 0.5;
    let cy = focus === 'bust' ? bMin[1] + spanY * 0.82 : bMin[1] + spanY * 0.5;
    let cz = (bMin[2] + bMax[2]) * 0.5;
    const fitSpan = focus === 'bust' ? Math.max(spanY * 0.34, spanX * 0.45, 1e-4) : Math.max(spanY * 1.08, Math.max(spanX, spanZ) * 0.95);

    const fovRad = (36 * Math.PI) / 180;
    const aspect = Math.max(0.2, width / Math.max(1, height));
    const vHalf = Math.tan(fovRad * 0.5);
    const hHalf = vHalf * aspect;
    const baseDist = Math.max(
      (fitSpan * 0.54) / vHalf,
      (Math.max(spanX, spanZ) * (focus === 'bust' ? 0.28 : 0.56)) / Math.max(0.15, hHalf),
    );
    const dist = Math.max(maxSpan * 0.08, baseDist / Math.max(0.15, Math.min(12, zoom)));

    const cyaw = Math.cos(yaw), syaw = Math.sin(yaw);
    const cp = Math.cos(pitch), sp = Math.sin(pitch);
    const right = [cyaw, 0, -syaw];
    const up = [-syaw * sp, cp, -cyaw * sp];
    const fwd = [syaw * cp, sp, cyaw * cp];

    const panScale = dist * 0.0024;
    cx -= (right[0] * panX - up[0] * panY) * panScale;
    cy -= (right[1] * panX - up[1] * panY) * panScale;
    cz -= (right[2] * panX - up[2] * panY) * panScale;

    const eye = [cx + fwd[0] * dist, cy + fwd[1] * dist, cz + fwd[2] * dist];
    const vp = buildPerspectiveViewProj(
      eye,
      [cx, cy, cz],
      [0, 1, 0],
      fovRad,
      aspect,
      Math.max(1e-4, dist * 0.02),
      Math.max(10, dist * 16 + maxSpan * 8),
    );

    gl.enable(gl.DEPTH_TEST);
    gl.depthFunc(gl.LEQUAL);
    gl.disable(gl.CULL_FACE);
    if (shading === 'wireframe') {
      gl.enable(gl.POLYGON_OFFSET_FILL);
      gl.polygonOffset(1.0, 1.0);
    } else {
      gl.disable(gl.POLYGON_OFFSET_FILL);
    }

    gl.useProgram(meshProg);
    gl.uniformMatrix4fv(locs.uViewProj, false, vp);
    gl.uniform3fv(locs.uEye, eye);
    const modeInt = shading === 'clay' ? 1 : shading === 'normals' ? 2 : 0;
    gl.uniform1i(locs.uMode, modeInt);
    gl.uniform1i(locs.uTex, 0);

    gl.bindVertexArray(glState.vao);
    const subs = info.submeshes?.length
      ? info.submeshes
      : [{ indexOffset: 0, indexCount: info.fullMesh.indices.length, imageIndex: null, baseColor: [0.85, 0.88, 0.94] }];
    for (const sm of subs) {
      if (!sm.indexCount) continue;
      const tex = shading === 'textured' && typeof sm.imageIndex === 'number'
        ? glState.texturesByImage.get(sm.imageIndex) || null
        : null;
      if (tex) {
        gl.activeTexture(gl.TEXTURE0);
        gl.bindTexture(gl.TEXTURE_2D, tex);
        gl.uniform1i(locs.uHasTex, 1);
      } else {
        gl.uniform1i(locs.uHasTex, 0);
      }
      const c = sm.baseColor || [0.85, 0.88, 0.94];
      gl.uniform3f(locs.uSubColor, c[0], c[1], c[2]);
      gl.drawElements(gl.TRIANGLES, sm.indexCount, gl.UNSIGNED_INT, sm.indexOffset * 4);
    }
    gl.bindVertexArray(null);
    gl.disable(gl.POLYGON_OFFSET_FILL);

    if (shading === 'wireframe') {
      ensureWireframeVao(glState, info);
      if (glState.wireVao && glState.wireCount > 0) {
        gl.enable(gl.BLEND);
        gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
        gl.uniform1i(locs.uMode, 3);
        gl.uniform1i(locs.uHasTex, 0);
        gl.bindVertexArray(glState.wireVao);
        gl.drawElements(gl.LINES, glState.wireCount, gl.UNSIGNED_INT, 0);
        gl.bindVertexArray(null);
        gl.disable(gl.BLEND);
      }
    }
    return;
  }

  // Fallback 2D software painter's renderer (used in headless Node / jsdom unit tests)
  const { ctx, w, h } = prepCanvas(canvas, width, height);
  if (!ctx) return;

  const bg = ctx.createLinearGradient(0, 0, 0, h);
  bg.addColorStop(0, '#1e2430');
  bg.addColorStop(1, '#12151b');
  ctx.fillStyle = bg;
  ctx.fillRect(0, 0, w, h);

  ctx.save();
  ctx.fillStyle = 'rgba(0, 0, 0, 0.34)';
  ctx.beginPath();
  ctx.ellipse(w * 0.5, h * 0.78, w * 0.28, h * 0.09, 0, 0, Math.PI * 2);
  ctx.fill();
  ctx.restore();

  if (!info || !info.sampleTris?.length) return;

  const [min, max] = info.focusBounds || info.bounds;
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
          const parsed = await parseGlbPreviewAsync(buf);
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
