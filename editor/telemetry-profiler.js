// Ganbate Studio Telemetry & Culling Profiler Workbench (`editor/telemetry-profiler.js`).
// Standalone docked workbench (`mountTelemetryProfiler(container, studioApi)`) with zero
// third-party dependencies and zero per-frame GC allocations in its ring buffer.

import { cameraWorld, orbitBasis } from './cameras.js';
import { fromTRS, multiply, point } from './math.js';

const BUDGET_60HZ_MS = 1000 / 60;
const BUDGET_120HZ_MS = 1000 / 120;

const STRINGS = {
  en: {
    title: 'Telemetry & Culling Profiler',
    pause: 'Pause',
    resume: 'Resume',
    reset: 'Clear',
    freezeCam: 'Freeze Frustum',
    unfreezeCam: 'Unfreeze Frustum',
    framegraph: 'Rolling 120-Frame CPU / GPU Timing',
    vramHud: 'Draw, RenderBundle & Hardware Texture Compression Budget',
    cullTitle: 'Frustum, Distance, Subpixel & Early-Z Culling Inspector',
    visible: 'Visible',
    frustumCulled: 'Frustum Culled',
    distSubpxCulled: 'Dist / Subpixel Culled',
    earlyZ: 'Early-Z Front-to-Back Bins',
    maxDist: 'Max Distance (m)',
    minPx: 'Min Screen Size (px)',
  },
  ko: {
    title: '텔레메트리 & 컬링 프로파일러',
    pause: '일시정지',
    resume: '재개',
    reset: '초기화',
    freezeCam: '프러스텀 고정',
    unfreezeCam: '프러스텀 해제',
    framegraph: '실시간 120프레임 CPU / GPU 타이밍',
    vramHud: '드로우, 렌더번들 & 하드웨어 텍스처 압축 예산',
    cullTitle: '프러스텀 · 거리 · 서브픽셀 & Early-Z 컬링 인스펙터',
    visible: '가시 객체',
    frustumCulled: '프러스텀 컬링',
    distSubpxCulled: '거리 / 서브픽셀 컬링',
    earlyZ: 'Early-Z 전후 깊이 분포',
    maxDist: '최대 가시거리 (m)',
    minPx: '최소 화면 크기 (px)',
  },
};

function esc(value) {
  return String(value ?? '')
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;');
}

function fmtBytes(bytes) {
  const b = Number.isFinite(bytes) && bytes > 0 ? bytes : 0;
  if (b >= 1024 * 1024) return `${(b / (1024 * 1024)).toFixed(2)} MiB`;
  if (b >= 1024) return `${(b / 1024).toFixed(1)} KiB`;
  return `${Math.round(b)} B`;
}

/**
 * Fixed-capacity TypedArray ring buffer for rolling frame telemetry with zero per-sample heap
 * allocations during steady-state profiling.
 */
export function createFrameRing(capacity = 120) {
  const cap = Math.max(4, Math.trunc(capacity));
  return {
    capacity: cap,
    head: 0,
    count: 0,
    cpuMs: new Float64Array(cap),
    gpuMs: new Float64Array(cap),
    fps: new Float64Array(cap),
    draws: new Uint32Array(cap),
    triangles: new Uint32Array(cap),
    scratchSort: new Float64Array(cap),
  };
}

export function pushFrameSample(ring, sample = {}) {
  const idx = ring.head;
  const cpu = Number.isFinite(sample.frameTimeMs) && sample.frameTimeMs >= 0 ? sample.frameTimeMs : 0;
  const gpu = Number.isFinite(sample.gpuTimeMs) && sample.gpuTimeMs >= 0 ? sample.gpuTimeMs : 0;
  const fps = Number.isFinite(sample.fps) && sample.fps >= 0
    ? sample.fps
    : (cpu > 1e-6 ? 1000 / cpu : 0);
  ring.cpuMs[idx] = cpu;
  ring.gpuMs[idx] = gpu;
  ring.fps[idx] = fps;
  ring.draws[idx] = Math.max(0, Math.trunc(sample.draws || 0));
  ring.triangles[idx] = Math.max(0, Math.trunc(sample.triangles || 0));
  ring.head = (idx + 1) % ring.capacity;
  if (ring.count < ring.capacity) ring.count += 1;
}

export function clearFrameRing(ring) {
  ring.head = 0;
  ring.count = 0;
}

function percentileFromSorted(sorted, len, q) {
  if (len === 0) return 0;
  const pos = Math.min(len - 1, Math.max(0, Math.ceil(q * len) - 1));
  return sorted[pos];
}

function summarizeSeries(source, head, count, cap, scratch) {
  if (count === 0) return { p50: 0, p95: 0, p99: 0, avg: 0, max: 0, latest: 0 };
  let sum = 0;
  const start = count === cap ? head : 0;
  for (let i = 0; i < count; i += 1) {
    const v = source[(start + i) % cap];
    scratch[i] = v;
    sum += v;
  }
  const latest = source[(head + cap - 1) % cap];
  const sub = scratch.subarray(0, count);
  sub.sort();
  return {
    p50: percentileFromSorted(sub, count, 0.50),
    p95: percentileFromSorted(sub, count, 0.95),
    p99: percentileFromSorted(sub, count, 0.99),
    avg: sum / count,
    max: sub[count - 1],
    latest,
  };
}

/**
 * Computes `p50`, `p95`, `p99`, average, and bottleneck classification from a `FrameRing`.
 */
export function computeFramePercentiles(ring) {
  const cpu = summarizeSeries(ring.cpuMs, ring.head, ring.count, ring.capacity, ring.scratchSort);
  const gpu = summarizeSeries(ring.gpuMs, ring.head, ring.count, ring.capacity, ring.scratchSort);
  const latestIdx = ring.count > 0 ? (ring.head + ring.capacity - 1) % ring.capacity : 0;
  const fps = ring.count > 0 ? ring.fps[latestIdx] : 0;
  const draws = ring.count > 0 ? ring.draws[latestIdx] : 0;
  const triangles = ring.count > 0 ? ring.triangles[latestIdx] : 0;

  const peakP95 = Math.max(cpu.p95, gpu.p95);
  let bottleneck = 'within-budget';
  if (peakP95 > BUDGET_60HZ_MS) {
    bottleneck = gpu.p95 >= cpu.p95 * 1.15 ? 'gpu-bound' : 'cpu-bound';
  }

  return {
    samples: ring.count,
    fps,
    draws,
    triangles,
    cpu,
    gpu,
    budget60HeadroomMs: BUDGET_60HZ_MS - peakP95,
    budget120HeadroomMs: BUDGET_120HZ_MS - peakP95,
    bottleneck,
  };
}

/**
 * Exact GPU mip-chain byte footprint matching `ganbate` Vulkan / Metal / WebGPU physical 4x4
 * block-aligned mip extents (`ceil(mipW / 4) * ceil(mipH / 4) * blockBytes`).
 */
export function computeMipChainBytes(width, height, format = 'rgba8', withMips = true) {
  let w = Math.max(1, Math.trunc(width || 1));
  let h = Math.max(1, Math.trunc(height || 1));
  let total = 0;
  const kind = String(format).toLowerCase();
  while (true) {
    if (kind === 'bc1') {
      total += Math.ceil(w / 4) * Math.ceil(h / 4) * 8;
    } else if (kind === 'bc7' || kind === 'astc4x4' || kind === 'astc-4x4') {
      total += Math.ceil(w / 4) * Math.ceil(h / 4) * 16;
    } else {
      total += w * h * 4;
    }
    if (!withMips || (w === 1 && h === 1)) break;
    w = Math.max(1, w >> 1);
    h = Math.max(1, h >> 1);
  }
  return total;
}

/**
 * Computes comparative VRAM footprints across uncompressed `RGBA8` vs hardware `BC1` (8:1),
 * `BC7` (4:1), and `ASTC 4x4` (4:1) block-compressed mip chains.
 */
export function computeVramCompressionBudget(textures = [{ width: 2048, height: 2048, mips: true }]) {
  const list = Array.isArray(textures) && textures.length > 0
    ? textures
    : [{ width: 2048, height: 2048, mips: true }];
  let rgba8Bytes = 0;
  let bc1Bytes = 0;
  let bc7Bytes = 0;
  let astc4x4Bytes = 0;
  for (const tex of list) {
    const w = tex.width || 1024;
    const h = tex.height || 1024;
    const mips = tex.mips !== false;
    rgba8Bytes += computeMipChainBytes(w, h, 'rgba8', mips);
    bc1Bytes += computeMipChainBytes(w, h, 'bc1', mips);
    bc7Bytes += computeMipChainBytes(w, h, 'bc7', mips);
    astc4x4Bytes += computeMipChainBytes(w, h, 'astc4x4', mips);
  }
  return {
    textureCount: list.length,
    rgba8Bytes,
    bc1Bytes,
    bc7Bytes,
    astc4x4Bytes,
    bc1Ratio: bc1Bytes > 0 ? rgba8Bytes / bc1Bytes : 1,
    bc7Ratio: bc7Bytes > 0 ? rgba8Bytes / bc7Bytes : 1,
    astcRatio: astc4x4Bytes > 0 ? rgba8Bytes / astc4x4Bytes : 1,
    bc1SavedBytes: Math.max(0, rgba8Bytes - bc1Bytes),
    bc7SavedBytes: Math.max(0, rgba8Bytes - bc7Bytes),
  };
}

/**
 * Normalizes a camera specification into `{ position, forward, right, up, fovDeg, aspect, near, far }`.
 */
export function normalizeCameraFrustum(camera = {}) {
  const fovDeg = Number.isFinite(camera.fov) && camera.fov > 1 && camera.fov < 175 ? camera.fov : 50;
  const aspect = Number.isFinite(camera.aspect) && camera.aspect > 0.05 ? camera.aspect : (16 / 9);
  const near = Number.isFinite(camera.near) && camera.near > 1e-4 ? camera.near : 0.1;
  const far = Number.isFinite(camera.far) && camera.far > near ? camera.far : 500;

  if (Array.isArray(camera.position) && Array.isArray(camera.forward)) {
    const fLen = Math.hypot(...camera.forward) || 1;
    const forward = camera.forward.map((v) => v / fLen);
    const upRef = Array.isArray(camera.up) ? camera.up : [0, 1, 0];
    let rx = forward[1] * upRef[2] - forward[2] * upRef[1];
    let ry = forward[2] * upRef[0] - forward[0] * upRef[2];
    let rz = forward[0] * upRef[1] - forward[1] * upRef[0];
    let rLen = Math.hypot(rx, ry, rz);
    if (rLen < 1e-6) {
      rx = 1; ry = 0; rz = 0; rLen = 1;
    }
    const right = [rx / rLen, ry / rLen, rz / rLen];
    const up = [
      right[1] * forward[2] - right[2] * forward[1],
      right[2] * forward[0] - right[0] * forward[2],
      right[0] * forward[1] - right[1] * forward[0],
    ];
    return { position: [...camera.position], forward, right, up, fovDeg, aspect, near, far };
  }

  const yaw = Number.isFinite(camera.yaw) ? camera.yaw : 0;
  const pitch = Number.isFinite(camera.pitch) ? camera.pitch : 0.25;
  const roll = Number.isFinite(camera.roll) ? camera.roll : 0;
  const distance = Number.isFinite(camera.distance) && camera.distance > 0 ? camera.distance : 12;
  const target = Array.isArray(camera.target) && camera.target.length === 3 ? camera.target : [0, 0, 0];
  const { back, right, up } = orbitBasis({ yaw, pitch, roll });
  const position = target.map((t, i) => t + back[i] * distance);
  const forward = back.map((b) => -b);
  return { position, forward, right, up, fovDeg, aspect, near, far };
}

function entityWorldTransform(entities, id) {
  return cameraWorld(entities, id);
}

function entityLocalBounds(entity) {
  const b = entity?.components?.bounds || entity?.bounds;
  if (b && Array.isArray(b.min) && Array.isArray(b.max)) {
    return { min: b.min, max: b.max };
  }
  const half = Number.isFinite(entity?.radius) && entity.radius > 0 ? entity.radius : 0.5;
  return { min: [-half, -half, -half], max: [half, half, half] };
}

function transformAabbArvo(local, m) {
  const min = [m[12], m[13], m[14]];
  const max = [m[12], m[13], m[14]];
  for (let col = 0; col < 3; col += 1) {
    const lo = local.min[col];
    const hi = local.max[col];
    const base = col * 4;
    for (let row = 0; row < 3; row += 1) {
      const a = m[base + row] * lo;
      const b = m[base + row] * hi;
      if (a < b) {
        min[row] += a;
        max[row] += b;
      } else {
        min[row] += b;
        max[row] += a;
      }
    }
  }
  return { min, max };
}

/**
 * Evaluates frustum, max-distance, screen-space subpixel contribution culling, and 8-bin
 * front-to-back Early-Z depth ordering across a scene's entities.
 */
export function evaluateSceneCulling(entities = {}, rawCamera = {}, options = {}) {
  const cam = normalizeCameraFrustum(rawCamera);
  const maxDistance = Number.isFinite(options.maxDistance) && options.maxDistance > 0
    ? options.maxDistance
    : cam.far;
  const minScreenPx = Number.isFinite(options.minScreenPx) && options.minScreenPx >= 0
    ? options.minScreenPx
    : 1.5;
  const viewportHeightPx = Number.isFinite(options.viewportHeightPx) && options.viewportHeightPx > 0
    ? options.viewportHeightPx
    : 1080;

  const halfFovRad = (cam.fovDeg * Math.PI) / 360;
  const tanHalfY = Math.tan(halfFovRad);
  const tanHalfX = tanHalfY * cam.aspect;
  const projScaleY = 1 / Math.max(1e-6, tanHalfY);
  const secX = Math.hypot(1, tanHalfX);
  const secY = Math.hypot(1, tanHalfY);

  const items = [];
  let visibleCount = 0;
  let frustumCulledCount = 0;
  let distanceCulledCount = 0;
  let subpixelCulledCount = 0;

  for (const [id, entity] of Object.entries(entities || {})) {
    if (!entity || entity.components?.camera || entity.components?.light) continue;
    const world = entityWorldTransform(entities, id) || fromTRS(
      entity.components?.transform?.translation,
      entity.components?.transform?.rotation,
      entity.components?.transform?.scale,
    );
    const aabb = transformAabbArvo(entityLocalBounds(entity), world);
    const center = [
      0.5 * (aabb.min[0] + aabb.max[0]),
      0.5 * (aabb.min[1] + aabb.max[1]),
      0.5 * (aabb.min[2] + aabb.max[2]),
    ];
    const ext = [
      0.5 * Math.max(0, aabb.max[0] - aabb.min[0]),
      0.5 * Math.max(0, aabb.max[1] - aabb.min[1]),
      0.5 * Math.max(0, aabb.max[2] - aabb.min[2]),
    ];
    const radius = Math.hypot(ext[0], ext[1], ext[2]);

    const rel = [
      center[0] - cam.position[0],
      center[1] - cam.position[1],
      center[2] - cam.position[2],
    ];
    // View-space coordinates (x right, y up, z forward depth >= 0 in front of camera).
    const vx = rel[0] * cam.right[0] + rel[1] * cam.right[1] + rel[2] * cam.right[2];
    const vy = rel[0] * cam.up[0] + rel[1] * cam.up[1] + rel[2] * cam.up[2];
    const vz = rel[0] * cam.forward[0] + rel[1] * cam.forward[1] + rel[2] * cam.forward[2];
    const dist = Math.hypot(vx, vy, vz);

    const inNearFar = (vz + radius >= cam.near) && (vz - radius <= cam.far);
    const inLeftRight = Math.abs(vx) <= vz * tanHalfX + radius * secX;
    const inTopBottom = Math.abs(vy) <= vz * tanHalfY + radius * secY;
    const inFrustum = inNearFar && inLeftRight && inTopBottom;

    let status = 'visible';
    const projectedPx = vz > 1e-4 ? (radius * projScaleY * viewportHeightPx) / vz : viewportHeightPx;

    if (!inFrustum) {
      status = 'frustum-culled';
      frustumCulledCount += 1;
    } else if (dist - radius > maxDistance) {
      status = 'distance-culled';
      distanceCulledCount += 1;
    } else if (minScreenPx > 0 && projectedPx < minScreenPx) {
      status = 'subpixel-culled';
      subpixelCulledCount += 1;
    } else {
      visibleCount += 1;
    }

    const depthRange = Math.max(1e-3, Math.min(maxDistance, cam.far) - cam.near);
    const depthNorm = Math.min(1, Math.max(0, (vz - cam.near) / depthRange));
    const depthKeyU16 = Math.round(depthNorm * 65535);

    items.push({
      id,
      name: entity.name || id,
      status,
      viewX: vx,
      viewY: vy,
      viewZ: vz,
      distance: dist,
      radius,
      projectedPx,
      depthKeyU16,
    });
  }

  // Front-to-back Early-Z ordering among visible items.
  const visibleSorted = items
    .filter((it) => it.status === 'visible')
    .sort((a, b) => a.depthKeyU16 - b.depthKeyU16 || a.id.localeCompare(b.id));

  const earlyZBins = new Array(8).fill(0);
  for (const it of visibleSorted) {
    const bin = Math.min(7, Math.floor((it.depthKeyU16 / 65536) * 8));
    earlyZBins[bin] += 1;
  }

  return {
    camera: cam,
    maxDistance,
    minScreenPx,
    total: items.length,
    visible: visibleCount,
    frustumCulled: frustumCulledCount,
    distanceCulled: distanceCulledCount,
    subpixelCulled: subpixelCulledCount,
    distanceOrSubpixelCulled: distanceCulledCount + subpixelCulledCount,
    earlyZBins,
    visibleOrder: visibleSorted.map((it) => it.id),
    items,
  };
}

function renderSparklineSvg(ring) {
  const width = 360;
  const height = 84;
  const maxMs = 33.34;
  const yOf = (ms) => height - Math.min(1, Math.max(0, ms / maxMs)) * (height - 10) - 5;

  const y60 = yOf(BUDGET_60HZ_MS).toFixed(1);
  const y120 = yOf(BUDGET_120HZ_MS).toFixed(1);

  let cpuPts = '';
  let gpuPts = '';
  if (ring.count > 0) {
    const start = ring.count === ring.capacity ? ring.head : 0;
    const denom = Math.max(1, ring.capacity - 1);
    for (let i = 0; i < ring.count; i += 1) {
      const idx = (start + i) % ring.capacity;
      const x = ((i / denom) * width).toFixed(1);
      cpuPts += `${x},${yOf(ring.cpuMs[idx]).toFixed(1)} `;
      gpuPts += `${x},${yOf(ring.gpuMs[idx]).toFixed(1)} `;
    }
  }

  return `<svg viewBox="0 0 ${width} ${height}" width="100%" height="${height}" role="img" aria-label="CPU and GPU frame times">
    <rect x="0" y="0" width="${width}" height="${height}" rx="6" fill="#0d1117" stroke="#263040"/>
    <line x1="0" y1="${y60}" x2="${width}" y2="${y60}" stroke="#3fb950" stroke-dasharray="4 3" stroke-width="1"/>
    <line x1="0" y1="${y120}" x2="${width}" y2="${y120}" stroke="#58a6ff" stroke-dasharray="2 3" stroke-width="1" opacity="0.7"/>
    ${gpuPts ? `<polyline fill="none" stroke="#f0883e" stroke-width="1.8" points="${gpuPts.trim()}"/>` : ''}
    ${cpuPts ? `<polyline fill="none" stroke="#39c5cf" stroke-width="1.8" points="${cpuPts.trim()}"/>` : ''}
  </svg>`;
}

function renderFrustumConeSvg(cull) {
  const size = 180;
  const cx = size * 0.5;
  const cy = size - 18;
  const range = Math.max(10, Math.min(cull.maxDistance, cull.camera.far));
  const scale = (size - 32) / range;
  const halfFov = (cull.camera.fovDeg * Math.PI) / 360;
  const tanX = Math.tan(halfFov) * cull.camera.aspect;
  const coneX = Math.min(cx - 8, range * tanX * scale);
  const coneY = cy - range * scale;

  const dots = cull.items.slice(0, 200).map((it) => {
    const px = Math.max(6, Math.min(size - 6, cx + it.viewX * scale));
    const py = Math.max(6, Math.min(size - 6, cy - it.viewZ * scale));
    const color = it.status === 'visible'
      ? '#3fb950'
      : (it.status === 'frustum-culled' ? '#6e7681' : '#d29922');
    return `<circle cx="${px.toFixed(1)}" cy="${py.toFixed(1)}" r="3" fill="${color}"/>`;
  }).join('');

  return `<svg viewBox="0 0 ${size} ${size}" width="${size}" height="${size}" role="img" aria-label="Top-down frustum cone">
    <rect x="0" y="0" width="${size}" height="${size}" rx="6" fill="#0d1117" stroke="#263040"/>
    <polygon points="${cx},${cy} ${(cx - coneX).toFixed(1)},${coneY.toFixed(1)} ${(cx + coneX).toFixed(1)},${coneY.toFixed(1)}" fill="rgba(56,139,253,0.14)" stroke="#58a6ff" stroke-width="1.2"/>
    ${dots}
    <circle cx="${cx}" cy="${cy}" r="4" fill="#f0f6fc"/>
  </svg>`;
}

function extractDocumentTextures(assets = []) {
  const texAssets = (assets || []).filter((a) => a && (a.kind === 'texture' || /\.(png|ktx2|dds|webp|jpg)$/i.test(a.path || '')));
  if (texAssets.length === 0) {
    return [
      { width: 2048, height: 2048, mips: true },
      { width: 2048, height: 2048, mips: true },
      { width: 1024, height: 1024, mips: true },
    ];
  }
  return texAssets.map((a) => {
    const approxPixels = Math.max(64 * 64, Math.round((a.bytes || 1048576) / 0.75));
    const side = Math.min(4096, Math.max(64, 1 << Math.round(Math.log2(Math.sqrt(approxPixels)))));
    return { width: side, height: side, mips: true };
  });
}

function readActiveCameraFromSnapshot(snapshot) {
  const selCam = snapshot?.selection?.activeCamera;
  if (selCam && typeof selCam === 'object') return selCam;
  const entities = snapshot?.activeDocument?.root?.entities;
  if (entities && typeof entities === 'object') {
    for (const [id, ent] of Object.entries(entities)) {
      if (ent?.components?.camera) {
        const w = cameraWorld(entities, id);
        if (w) {
          const position = point(w, [0, 0, 0]);
          const forward = [-w[8], -w[9], -w[10]];
          const up = [w[4], w[5], w[6]];
          return {
            position,
            forward,
            up,
            fov: ent.components.camera.fov ?? 50,
            near: ent.components.camera.near ?? 0.1,
            far: ent.components.camera.far ?? 500,
          };
        }
      }
    }
  }
  return { yaw: 0.4, pitch: 0.25, distance: 14, target: [0, 0, 0], fov: 50, near: 0.1, far: 500 };
}

/**
 * Mounts the standalone Telemetry & Culling Profiler into `container`.
 * Implements `StudioWorkbenchMountFn` (`editor/plugin.d.ts`).
 */
export function mountTelemetryProfiler(container, studioApi) {
  if (!container) throw new Error('mountTelemetryProfiler requires a container element');

  const ring = createFrameRing(120);
  let paused = false;
  let frozenCamera = null;
  let maxDistance = 250;
  let minScreenPx = 1.5;
  let active = true;
  const unsubs = [];

  function currentSnapshot() {
    return studioApi?.getState?.() || {
      locale: studioApi?.getLocale?.() || 'en',
      assets: studioApi?.getAssets?.() || [],
      activeDocument: studioApi?.getActiveDocument?.() || null,
      selection: studioApi?.getSelection?.() || { entityIds: [], activeCamera: null },
      stats: studioApi?.getViewport?.()?.getStats?.() || {
        fps: 0,
        frameTimeMs: 0,
        gpuTimeMs: 0,
        draws: 0,
        triangles: 0,
        vramBytes: 0,
        backend: 'unknown',
      },
    };
  }

  function render(snapshot = currentSnapshot()) {
    const locale = snapshot?.locale === 'ko' ? 'ko' : 'en';
    const L = STRINGS[locale];
    const stats = snapshot?.stats || studioApi?.getViewport?.()?.getStats?.() || {};
    if (!paused && (stats.frameTimeMs > 0 || stats.gpuTimeMs > 0 || stats.fps > 0)) {
      pushFrameSample(ring, stats);
    }

    const summary = computeFramePercentiles(ring);
    const vram = computeVramCompressionBudget(extractDocumentTextures(snapshot?.assets));
    const engine = studioApi?.getEngine?.() || null;
    const bundleStats = typeof engine?.bundleStats === 'function'
      ? engine.bundleStats()
      : { cachedPasses: 0, hits: 0, misses: 0 };
    const compressedCaps = engine?.compressedFormats || {};

    const liveCam = readActiveCameraFromSnapshot(snapshot);
    const camForCull = frozenCamera || liveCam;
    const entities = snapshot?.activeDocument?.root?.entities || {};
    const cull = evaluateSceneCulling(entities, camForCull, {
      maxDistance,
      minScreenPx,
      viewportHeightPx: 1080,
    });

    const maxBin = Math.max(1, ...cull.earlyZBins);
    const binBars = cull.earlyZBins.map((count, idx) => {
      const pct = Math.round((count / maxBin) * 100);
      return `<div class="tp-bin" title="Bin ${idx + 1}: ${count}">
        <div class="tp-bin-bar" style="height:${Math.max(4, pct)}%"></div>
        <span>${count}</span>
      </div>`;
    }).join('');

    const html = `<div class="gb-telemetry-profiler" data-frozen="${frozenCamera ? 'true' : 'false'}" style="font-family:system-ui,sans-serif;color:#e6edf3;background:#090d13;padding:12px;display:grid;gap:12px;">
      <header style="display:flex;align-items:center;justify-content:space-between;gap:8px;flex-wrap:wrap;">
        <div>
          <strong style="font-size:14px;">${esc(L.title)}</strong>
          <span style="font-size:11px;color:#8b949e;margin-left:8px;">Backend: ${esc(stats.backend || 'native/web')} · ${esc(summary.bottleneck)}</span>
        </div>
        <div style="display:flex;gap:6px;">
          <button type="button" data-tp-action="toggle-pause">${esc(paused ? L.resume : L.pause)}</button>
          <button type="button" data-tp-action="clear-ring">${esc(L.reset)}</button>
          <button type="button" data-tp-action="toggle-freeze">${esc(frozenCamera ? L.unfreezeCam : L.freezeCam)}</button>
        </div>
      </header>

      <section data-tp-section="framegraph" style="background:#111722;border:1px solid #222c3c;border-radius:8px;padding:10px;display:grid;gap:8px;">
        <div style="display:flex;justify-content:space-between;font-size:12px;">
          <span>${esc(L.framegraph)}</span>
          <span><strong>${summary.fps.toFixed(1)} FPS</strong> · CPU p50 ${summary.cpu.p50.toFixed(2)} ms / p95 ${summary.cpu.p95.toFixed(2)} ms / p99 ${summary.cpu.p99.toFixed(2)} ms · GPU p50 ${summary.gpu.p50.toFixed(2)} ms / p95 ${summary.gpu.p95.toFixed(2)} ms</span>
        </div>
        ${renderSparklineSvg(ring)}
      </section>

      <section data-tp-section="vram" style="background:#111722;border:1px solid #222c3c;border-radius:8px;padding:10px;display:grid;gap:6px;font-size:12px;">
        <div style="display:flex;justify-content:space-between;flex-wrap:wrap;gap:6px;">
          <strong>${esc(L.vramHud)}</strong>
          <span>Draws: <strong>${summary.draws}</strong> · Tris: <strong>${summary.triangles}</strong> · RenderBundles: <strong>${bundleStats.cachedPasses}</strong> (${bundleStats.hits} hits)</span>
        </div>
        <div style="display:grid;grid-template-columns:repeat(auto-fit,minmax(135px,1fr));gap:8px;">
          <div>RGBA8 Uncompressed: <strong>${fmtBytes(vram.rgba8Bytes)}</strong> (1.0x)</div>
          <div>BC1 Hardware: <strong>${fmtBytes(vram.bc1Bytes)}</strong> (${vram.bc1Ratio.toFixed(1)}x smaller)</div>
          <div>BC7 Hardware: <strong>${fmtBytes(vram.bc7Bytes)}</strong> (${vram.bc7Ratio.toFixed(1)}x smaller)</div>
          <div>ASTC 4x4 Mobile: <strong>${fmtBytes(vram.astc4x4Bytes)}</strong> (${vram.astcRatio.toFixed(1)}x smaller)</div>
        </div>
        <div style="font-size:11px;color:#8b949e;">
          GPU HW Formats: BC=${compressedCaps.bc ? 'yes' : 'fallback'} · ASTC=${compressedCaps.astc ? 'yes' : 'fallback'} · Runtime VRAM=${fmtBytes(stats.vramBytes || 0)}
        </div>
      </section>

      <section data-tp-section="culling" style="background:#111722;border:1px solid #222c3c;border-radius:8px;padding:10px;display:grid;gap:8px;font-size:12px;">
        <div style="display:flex;justify-content:space-between;flex-wrap:wrap;gap:8px;">
          <strong>${esc(L.cullTitle)} ${frozenCamera ? '(FROZEN)' : '(LIVE)'}</strong>
          <span>${esc(L.visible)}: <strong>${cull.visible}</strong> / ${cull.total} · ${esc(L.frustumCulled)}: <strong>${cull.frustumCulled}</strong> · ${esc(L.distSubpxCulled)}: <strong>${cull.distanceOrSubpixelCulled}</strong></span>
        </div>
        <div style="display:flex;gap:12px;align-items:center;flex-wrap:wrap;">
          <label>${esc(L.maxDist)} <input type="number" data-tp-input="max-distance" value="${maxDistance}" min="5" max="5000" step="10" style="width:72px;"/></label>
          <label>${esc(L.minPx)} <input type="number" data-tp-input="min-px" value="${minScreenPx}" min="0" max="64" step="0.5" style="width:64px;"/></label>
        </div>
        <div style="display:flex;gap:12px;align-items:center;flex-wrap:wrap;">
          ${renderFrustumConeSvg(cull)}
          <div style="flex:1;min-width:180px;">
            <div style="margin-bottom:4px;color:#8b949e;">${esc(L.earlyZ)}</div>
            <div style="display:grid;grid-template-columns:repeat(8,1fr);gap:4px;height:68px;align-items:end;">${binBars}</div>
          </div>
        </div>
      </section>
    </div>`;

    container.innerHTML = html;
  }

  function onClick(event) {
    const btn = event?.target?.closest?.('[data-tp-action]');
    if (!btn) return;
    const action = btn.getAttribute('data-tp-action');
    if (action === 'toggle-pause') {
      paused = !paused;
      render();
    } else if (action === 'clear-ring') {
      clearFrameRing(ring);
      render();
    } else if (action === 'toggle-freeze') {
      frozenCamera = frozenCamera ? null : normalizeCameraFrustum(readActiveCameraFromSnapshot(currentSnapshot()));
      studioApi?.notify?.('info', frozenCamera ? 'Camera frustum frozen for culling inspection' : 'Camera frustum unfrozen');
      render();
    }
  }

  function onInput(event) {
    const input = event?.target?.closest?.('[data-tp-input]');
    if (!input) return;
    const kind = input.getAttribute('data-tp-input');
    const val = Number(input.value);
    if (!Number.isFinite(val)) return;
    if (kind === 'max-distance' && val > 0) {
      maxDistance = val;
      render();
    } else if (kind === 'min-px' && val >= 0) {
      minScreenPx = val;
      render();
    }
  }

  container.addEventListener?.('click', onClick);
  container.addEventListener?.('change', onInput);

  if (typeof studioApi?.subscribe === 'function') {
    unsubs.push(studioApi.subscribe((snap) => {
      if (active) render(snap);
    }));
  } else if (typeof studioApi?.on === 'function') {
    unsubs.push(studioApi.on('stats', () => {
      if (active) render();
    }));
  }

  render();

  const instance = {
    refresh(snapshot) {
      if (active) render(snapshot);
    },
    onActivate() {
      active = true;
      render();
    },
    onDeactivate() {
      active = false;
    },
    freezeCamera(customCam) {
      frozenCamera = normalizeCameraFrustum(customCam || readActiveCameraFromSnapshot(currentSnapshot()));
      render();
      return frozenCamera;
    },
    unfreezeCamera() {
      frozenCamera = null;
      render();
    },
    isCameraFrozen() {
      return frozenCamera !== null;
    },
    setThresholds(next = {}) {
      if (Number.isFinite(next.maxDistance) && next.maxDistance > 0) maxDistance = next.maxDistance;
      if (Number.isFinite(next.minScreenPx) && next.minScreenPx >= 0) minScreenPx = next.minScreenPx;
      render();
    },
    getRingSummary() {
      return computeFramePercentiles(ring);
    },
    destroy() {
      active = false;
      while (unsubs.length > 0) {
        const off = unsubs.pop();
        if (typeof off === 'function') off();
      }
      container.removeEventListener?.('click', onClick);
      container.removeEventListener?.('change', onInput);
      container.innerHTML = '';
    },
    unmount() {
      instance.destroy();
    },
  };

  return instance;
}
