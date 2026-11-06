// Ganbate Engine — Landscape, Terrain Shaper, Spline Walkways, Water & Sky Cycle Workbench.
// Genre-neutral 3D heightfield sculpting (`1 unit = 1m`), 4-layer splatmap blending,
// Catmull-Rom spline walkways, deterministic PCG edge scatter, water plane, and AI digest.

export const TERRAIN_BRUSH_MODES = Object.freeze([
  { id: 'raise', label: 'Raise', icon: '▲' },
  { id: 'lower', label: 'Lower', icon: '▼' },
  { id: 'smooth', label: 'Smooth', icon: '≈' },
  { id: 'flatten', label: 'Flatten', icon: '▬' },
  { id: 'terrace', label: 'Terrace', icon: '🪜' },
  { id: 'spline', label: 'Walkway', icon: '〰' },
]);

export const TERRAIN_SKY_PRESETS = Object.freeze({
  dawn: Object.freeze({
    id: 'dawn',
    label: 'Dawn',
    timeHours: 6.0,
    weather: 'clear',
    sunDir: [0.68, 0.28, -0.68],
    skyTop: '#24345a',
    skyHorizon: '#e5956e',
    ambientColor: [0.75, 0.62, 0.68],
    sunColor: [1.0, 0.76, 0.58],
  }),
  midday: Object.freeze({
    id: 'midday',
    label: 'Midday',
    timeHours: 13.0,
    weather: 'clear',
    sunDir: [0.32, 0.88, -0.35],
    skyTop: '#295896',
    skyHorizon: '#94c2e8',
    ambientColor: [0.78, 0.84, 0.92],
    sunColor: [1.0, 0.98, 0.92],
  }),
  dusk: Object.freeze({
    id: 'dusk',
    label: 'Dusk',
    timeHours: 17.5,
    weather: 'clear',
    sunDir: [-0.64, 0.31, -0.70],
    skyTop: '#1c223d',
    skyHorizon: '#d97854',
    ambientColor: [0.76, 0.58, 0.64],
    sunColor: [1.0, 0.68, 0.44],
  }),
  midnight: Object.freeze({
    id: 'midnight',
    label: 'Midnight',
    timeHours: 0.5,
    weather: 'clear',
    sunDir: [0.22, 0.75, 0.62],
    skyTop: '#090d18',
    skyHorizon: '#1a2744',
    ambientColor: [0.34, 0.42, 0.62],
    sunColor: [0.52, 0.66, 0.95],
  }),
});

export const DEFAULT_SPLAT_LAYERS = Object.freeze([
  Object.freeze({
    id: 'grass',
    name: 'Lush Moss & Grass',
    color: [0.24, 0.46, 0.26],
    hex: '#3d7542',
    roughness: 0.78,
    minHeight: -1.0,
    maxHeight: 14.0,
    maxSlope: 0.52,
  }),
  Object.freeze({
    id: 'stone',
    name: 'Paved Flagstone',
    color: [0.52, 0.50, 0.48],
    hex: '#85807a',
    roughness: 0.62,
    minHeight: -4.0,
    maxHeight: 24.0,
    maxSlope: 0.75,
  }),
  Object.freeze({
    id: 'sand',
    name: 'Shoreline Gravel & Sand',
    color: [0.76, 0.69, 0.54],
    hex: '#c2b08a',
    roughness: 0.82,
    minHeight: -6.0,
    maxHeight: 1.35,
    maxSlope: 0.48,
  }),
  Object.freeze({
    id: 'cliff',
    name: 'Basalt Cliff Rock',
    color: [0.33, 0.31, 0.35],
    hex: '#544f59',
    roughness: 0.88,
    minHeight: -20.0,
    maxHeight: 50.0,
    maxSlope: 1.0,
  }),
]);

export const TERRAIN_LANDSCAPE_PRESETS = Object.freeze({
  sanctuary_garden: Object.freeze({
    id: 'sanctuary_garden',
    label: 'Evening Garden Sanctuary',
    waterLevel: -0.25,
    skyPreset: 'dusk',
    amplitude: 3.4,
    basinDepth: 1.8,
    terraceSteps: 0,
  }),
  alpine_valley: Object.freeze({
    id: 'alpine_valley',
    label: 'Alpine Valley',
    waterLevel: -0.6,
    skyPreset: 'midday',
    amplitude: 6.5,
    basinDepth: 2.2,
    terraceSteps: 0,
  }),
  coastal_archipelago: Object.freeze({
    id: 'coastal_archipelago',
    label: 'Coastal Archipelago',
    waterLevel: 0.45,
    skyPreset: 'dawn',
    amplitude: 4.2,
    basinDepth: 1.2,
    terraceSteps: 0,
  }),
  terraced_canyon: Object.freeze({
    id: 'terraced_canyon',
    label: 'Terraced Canyon',
    waterLevel: -0.8,
    skyPreset: 'dusk',
    amplitude: 5.8,
    basinDepth: 1.5,
    terraceSteps: 1.25,
  }),
});

function clamp(v, lo, hi) {
  return Math.max(lo, Math.min(hi, v));
}

function round3(v) {
  return Math.round(Number(v || 0) * 1000) / 1000;
}

function deterministicHash(seed) {
  let h = (Number(seed) || 1) >>> 0;
  h ^= h >>> 16;
  h = Math.imul(h, 0x7feb352d);
  h ^= h >>> 15;
  h = Math.imul(h, 0x846ca68b);
  h ^= h >>> 16;
  return (h >>> 0) / 4294967296;
}

export function gridIndex(cols, col, row) {
  return row * cols + col;
}

export function gridCellToWorld(grid, col, row) {
  const cols = Math.max(2, Number(grid?.cols) || 32);
  const rows = Math.max(2, Number(grid?.rows) || 32);
  const cellSize = Math.max(0.25, Number(grid?.cellSize) || 1);
  const halfW = ((cols - 1) * cellSize) * 0.5;
  const halfD = ((rows - 1) * cellSize) * 0.5;
  return [
    col * cellSize - halfW,
    Number(grid?.heights?.[row * cols + col]) || 0,
    row * cellSize - halfD,
  ];
}

export function worldToGridCell(grid, wx, wz) {
  const cols = Math.max(2, Number(grid?.cols) || 32);
  const rows = Math.max(2, Number(grid?.rows) || 32);
  const cellSize = Math.max(0.25, Number(grid?.cellSize) || 1);
  const halfW = ((cols - 1) * cellSize) * 0.5;
  const halfD = ((rows - 1) * cellSize) * 0.5;
  const col = clamp((wx + halfW) / cellSize, 0, cols - 1);
  const row = clamp((wz + halfD) / cellSize, 0, rows - 1);
  return { col, row };
}

export function sampleHeightAtWorld(grid, wx, wz) {
  const cols = Math.max(2, Number(grid?.cols) || 32);
  const rows = Math.max(2, Number(grid?.rows) || 32);
  const heights = grid?.heights || [];
  const { col, row } = worldToGridCell(grid, wx, wz);
  const c0 = Math.floor(col);
  const r0 = Math.floor(row);
  const c1 = Math.min(cols - 1, c0 + 1);
  const r1 = Math.min(rows - 1, r0 + 1);
  const tx = col - c0;
  const tz = row - r0;
  const h00 = Number(heights[r0 * cols + c0]) || 0;
  const h10 = Number(heights[r0 * cols + c1]) || 0;
  const h01 = Number(heights[r1 * cols + c0]) || 0;
  const h11 = Number(heights[r1 * cols + c1]) || 0;
  const h0 = h00 * (1 - tx) + h10 * tx;
  const h1 = h01 * (1 - tx) + h11 * tx;
  return h0 * (1 - tz) + h1 * tz;
}

export function computeVertexNormal(grid, col, row) {
  const cols = Math.max(2, Number(grid?.cols) || 32);
  const rows = Math.max(2, Number(grid?.rows) || 32);
  const cellSize = Math.max(0.25, Number(grid?.cellSize) || 1);
  const heights = grid?.heights || [];
  const cL = Math.max(0, col - 1);
  const cR = Math.min(cols - 1, col + 1);
  const rD = Math.max(0, row - 1);
  const rU = Math.min(rows - 1, row + 1);
  const hL = Number(heights[row * cols + cL]) || 0;
  const hR = Number(heights[row * cols + cR]) || 0;
  const hD = Number(heights[rD * cols + col]) || 0;
  const hU = Number(heights[rU * cols + col]) || 0;
  const dx = (hR - hL) / ((cR - cL || 1) * cellSize);
  const dz = (hU - hD) / ((rU - rD || 1) * cellSize);
  const nx = -dx;
  const ny = 1.0;
  const nz = -dz;
  const len = Math.hypot(nx, ny, nz) || 1;
  return [nx / len, ny / len, nz / len];
}

export function createDefaultTerrainDoc(name = 'Sanctuary Landscape', presetId = 'sanctuary_garden', options = {}) {
  const preset = TERRAIN_LANDSCAPE_PRESETS[presetId] || TERRAIN_LANDSCAPE_PRESETS.sanctuary_garden;
  const cols = clamp(Math.round(Number(options.cols) || 48), 8, 128);
  const rows = clamp(Math.round(Number(options.rows) || 48), 8, 128);
  const cellSize = Math.max(0.25, Number(options.cellSize) || 1.0);
  const heights = new Array(cols * rows).fill(0);
  const halfW = ((cols - 1) * cellSize) * 0.5;
  const halfD = ((rows - 1) * cellSize) * 0.5;

  let minH = Infinity;
  let maxH = -Infinity;

  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) {
      const wx = c * cellSize - halfW;
      const wz = r * cellSize - halfD;
      const dist = Math.hypot(wx, wz);
      const normDist = dist / Math.max(1, Math.hypot(halfW, halfD));
      // Gentle amphitheater hills around a central stage + pond basin
      const ridge =
        Math.sin(wx * 0.14) * Math.cos(wz * 0.12) * preset.amplitude * 0.55 +
        Math.cos((wx + wz) * 0.09) * preset.amplitude * 0.35 +
        Math.pow(clamp(normDist - 0.22, 0, 1), 1.6) * preset.amplitude * 0.95;
      // Central flat stage plateau around (0, 0) and pond depression around (0, -7.5)
      const stageFlat = Math.exp(-(wx * wx + wz * wz) / 38.0);
      const pondDistSq = wx * wx + (wz + 7.5) * (wz + 7.5);
      const pondBasin = Math.exp(-pondDistSq / 26.0) * preset.basinDepth;
      let h = ridge * (1 - stageFlat * 0.85) - pondBasin;
      if (preset.terraceSteps > 0.05) {
        h = Math.round(h / preset.terraceSteps) * preset.terraceSteps;
      }
      h = round3(h);
      heights[r * cols + c] = h;
      if (h < minH) minH = h;
      if (h > maxH) maxH = h;
    }
  }

  const grid = {
    cols,
    rows,
    cellSize,
    minHeight: Number.isFinite(minH) ? minH : 0,
    maxHeight: Number.isFinite(maxH) ? maxH : 0,
    heights,
  };

  const defaultSplines = [
    {
      id: 'walkway-main',
      name: 'Main Stone Walkway',
      width: 3.2,
      falloff: 2.0,
      flattenStrength: 0.85,
      layerIndex: 1, // Stone
      points: [
        [-14, sampleHeightAtWorld(grid, -14, 10), 10],
        [-5, sampleHeightAtWorld(grid, -5, 4), 4],
        [0, 0.15, 0],
        [6, sampleHeightAtWorld(grid, 6, -4), -4],
        [14, sampleHeightAtWorld(grid, 14, -10), -10],
      ],
    },
  ];

  const sky = TERRAIN_SKY_PRESETS[preset.skyPreset] || TERRAIN_SKY_PRESETS.dusk;
  const water = {
    enabled: true,
    level: preset.waterLevel,
    shallowColor: [0.16, 0.44, 0.56, 0.72],
    deepColor: [0.06, 0.18, 0.30, 0.88],
    foamWidth: 1.2,
  };

  const graded = applySplineWalkways(grid, defaultSplines);
  grid.heights = graded.nextHeights;
  grid.minHeight = graded.minHeight;
  grid.maxHeight = graded.maxHeight;

  const scatter = generateSplineEdgeScatter(grid, defaultSplines, water, options.assets || [], 1337);

  return {
    ganbate: 'terrain',
    version: 1,
    id: options.id || 'terrain_default_01',
    name,
    preset: preset.id,
    grid,
    splatLayers: DEFAULT_SPLAT_LAYERS.map((l) => ({ ...l, color: [...l.color] })),
    water,
    sky: {
      preset: sky.id,
      timeHours: sky.timeHours,
      weather: sky.weather,
    },
    splines: defaultSplines,
    scatter,
  };
}

function brushFalloffWeight(normDist, falloff = 'smooth') {
  if (normDist >= 1) return 0;
  if (normDist <= 0) return 1;
  if (falloff === 'linear') return 1 - normDist;
  if (falloff === 'spherical') return Math.sqrt(Math.max(0, 1 - normDist * normDist));
  // Smooth Hermite (`smoothstep`)
  const t = 1 - normDist;
  return t * t * (3 - 2 * t);
}

/**
 * Applies a terrain sculpting brush (`raise`, `lower`, `smooth`, `flatten`, `terrace`) at world
 * coordinates `(x, z)` (`1 unit = 1m`). Pure function returning updated heights and bounds.
 */
export function applyTerrainBrush(grid, brush = {}) {
  const cols = Math.max(2, Number(grid?.cols) || 32);
  const rows = Math.max(2, Number(grid?.rows) || 32);
  const cellSize = Math.max(0.25, Number(grid?.cellSize) || 1);
  const srcHeights = Array.isArray(grid?.heights) ? grid.heights : new Array(cols * rows).fill(0);
  const nextHeights = srcHeights.slice();

  const mode = brush.mode || 'raise';
  const wx = Number(brush.x) || 0;
  const wz = Number(brush.z) || 0;
  const radius = clamp(Number(brush.radius) || 4.0, 0.5, 64.0);
  const strength = clamp(Number(brush.strength) ?? 0.5, 0.01, 1.0);
  const falloff = brush.falloff || 'smooth';
  const targetHeight = Number.isFinite(brush.targetHeight)
    ? Number(brush.targetHeight)
    : sampleHeightAtWorld(grid, wx, wz);
  const terraceStep = Math.max(0.2, Number(brush.terraceStep) || 1.0);

  const halfW = ((cols - 1) * cellSize) * 0.5;
  const halfD = ((rows - 1) * cellSize) * 0.5;

  let affectedVertices = 0;
  let changed = false;

  for (let r = 0; r < rows; r++) {
    const vz = r * cellSize - halfD;
    if (Math.abs(vz - wz) > radius) continue;
    for (let c = 0; c < cols; c++) {
      const vx = c * cellSize - halfW;
      const dist = Math.hypot(vx - wx, vz - wz);
      if (dist > radius) continue;

      const w = brushFalloffWeight(dist / radius, falloff);
      if (w <= 1e-5) continue;

      const idx = r * cols + c;
      const cur = Number(srcHeights[idx]) || 0;
      let next = cur;

      if (mode === 'raise') {
        next = cur + strength * w * 1.25;
      } else if (mode === 'lower') {
        next = cur - strength * w * 1.25;
      } else if (mode === 'flatten') {
        next = cur + (targetHeight - cur) * clamp(strength * w, 0, 1);
      } else if (mode === 'terrace') {
        const quantized = Math.round(cur / terraceStep) * terraceStep;
        next = cur + (quantized - cur) * clamp(strength * w, 0, 1);
      } else if (mode === 'smooth') {
        let sum = 0;
        let count = 0;
        for (let dr = -1; dr <= 1; dr++) {
          const nr = r + dr;
          if (nr < 0 || nr >= rows) continue;
          for (let dc = -1; dc <= 1; dc++) {
            const nc = c + dc;
            if (nc < 0 || nc >= cols) continue;
            sum += Number(srcHeights[nr * cols + nc]) || 0;
            count++;
          }
        }
        const avg = count > 0 ? sum / count : cur;
        next = cur + (avg - cur) * clamp(strength * w, 0, 1);
      }

      next = round3(clamp(next, -60, 120));
      if (Math.abs(next - cur) > 1e-4) {
        nextHeights[idx] = next;
        changed = true;
        affectedVertices++;
      }
    }
  }

  let minHeight = Infinity;
  let maxHeight = -Infinity;
  for (let i = 0; i < nextHeights.length; i++) {
    const h = Number(nextHeights[i]) || 0;
    if (h < minHeight) minHeight = h;
    if (h > maxHeight) maxHeight = h;
  }

  return {
    changed,
    nextHeights,
    minHeight: Number.isFinite(minHeight) ? minHeight : 0,
    maxHeight: Number.isFinite(maxHeight) ? maxHeight : 0,
    affectedVertices,
  };
}

/**
 * Evaluates a Catmull-Rom spline through 3D control points `[[x, y, z], ...]`.
 */
export function evaluateCatmullRomSpline(points = [], samplesPerSegment = 10) {
  if (!Array.isArray(points) || points.length === 0) return [];
  if (points.length === 1) return [[...points[0]]];
  const out = [];
  const n = points.length;
  const steps = Math.max(2, Math.round(samplesPerSegment));

  for (let i = 0; i < n - 1; i++) {
    const p0 = points[Math.max(0, i - 1)];
    const p1 = points[i];
    const p2 = points[i + 1];
    const p3 = points[Math.min(n - 1, i + 2)];
    for (let s = 0; s < steps; s++) {
      const t = s / steps;
      const t2 = t * t;
      const t3 = t2 * t;
      const pt = [0, 1, 2].map((axis) => {
        const v0 = Number(p0[axis]) || 0;
        const v1 = Number(p1[axis]) || 0;
        const v2 = Number(p2[axis]) || 0;
        const v3 = Number(p3[axis]) || 0;
        return 0.5 * (
          (2 * v1) +
          (-v0 + v2) * t +
          (2 * v0 - 5 * v1 + 4 * v2 - v3) * t2 +
          (-v0 + 3 * v1 - 3 * v2 + v3) * t3
        );
      });
      out.push(pt);
    }
  }
  out.push([...points[n - 1]]);
  return out;
}

/**
 * Grades/flattens the terrain heightfield along each spline walkway corridor and returns
 * `{ nextHeights, pathMask, minHeight, maxHeight }` where `pathMask[idx]` is `{ weight, layerIndex }`.
 */
export function applySplineWalkways(grid, splines = []) {
  const cols = Math.max(2, Number(grid?.cols) || 32);
  const rows = Math.max(2, Number(grid?.rows) || 32);
  const cellSize = Math.max(0.25, Number(grid?.cellSize) || 1);
  const nextHeights = Array.isArray(grid?.heights) ? grid.heights.slice() : new Array(cols * rows).fill(0);
  const pathMask = new Array(cols * rows).fill(null);

  const halfW = ((cols - 1) * cellSize) * 0.5;
  const halfD = ((rows - 1) * cellSize) * 0.5;

  for (const spline of splines || []) {
    const pts = Array.isArray(spline?.points) ? spline.points : [];
    if (pts.length < 2) continue;
    const sampled = evaluateCatmullRomSpline(pts, 12);
    const halfWidth = Math.max(0.5, (Number(spline.width) || 3.0) * 0.5);
    const falloff = Math.max(0.25, Number(spline.falloff) || 1.8);
    const flattenStrength = clamp(Number(spline.flattenStrength) ?? 0.85, 0, 1);
    const layerIndex = clamp(Math.round(Number(spline.layerIndex) ?? 1), 0, 3);
    const maxDist = halfWidth + falloff;

    for (let r = 0; r < rows; r++) {
      const vz = r * cellSize - halfD;
      for (let c = 0; c < cols; c++) {
        const vx = c * cellSize - halfW;
        let bestDist = Infinity;
        let bestTargetY = 0;

        for (let i = 0; i < sampled.length - 1; i++) {
          const a = sampled[i];
          const b = sampled[i + 1];
          const abx = b[0] - a[0];
          const abz = b[2] - a[2];
          const lenSq = abx * abx + abz * abz || 1e-6;
          const t = clamp(((vx - a[0]) * abx + (vz - a[2]) * abz) / lenSq, 0, 1);
          const px = a[0] + abx * t;
          const pz = a[2] + abz * t;
          const d = Math.hypot(vx - px, vz - pz);
          if (d < bestDist) {
            bestDist = d;
            bestTargetY = a[1] + (b[1] - a[1]) * t;
          }
        }

        if (bestDist <= maxDist) {
          const idx = r * cols + c;
          const curY = Number(nextHeights[idx]) || 0;
          const w = bestDist <= halfWidth
            ? 1.0
            : brushFalloffWeight((bestDist - halfWidth) / falloff, 'smooth');
          const blended = curY + (bestTargetY - curY) * (w * flattenStrength);
          nextHeights[idx] = round3(blended);
          const prevMask = pathMask[idx];
          if (!prevMask || w > prevMask.weight) {
            pathMask[idx] = { weight: round3(w), layerIndex };
          }
        }
      }
    }
  }

  let minHeight = Infinity;
  let maxHeight = -Infinity;
  for (let i = 0; i < nextHeights.length; i++) {
    const h = Number(nextHeights[i]) || 0;
    if (h < minHeight) minHeight = h;
    if (h > maxHeight) maxHeight = h;
  }

  return {
    nextHeights,
    pathMask,
    minHeight: Number.isFinite(minHeight) ? minHeight : 0,
    maxHeight: Number.isFinite(maxHeight) ? maxHeight : 0,
  };
}

/**
 * Computes normalized 4-layer splatmap weights `[grass, stone, sand, cliff]` for each cell.
 */
export function computeSplatmapWeights(grid, layers = DEFAULT_SPLAT_LAYERS, water = {}, splines = []) {
  const cols = Math.max(2, Number(grid?.cols) || 32);
  const rows = Math.max(2, Number(grid?.rows) || 32);
  const heights = grid?.heights || [];
  const waterLevel = water?.enabled !== false ? (Number(water?.level) ?? -0.25) : -999;
  const { pathMask } = applySplineWalkways(grid, splines);

  const weights = new Array(cols * rows);
  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) {
      const idx = r * cols + c;
      const h = Number(heights[idx]) || 0;
      const n = computeVertexNormal(grid, c, r);
      const slope = clamp(1 - n[1], 0, 1);

      // Layer 2: Sand near water shoreline
      const shoreDist = Math.abs(h - waterLevel);
      let wSand = h <= waterLevel + 0.85 ? clamp(1 - shoreDist / 1.1, 0, 1) : 0;
      // Layer 3: Cliff on steep slopes
      let wCliff = clamp((slope - 0.22) / 0.35, 0, 1);
      // Layer 0: Grass on gentle/mid elevations above shoreline
      let wGrass = clamp(1 - Math.max(wSand * 0.9, wCliff * 0.95), 0.05, 1);
      // Layer 1: Stone on splines or high plateaus
      let wStone = h > 3.2 ? clamp((h - 3.2) / 4.0, 0, 0.65) : 0.08;

      const mask = pathMask[idx];
      if (mask && mask.weight > 0.01) {
        const mw = mask.weight;
        const targetLayer = clamp(mask.layerIndex ?? 1, 0, 3);
        const arr = [wGrass * (1 - mw), wStone * (1 - mw), wSand * (1 - mw), wCliff * (1 - mw)];
        arr[targetLayer] += mw * 1.6;
        [wGrass, wStone, wSand, wCliff] = arr;
      }

      const sum = wGrass + wStone + wSand + wCliff || 1;
      weights[idx] = [
        round3(wGrass / sum),
        round3(wStone / sum),
        round3(wSand / sum),
        round3(wCliff / sum),
      ];
    }
  }
  return weights;
}

/**
 * Generates deterministic PCG edge scatter along spline walkways and terrain biomes.
 */
export function generateSplineEdgeScatter(grid, splines = [], water = {}, availableAssets = [], seed = 1337) {
  const waterLevel = water?.enabled !== false ? (Number(water?.level) ?? -0.25) : -999;
  const modelAssets = (availableAssets || []).filter(
    (a) => a && (a.kind === 'model' || a.kind === 'prefab' || String(a.path || '').endsWith('.glb')),
  );
  const archetypes = [
    { kind: 'lantern', label: 'Garden Stone Lantern', scale: [1.0, 1.0, 1.0], light: true },
    { kind: 'foliage_tree', label: 'Canopy Blossom Tree', scale: [1.25, 1.35, 1.25], light: false },
    { kind: 'accent_rock', label: 'Mossy Garden Boulder', scale: [0.9, 0.85, 0.95], light: false },
  ];

  const instances = [];
  let seq = 1;

  for (let sIdx = 0; sIdx < (splines || []).length; sIdx++) {
    const spline = splines[sIdx];
    const pts = Array.isArray(spline?.points) ? spline.points : [];
    if (pts.length < 2) continue;
    const sampled = evaluateCatmullRomSpline(pts, 6);
    const halfWidth = Math.max(0.8, (Number(spline.width) || 3.0) * 0.5);

    for (let i = 1; i < sampled.length - 1; i += 2) {
      const prev = sampled[i - 1];
      const curr = sampled[i];
      const next = sampled[i + 1];
      const tx = next[0] - prev[0];
      const tz = next[2] - prev[2];
      const tLen = Math.hypot(tx, tz) || 1;
      // Perpendicular normal in XZ plane
      const nx = -tz / tLen;
      const nz = tx / tLen;

      for (const side of [-1, 1]) {
        const h1 = deterministicHash(seed + seq * 31 + sIdx * 97);
        const h2 = deterministicHash(seed + seq * 59 + i * 17);
        seq++;
        if (h1 < 0.25) continue;

        const offset = halfWidth + 0.85 + h2 * 1.4;
        const wx = round3(curr[0] + nx * side * offset);
        const wz = round3(curr[2] + nz * side * offset);
        const wy = round3(sampleHeightAtWorld(grid, wx, wz));

        // Skip underwater placements
        if (wy <= waterLevel + 0.08) continue;

        const archIdx = (i + (side > 0 ? 0 : 1)) % archetypes.length;
        const arch = archetypes[archIdx];
        const matchedAsset = modelAssets.length
          ? modelAssets[(seq + archIdx) % modelAssets.length]
          : null;
        const yaw = round3(Math.atan2(-nx * side, -nz * side));

        instances.push({
          id: `scatter_${String(instances.length + 1).padStart(2, '0')}`,
          name: `${arch.label} ${instances.length + 1}`,
          archetype: arch.kind,
          assetId: matchedAsset?.id || null,
          assetPath: matchedAsset?.path || null,
          translation: [wx, wy, wz],
          rotationYaw: yaw,
          scale: [...arch.scale],
          emitsLight: Boolean(arch.light),
        });
      }
    }
  }

  return instances;
}

/**
 * Produces a compact (`<120 tokens`) semantic summary of the `.terrain.json` document for AI agents.
 */
export function summarizeTerrainForAi(doc) {
  const root = doc?.root || doc || {};
  const grid = root.grid || { cols: 32, rows: 32, cellSize: 1, heights: [] };
  const cols = Number(grid.cols) || 32;
  const rows = Number(grid.rows) || 32;
  const cellSize = Number(grid.cellSize) || 1;
  const heights = Array.isArray(grid.heights) ? grid.heights : [];
  const waterLevel = root.water?.enabled !== false ? (Number(root.water?.level) ?? -0.25) : -999;

  let minH = Infinity;
  let maxH = -Infinity;
  let sumH = 0;
  let wetCount = 0;
  for (let i = 0; i < heights.length; i++) {
    const h = Number(heights[i]) || 0;
    if (h < minH) minH = h;
    if (h > maxH) maxH = h;
    sumH += h;
    if (h <= waterLevel) wetCount++;
  }
  const count = Math.max(1, heights.length);
  const meanH = round3(sumH / count);
  minH = Number.isFinite(minH) ? round3(minH) : 0;
  maxH = Number.isFinite(maxH) ? round3(maxH) : 0;
  const waterCoveragePct = Math.round((wetCount / count) * 100);
  const splines = Array.isArray(root.splines) ? root.splines : [];
  const scatter = Array.isArray(root.scatter) ? root.scatter : [];
  const widthM = Math.round((cols - 1) * cellSize);
  const depthM = Math.round((rows - 1) * cellSize);
  const skyPreset = root.sky?.preset || 'dusk';
  const timeHours = Number(root.sky?.timeHours ?? 17.5).toFixed(1);

  const text =
    `Terrain "${root.name || 'Landscape'}" (${cols}x${rows} grid, ${widthM}x${depthM}m, 1u=1m): ` +
    `elev [${minH}m..${maxH}m, avg ${meanH}m], water@${round3(waterLevel)}m (${waterCoveragePct}% wet), ` +
    `sky=${skyPreset}@${timeHours}h, splines=${splines.length}, scatter=${scatter.length} props.`;
  const tokensApprox = Math.ceil(text.split(/\s+/).length * 1.3);

  return {
    text,
    tokensApprox,
    grid: { cols, rows, cellSize, widthM, depthM },
    elevation: { min: minH, max: maxH, mean: meanH },
    waterCoveragePct,
    splines: splines.length,
    scatterCount: scatter.length,
  };
}

/**
 * Exports a `.terrain.json` document to deterministic `.gbs` SceneScript commands.
 */
export function exportTerrainToGbs(doc) {
  const root = doc?.root || doc || {};
  const lines = [
    `# Ganbate SceneScript (.gbs) — Landscape & Scatter Export`,
    `spawn TerrainRoot at 0 0 0 name="${(root.name || 'Landscape').replace(/"/g, '')}"`,
  ];
  if (root.water?.enabled !== false) {
    const wl = round3(root.water?.level ?? -0.25);
    lines.push(`spawn WaterSurface at 0 ${wl} 0 name="Water Pond Surface"`);
  }
  for (const item of root.scatter || []) {
    const [x, y, z] = item.translation || [0, 0, 0];
    const alias = String(item.id || 'Prop').replace(/[^a-zA-Z0-9_]/g, '_');
    const cleanName = String(item.name || alias).replace(/"/g, '');
    lines.push(`spawn ${alias} at ${round3(x)} ${round3(y)} ${round3(z)} name="${cleanName}"`);
  }
  return lines.join('\n');
}

/**
 * Projects all scattered landscape props onto the feasible terrain constraint manifold:
 *   x* = argmin_{x in C} ||x - x_hat||^2
 * Enforces:
 *   1. Exact ground heightfield snap (`y* = sampleHeightAtWorld(grid, x, z)`)
 *   2. Walkway corridor clearance (`dist_XZ >= halfWidth + 0.65m`)
 *   3. Mutual prop non-penetration (`dist_XZ >= 1.35m`)
 *   4. Dry-land elevation above water level
 */
export function projectScatterConstraints(doc) {
  const root = doc?.root || doc || {};
  const grid = root.grid || { cols: 32, rows: 32, cellSize: 1, heights: [] };
  const splines = Array.isArray(root.splines) ? root.splines : [];
  const waterLevel = root.water?.enabled !== false ? (Number(root.water?.level) ?? -0.25) : -999;
  const rawScatter = Array.isArray(root.scatter) ? structuredClone(root.scatter) : [];

  const sampledSplinePts = [];
  for (const sp of splines) {
    const pts = evaluateCatmullRomSpline(sp.points || [], 6);
    const minClear = Math.max(0.8, (Number(sp.width) || 3.0) * 0.5) + 0.65;
    for (const p of pts) {
      sampledSplinePts.push({ x: p[0], z: p[2], minClear, splineId: sp.id || 'walkway_01' });
    }
  }

  let repairedCount = 0;
  let maxDisplacementM = 0;
  const relationalEdges = [];

  for (let i = 0; i < rawScatter.length; i++) {
    const item = rawScatter[i];
    const orig = Array.isArray(item.translation) ? item.translation.slice() : [0, 0, 0];
    let wx = Number(orig[0]) || 0;
    let wz = Number(orig[2]) || 0;

    // 1. Walkway corridor clearance projection
    let nearestSpline = null;
    let nearestDist = Infinity;
    for (const sp of sampledSplinePts) {
      const dx = wx - sp.x;
      const dz = wz - sp.z;
      const d = Math.hypot(dx, dz);
      if (d < nearestDist) {
        nearestDist = d;
        nearestSpline = sp;
      }
      if (d < sp.minClear) {
        const push = sp.minClear - d + 0.05;
        const ux = d > 1e-4 ? dx / d : 1;
        const uz = d > 1e-4 ? dz / d : 0;
        wx += ux * push;
        wz += uz * push;
      }
    }

    // 2. Mutual horizontal non-penetration against earlier items
    for (let j = 0; j < i; j++) {
      const other = rawScatter[j].translation || [0, 0, 0];
      const dx = wx - other[0];
      const dz = wz - other[2];
      const d = Math.hypot(dx, dz);
      const minSep = 1.35;
      if (d < minSep) {
        const push = minSep - d + 0.04;
        const ux = d > 1e-4 ? dx / d : 0;
        const uz = d > 1e-4 ? dz / d : 1;
        wx += ux * push;
        wz += uz * push;
      }
    }

    // 3. Snap to ground heightfield & enforce water clearance
    let wy = sampleHeightAtWorld(grid, wx, wz);
    if (wy <= waterLevel + 0.08) {
      wy = waterLevel + 0.12;
    }

    const nextPos = [round3(wx), round3(wy), round3(wz)];
    const disp = round3(Math.hypot(nextPos[0] - orig[0], nextPos[1] - orig[1], nextPos[2] - orig[2]));
    if (disp > 0.01) {
      repairedCount++;
      if (disp > maxDisplacementM) maxDisplacementM = disp;
    }
    item.translation = nextPos;

    relationalEdges.push({
      from: item.id || `scatter_${i + 1}`,
      to: 'terrain0000000001',
      type: 'snap_to_ground',
      elevationM: nextPos[1],
    });
    if (nearestSpline && nearestDist <= 8.0) {
      relationalEdges.push({
        from: item.id || `scatter_${i + 1}`,
        to: nearestSpline.splineId,
        type: 'along_spline',
        offsetM: round3(nearestDist),
      });
    }
  }

  return {
    repairedCount,
    maxDisplacementM,
    nextScatter: rawScatter,
    relationalEdges,
  };
}

/**
 * Resolves a 2-Agent Landscape Co-Design transaction where:
 *   - `Agent A (Walkway & Grading Architect)` places a garden landmark near the walkway
 *   - `Agent B (Foliage & Lighting Director)` places a stone lantern at an overlapping coordinate
 * Computes Pareto-minimal conflict resolution `(x_A*, x_B*)` and re-projects all scatter constraints.
 */
export function resolveTerrainTwoAgentCoDesign(doc, options = {}) {
  const root = doc?.root || doc || {};
  const grid = root.grid || { cols: 32, rows: 32, cellSize: 1, heights: [] };
  const propA = options.agentA || {
    id: 'co_pavilion_01',
    name: 'Garden Stone Basin',
    archetype: 'accent_rock',
    proposedXZ: [2.4, 1.8],
    weight: 0.55,
  };
  const propB = options.agentB || {
    id: 'co_lantern_01',
    name: 'Walkway Stone Lantern',
    archetype: 'lantern',
    proposedXZ: [2.6, 1.9],
    weight: 0.45,
    emitsLight: true,
  };

  let [ax, az] = propA.proposedXZ;
  let [bx, bz] = propB.proposedXZ;
  const dx = bx - ax;
  const dz = bz - az;
  const dist = Math.hypot(dx, dz);
  const minSep = 1.6;
  let conflictResolved = false;

  if (dist < minSep) {
    conflictResolved = true;
    const overlap = minSep - dist + 0.05;
    const ux = dist > 1e-4 ? dx / dist : 1;
    const uz = dist > 1e-4 ? dz / dist : 0;
    const wA = Number(propA.weight) || 0.5;
    const wB = Number(propB.weight) || 0.5;
    const wSum = wA + wB || 1;
    ax -= ux * overlap * (wB / wSum);
    az -= uz * overlap * (wB / wSum);
    bx += ux * overlap * (wA / wSum);
    bz += uz * overlap * (wA / wSum);
  }

  const ay = sampleHeightAtWorld(grid, ax, az);
  const by = sampleHeightAtWorld(grid, bx, bz);

  const mergedScatter = [
    ...(Array.isArray(root.scatter) ? root.scatter.filter((s) => s.id !== propA.id && s.id !== propB.id) : []),
    {
      id: propA.id,
      name: propA.name,
      archetype: propA.archetype || 'accent_rock',
      translation: [round3(ax), round3(ay), round3(az)],
      rotationYaw: 0,
      scale: [1, 1, 1],
      emitsLight: false,
    },
    {
      id: propB.id,
      name: propB.name,
      archetype: propB.archetype || 'lantern',
      translation: [round3(bx), round3(by), round3(bz)],
      rotationYaw: 0,
      scale: [1, 1, 1],
      emitsLight: Boolean(propB.emitsLight),
    },
  ];

  const projected = projectScatterConstraints({
    ...root,
    scatter: mergedScatter,
  });

  return {
    conflictResolved,
    agentAResolved: projected.nextScatter.find((s) => s.id === propA.id)?.translation || [round3(ax), round3(ay), round3(az)],
    agentBResolved: projected.nextScatter.find((s) => s.id === propB.id)?.translation || [round3(bx), round3(by), round3(bz)],
    repairedCount: projected.repairedCount,
    nextScatter: projected.nextScatter,
    relationalEdges: projected.relationalEdges,
  };
}

/**
 * Builds scene entity definitions from a `.terrain.json` document so syncing a landscape
 * immediately populates the open `.scene.json` with a terrain heightfield entity, water plane,
 * spline walkways, and scattered props.
 */
export function buildTerrainSceneEntities(doc) {
  const root = doc?.root || doc || {};
  const grid = root.grid || { cols: 32, rows: 32, cellSize: 1, heights: [] };
  const cols = Number(grid.cols) || 32;
  const rows = Number(grid.rows) || 32;
  const cellSize = Number(grid.cellSize) || 1;
  const worldWidth = Math.max(8, (cols - 1) * cellSize);
  const worldDepth = Math.max(8, (rows - 1) * cellSize);
  const out = {};

  out['terrain0000000001'] = {
    name: root.name || 'Landscape Terrain',
    order: 'a0',
    components: {
      transform: { translation: [0, 0, 0], rotation: [0, 0, 0, 1], scale: [1, 1, 1] },
      terrain: {
        cols,
        rows,
        resolution: cols,
        cellSize,
        worldWidth,
        worldDepth,
        minHeight: grid.minHeight,
        maxHeight: grid.maxHeight,
        heights: Array.isArray(grid.heights) ? grid.heights.slice() : [],
        splatLayers: Array.isArray(root.layers) ? structuredClone(root.layers) : [],
        waterLevel: root.water?.level ?? -0.25,
        splines: structuredClone(root.splines || []),
      },
    },
  };

  if (root.water?.enabled !== false) {
    const wl = round3(root.water?.level ?? -0.25);
    out['terrain0000000002'] = {
      name: 'Landscape Water Plane',
      parent: 'terrain0000000001',
      order: 'a1',
      components: {
        transform: { translation: [0, wl, 0], rotation: [0, 0, 0, 1], scale: [1, 1, 1] },
        water: {
          level: wl,
          worldWidth,
          worldDepth,
          shallowColor: root.water?.shallowColor || [0.16, 0.44, 0.56, 0.72],
          deepColor: root.water?.deepColor || [0.06, 0.18, 0.30, 0.88],
        },
      },
    };
  }

  const scatterList = Array.isArray(root.scatter) ? root.scatter.slice(0, 24) : [];
  for (let i = 0; i < scatterList.length; i++) {
    const item = scatterList[i];
    const eid = `terrainsc${String(i + 1).padStart(8, '0')}`;
    const yaw = Number(item.rotationYaw) || 0;
    const half = yaw * 0.5;
    const isLantern = item.archetype === 'lantern' || Boolean(item.emitsLight);
    const isTree = item.archetype === 'foliage_tree';
    const components = {
      transform: {
        translation: (item.translation || [0, 0, 0]).map((v) => round3(v)),
        rotation: [0, round3(Math.sin(half)), 0, round3(Math.cos(half))],
        scale: (item.scale || [1, 1, 1]).map((v) => round3(v)),
      },
      mesh: {
        primitive: isLantern ? 'lantern' : isTree ? 'tree' : 'box',
        archetype: item.archetype || 'foliage_tree',
        color: isLantern ? [0.95, 0.72, 0.38] : isTree ? [0.92, 0.58, 0.72] : [0.52, 0.54, 0.56],
        ...(item.assetId ? { model: item.assetId } : {}),
      },
      relations: [
        { type: 'snap_to_ground', to: 'terrain0000000001' },
        { type: 'along_spline', to: 'walkway_01' },
      ],
    };
    if (item.emitsLight) {
      components.light = { color: [1.0, 0.86, 0.62], intensity: 3.6, range: 9.0 };
    }
    out[eid] = {
      name: item.name || `Scattered Prop ${i + 1}`,
      parent: 'terrain0000000001',
      order: `b${String(i).padStart(2, '0')}`,
      components,
    };
  }

  return out;
}

/**
 * Mounts the interactive Landscape & Terrain Studio workbench inside `container`.
 */
export function mountTerrainStudio(container, studioApi = {}) {
  if (!container) throw new Error('mountTerrainStudio requires a container element');
  const docOwner = container.ownerDocument || globalThis.document;
  const el = (tag, attrs = {}, ...children) => {
    const node = docOwner.createElement(tag);
    for (const [k, v] of Object.entries(attrs || {})) {
      if (k.startsWith('on') && typeof v === 'function') {
        node.addEventListener(k.slice(2).toLowerCase(), v);
      } else if (k === 'class') {
        node.className = v;
      } else if (k === 'style' && typeof v === 'object') {
        Object.assign(node.style, v);
      } else if (v !== undefined && v !== null && v !== false) {
        node.setAttribute(k, v === true ? '' : String(v));
      }
    }
    for (const child of children.flat()) {
      if (child === null || child === undefined || child === false) continue;
      node.append(typeof child === 'string' || typeof child === 'number'
        ? docOwner.createTextNode(String(child))
        : child);
    }
    return node;
  };

  let activeDocId = null;
  let terrainDoc = createDefaultTerrainDoc('Sanctuary Landscape', 'sanctuary_garden', {
    assets: studioApi.listAssets?.() || [],
  });
  let splatWeights = computeSplatmapWeights(
    terrainDoc.grid,
    terrainDoc.splatLayers,
    terrainDoc.water,
    terrainDoc.splines,
  );

  const uiState = {
    brushMode: 'raise',
    brushRadius: 5.5,
    brushStrength: 0.55,
    brushFalloff: 'smooth',
    flattenHeight: 0.0,
    terraceStep: 1.0,
    activeSplineIdx: 0,
    viewMode: '3d', // '3d' | 'top'
    camera: {
      yaw: 0.62,
      pitch: 0.52,
      distance: 44,
      target: [0, 0.5, 0],
    },
    hoverWorld: null, // [wx, wy, wz]
    sculpting: false,
    orbiting: null,
    strokeSeq: 0,
  };

  // Project a 3D world point `[x, y, z]` onto 2D canvas pixels `[px, py, depth]`.
  function projectPoint(pt, width, height) {
    const [wx, wy, wz] = pt;
    if (uiState.viewMode === 'top') {
      const cols = terrainDoc.grid.cols;
      const rows = terrainDoc.grid.rows;
      const worldW = (cols - 1) * terrainDoc.grid.cellSize;
      const worldD = (rows - 1) * terrainDoc.grid.cellSize;
      const scale = Math.min(width, height) * 0.82 / Math.max(worldW, worldD, 1);
      return {
        x: width * 0.5 + wx * scale,
        y: height * 0.5 + wz * scale,
        depth: 10 - wy * 0.1,
      };
    }
    const cam = uiState.camera;
    const cy = Math.cos(cam.yaw);
    const sy = Math.sin(cam.yaw);
    const cp = Math.cos(cam.pitch);
    const sp = Math.sin(cam.pitch);
    const forward = [-sy * cp, -sp, -cy * cp];
    const right = [cy, 0, -sy];
    const up = [-sy * sp, cp, -cy * sp];
    const eye = [
      cam.target[0] - forward[0] * cam.distance,
      cam.target[1] - forward[1] * cam.distance,
      cam.target[2] - forward[2] * cam.distance,
    ];
    const dx = wx - eye[0];
    const dy = wy - eye[1];
    const dz = wz - eye[2];
    const zCam = dx * forward[0] + dy * forward[1] + dz * forward[2];
    if (zCam <= 0.2) return null;
    const xCam = dx * right[0] + dy * right[1] + dz * right[2];
    const yCam = dx * up[0] + dy * up[1] + dz * up[2];
    const fovScale = (height * 0.5) / Math.tan((50 * Math.PI) / 360);
    return {
      x: width * 0.5 + (xCam / zCam) * fovScale,
      y: height * 0.5 - (yCam / zCam) * fovScale,
      depth: zCam,
    };
  }

  // Unproject a 2D canvas click `(px, py)` onto the terrain surface (or `y = 0` plane).
  function screenToTerrainWorld(px, py, width, height) {
    const cols = terrainDoc.grid.cols;
    const rows = terrainDoc.grid.rows;
    const cellSize = terrainDoc.grid.cellSize;
    const halfW = ((cols - 1) * cellSize) * 0.5;
    const halfD = ((rows - 1) * cellSize) * 0.5;

    if (uiState.viewMode === 'top') {
      const worldW = (cols - 1) * cellSize;
      const worldD = (rows - 1) * cellSize;
      const scale = Math.min(width, height) * 0.82 / Math.max(worldW, worldD, 1);
      const wx = clamp((px - width * 0.5) / scale, -halfW, halfW);
      const wz = clamp((py - height * 0.5) / scale, -halfD, halfD);
      const wy = sampleHeightAtWorld(terrainDoc.grid, wx, wz);
      return [round3(wx), round3(wy), round3(wz)];
    }

    // Find closest projected grid vertex or ray-plane intersection
    let bestPt = null;
    let bestDistSq = Infinity;
    for (let r = 0; r < rows; r++) {
      for (let c = 0; c < cols; c++) {
        const wpt = gridCellToWorld(terrainDoc.grid, c, r);
        const proj = projectPoint(wpt, width, height);
        if (!proj) continue;
        const dSq = (proj.x - px) * (proj.x - px) + (proj.y - py) * (proj.y - py);
        if (dSq < bestDistSq) {
          bestDistSq = dSq;
          bestPt = wpt;
        }
      }
    }
    return bestPt ? bestPt.map((v) => round3(v)) : [0, 0, 0];
  }

  async function commitTerrainMutation(label, mergeKey, mutator) {
    mutator(terrainDoc);
    splatWeights = computeSplatmapWeights(
      terrainDoc.grid,
      terrainDoc.splatLayers,
      terrainDoc.water,
      terrainDoc.splines,
    );
    drawStage();
    updateInspectorWidgets();

    if (activeDocId && typeof studioApi.edit === 'function') {
      await studioApi.edit(
        activeDocId,
        [
          { set: ['grid'], value: structuredClone(terrainDoc.grid) },
          { set: ['water'], value: structuredClone(terrainDoc.water) },
          { set: ['sky'], value: structuredClone(terrainDoc.sky) },
          { set: ['splines'], value: structuredClone(terrainDoc.splines) },
          { set: ['scatter'], value: structuredClone(terrainDoc.scatter) },
        ],
        label,
        mergeKey,
      );
    }
  }

  async function applyBrushStroke(stroke = {}) {
    const mode = stroke.mode || uiState.brushMode;
    const x = Number.isFinite(stroke.x) ? Number(stroke.x) : 0;
    const z = Number.isFinite(stroke.z) ? Number(stroke.z) : 0;
    const radius = Number.isFinite(stroke.radius) ? Number(stroke.radius) : uiState.brushRadius;
    const strength = Number.isFinite(stroke.strength) ? Number(stroke.strength) : uiState.brushStrength;
    const falloff = stroke.falloff || uiState.brushFalloff;

    if (mode === 'spline') {
      return addSplinePoint(uiState.activeSplineIdx, [x, sampleHeightAtWorld(terrainDoc.grid, x, z), z]);
    }

    const res = applyTerrainBrush(terrainDoc.grid, {
      mode,
      x,
      z,
      radius,
      strength,
      falloff,
      targetHeight: stroke.targetHeight ?? uiState.flattenHeight,
      terraceStep: stroke.terraceStep ?? uiState.terraceStep,
    });
    if (!res.changed) return res;

    await commitTerrainMutation(
      `Terrain ${mode} brush`,
      stroke.mergeKey || `terrain-brush-${uiState.strokeSeq}`,
      (doc) => {
        doc.grid.heights = res.nextHeights;
        doc.grid.minHeight = res.minHeight;
        doc.grid.maxHeight = res.maxHeight;
      },
    );
    return res;
  }

  async function addSplinePoint(splineIndex = 0, point = [0, 0, 0]) {
    const pt = [round3(point[0]), round3(point[1]), round3(point[2])];
    await commitTerrainMutation('Add walkway spline point', undefined, (doc) => {
      if (!Array.isArray(doc.splines) || !doc.splines.length) {
        doc.splines = [{
          id: 'walkway-1',
          name: 'Stone Walkway 1',
          width: 3.0,
          falloff: 1.8,
          flattenStrength: 0.85,
          layerIndex: 1,
          points: [],
        }];
      }
      const target = doc.splines[clamp(splineIndex, 0, doc.splines.length - 1)];
      target.points.push(pt);
      const graded = applySplineWalkways(doc.grid, doc.splines);
      doc.grid.heights = graded.nextHeights;
      doc.grid.minHeight = graded.minHeight;
      doc.grid.maxHeight = graded.maxHeight;
      doc.scatter = generateSplineEdgeScatter(
        doc.grid,
        doc.splines,
        doc.water,
        studioApi.listAssets?.() || [],
        1337,
      );
    });
    return terrainDoc.splines[clamp(splineIndex, 0, terrainDoc.splines.length - 1)];
  }

  async function applySkyPreset(presetId) {
    const sky = TERRAIN_SKY_PRESETS[presetId];
    if (!sky) return;
    await commitTerrainMutation(`Sky preset: ${sky.label}`, undefined, (doc) => {
      doc.sky = {
        preset: sky.id,
        timeHours: sky.timeHours,
        weather: sky.weather,
      };
    });
    const vp = studioApi.getViewport?.();
    vp?.setTime?.(sky.timeHours);
    vp?.setWeather?.(sky.weather);
  }

  async function syncToScene() {
    const activeDoc = studioApi.getActiveDocument?.();
    const sceneEntities = buildTerrainSceneEntities(terrainDoc);
    const ops = Object.entries(sceneEntities).map(([eid, ent]) => ({
      set: ['entities', eid],
      value: ent,
    }));
    const targetDocId = activeDoc?.kind === 'scene' ? activeDoc.id : null;
    if (typeof studioApi.edit === 'function') {
      await studioApi.edit(targetDocId, ops, `Sync Landscape (${terrainDoc.name})`);
    }
    studioApi.notify?.('info', `Synced "${terrainDoc.name}" (${Object.keys(sceneEntities).length} landscape & prop entities) to 3D Scene.`);
    return sceneEntities;
  }

  // ---- Build DOM -------------------------------------------------------------------------------
  const canvas = el('canvas', {
    class: 'terrain-stage-canvas',
    width: 920,
    height: 580,
    'data-role': 'terrain-canvas',
  });
  const hudCoordsEl = el('div', { class: 'terrain-stage-badge', 'data-role': 'terrain-cursor-readout' },
    'Cursor: 0.0m, 0.0m · Elev: 0.00m',
  );
  const aiDigestEl = el('pre', { class: 'terrain-ai-digest', 'data-role': 'terrain-ai-digest' }, '');
  const constraintReadoutEl = el('pre', { class: 'terrain-ai-digest', 'data-role': 'terrain-constraint-readout' }, '');
  const splineListEl = el('div', { class: 'terrain-spline-list', 'data-role': 'terrain-spline-list' });
  const radiusValEl = el('span', { class: 'val' }, `${uiState.brushRadius.toFixed(1)}m`);
  const strengthValEl = el('span', { class: 'val' }, `${Math.round(uiState.brushStrength * 100)}%`);
  const waterValEl = el('span', { class: 'val' }, `${terrainDoc.water.level.toFixed(2)}m`);

  const brushButtons = new Map();
  const skyButtons = new Map();

  async function repairScatterConstraints() {
    const res = projectScatterConstraints(terrainDoc);
    await commitTerrainMutation(
      `Project scatter constraints (${res.repairedCount} repaired)`,
      undefined,
      (doc) => {
        doc.scatter = res.nextScatter;
      },
    );
    constraintReadoutEl.textContent =
      `x* Projection: ${res.repairedCount} props adjusted (max Δ=${res.maxDisplacementM.toFixed(2)}m)\n` +
      `Relational edges: ${res.relationalEdges.length} (snap_to_ground + along_spline)`;
    return res;
  }

  async function runTwoAgentCoDesign(options = {}) {
    const res = resolveTerrainTwoAgentCoDesign(terrainDoc, options);
    await commitTerrainMutation(
      'Resolve 2-Agent Landscape Co-Design',
      undefined,
      (doc) => {
        doc.scatter = res.nextScatter;
      },
    );
    constraintReadoutEl.textContent =
      `2-Agent Co-Design: conflict=${res.conflictResolved ? 'RESOLVED' : 'NONE'}\n` +
      `Agent A -> [${res.agentAResolved.join(', ')}] · Agent B -> [${res.agentBResolved.join(', ')}]\n` +
      `Edges: ${res.relationalEdges.length}`;
    return res;
  }

  function updateInspectorWidgets() {
    const digest = summarizeTerrainForAi(terrainDoc);
    aiDigestEl.textContent = `${digest.text}\n(~${digest.tokensApprox} tokens · budget <120)`;
    if (!constraintReadoutEl.textContent) {
      const snap = projectScatterConstraints(terrainDoc);
      constraintReadoutEl.textContent =
        `Relational edges: ${snap.relationalEdges.length} (snap_to_ground + along_spline)\n` +
        `Pending constraint drift: ${snap.repairedCount} props (max Δ=${snap.maxDisplacementM.toFixed(2)}m)`;
    }
    waterValEl.textContent = `${Number(terrainDoc.water?.level ?? -0.25).toFixed(2)}m`;
    for (const [id, btn] of brushButtons) {
      const on = uiState.brushMode === id;
      btn.classList.toggle('on', on);
      btn.setAttribute('aria-pressed', String(on));
    }
    for (const [id, btn] of skyButtons) {
      const on = (terrainDoc.sky?.preset || 'dusk') === id;
      btn.classList.toggle('on', on);
      btn.setAttribute('aria-pressed', String(on));
    }
    splineListEl.replaceChildren(
      ...(terrainDoc.splines || []).map((sp, idx) =>
        el('div', {
          class: `terrain-spline-item${idx === uiState.activeSplineIdx ? ' active' : ''}`,
          onclick: () => {
            uiState.activeSplineIdx = idx;
            updateInspectorWidgets();
            drawStage();
          },
        },
          el('span', {}, `${sp.name || sp.id} (${(sp.points || []).length} pts · ${sp.width}m)`),
          el('button', {
            type: 'button',
            class: 'terrain-studio-btn',
            onclick: (ev) => {
              ev.stopPropagation();
              void commitTerrainMutation('Clear spline points', undefined, (doc) => {
                if (doc.splines?.[idx]) doc.splines[idx].points = [];
              });
            },
          }, 'Clear'),
        ),
      ),
    );
  }

  function drawStage() {
    const ctx = typeof canvas.getContext === 'function' ? canvas.getContext('2d') : null;
    if (!ctx) return;
    const width = canvas.width || 920;
    const height = canvas.height || 580;

    const sky = TERRAIN_SKY_PRESETS[terrainDoc.sky?.preset] || TERRAIN_SKY_PRESETS.dusk;
    const grad = ctx.createLinearGradient(0, 0, 0, height);
    grad.addColorStop(0, sky.skyTop);
    grad.addColorStop(0.55, sky.skyHorizon);
    grad.addColorStop(1, '#10141d');
    ctx.fillStyle = grad;
    ctx.fillRect(0, 0, width, height);

    const grid = terrainDoc.grid;
    const cols = grid.cols;
    const rows = grid.rows;
    const waterEnabled = terrainDoc.water?.enabled !== false;
    const waterLevel = Number(terrainDoc.water?.level ?? -0.25);
    const layers = terrainDoc.splatLayers || DEFAULT_SPLAT_LAYERS;
    const [sx, sy, sz] = sky.sunDir;
    const sLen = Math.hypot(sx, sy, sz) || 1;
    const sun = [sx / sLen, sy / sLen, sz / sLen];

    // Project all vertices once
    const projected = new Array(cols * rows);
    for (let r = 0; r < rows; r++) {
      for (let c = 0; c < cols; c++) {
        const wpt = gridCellToWorld(grid, c, r);
        projected[r * cols + c] = {
          world: wpt,
          screen: projectPoint(wpt, width, height),
        };
      }
    }

    // Painter's sort quads back-to-front
    const quads = [];
    for (let r = 0; r < rows - 1; r++) {
      for (let c = 0; c < cols - 1; c++) {
        const i00 = r * cols + c;
        const i10 = r * cols + (c + 1);
        const i01 = (r + 1) * cols + c;
        const i11 = (r + 1) * cols + (c + 1);
        const p00 = projected[i00];
        const p10 = projected[i10];
        const p01 = projected[i01];
        const p11 = projected[i11];
        if (!p00.screen || !p10.screen || !p01.screen || !p11.screen) continue;
        const depth = (p00.screen.depth + p10.screen.depth + p01.screen.depth + p11.screen.depth) * 0.25;
        quads.push({ c, r, i00, i10, i01, i11, p00, p10, p01, p11, depth });
      }
    }
    quads.sort((a, b) => b.depth - a.depth);

    for (const q of quads) {
      const n = computeVertexNormal(grid, q.c, q.r);
      const ndotl = clamp(n[0] * sun[0] + n[1] * sun[1] + n[2] * sun[2], 0.18, 1.0);
      const w = splatWeights[q.i00] || [0.7, 0.1, 0.1, 0.1];
      let cr = 0;
      let cg = 0;
      let cb = 0;
      for (let l = 0; l < 4; l++) {
        const col = layers[l]?.color || DEFAULT_SPLAT_LAYERS[l].color;
        cr += col[0] * w[l];
        cg += col[1] * w[l];
        cb += col[2] * w[l];
      }
      const avgY = (q.p00.world[1] + q.p10.world[1] + q.p01.world[1] + q.p11.world[1]) * 0.25;
      let r8 = Math.round(clamp(cr * (0.45 + 0.65 * ndotl) * sky.sunColor[0], 0, 1) * 255);
      let g8 = Math.round(clamp(cg * (0.45 + 0.65 * ndotl) * sky.sunColor[1], 0, 1) * 255);
      let b8 = Math.round(clamp(cb * (0.45 + 0.65 * ndotl) * sky.sunColor[2], 0, 1) * 255);

      if (waterEnabled && avgY <= waterLevel) {
        const depthBelow = clamp((waterLevel - avgY) / 2.2, 0.15, 0.88);
        r8 = Math.round(r8 * (1 - depthBelow) + 28 * depthBelow);
        g8 = Math.round(g8 * (1 - depthBelow) + 96 * depthBelow);
        b8 = Math.round(b8 * (1 - depthBelow) + 138 * depthBelow);
      }

      ctx.beginPath();
      ctx.moveTo(q.p00.screen.x, q.p00.screen.y);
      ctx.lineTo(q.p10.screen.x, q.p10.screen.y);
      ctx.lineTo(q.p11.screen.x, q.p11.screen.y);
      ctx.lineTo(q.p01.screen.x, q.p01.screen.y);
      ctx.closePath();
      ctx.fillStyle = `rgb(${r8}, ${g8}, ${b8})`;
      ctx.fill();
      ctx.strokeStyle = waterEnabled && Math.abs(avgY - waterLevel) < 0.16
        ? 'rgba(185, 235, 255, 0.55)'
        : 'rgba(255, 255, 255, 0.06)';
      ctx.lineWidth = 0.7;
      ctx.stroke();
    }

    // Draw spline walkways & control handles
    for (let sIdx = 0; sIdx < (terrainDoc.splines || []).length; sIdx++) {
      const sp = terrainDoc.splines[sIdx];
      const sampled = evaluateCatmullRomSpline(sp.points || [], 10);
      if (sampled.length >= 2) {
        ctx.beginPath();
        let started = false;
        for (const pt of sampled) {
          const wy = sampleHeightAtWorld(grid, pt[0], pt[2]) + 0.12;
          const scr = projectPoint([pt[0], wy, pt[2]], width, height);
          if (!scr) continue;
          if (!started) {
            ctx.moveTo(scr.x, scr.y);
            started = true;
          } else {
            ctx.lineTo(scr.x, scr.y);
          }
        }
        ctx.strokeStyle = sIdx === uiState.activeSplineIdx ? '#ffd97d' : '#94a3b8';
        ctx.lineWidth = 3;
        ctx.stroke();
      }
      for (const cp of sp.points || []) {
        const scr = projectPoint([cp[0], cp[1] + 0.18, cp[2]], width, height);
        if (!scr) continue;
        ctx.beginPath();
        ctx.arc(scr.x, scr.y, 4.5, 0, Math.PI * 2);
        ctx.fillStyle = '#f4c96b';
        ctx.fill();
        ctx.strokeStyle = '#14161c';
        ctx.lineWidth = 1.5;
        ctx.stroke();
      }
    }

    // Draw PCG scattered props
    for (const item of terrainDoc.scatter || []) {
      const [wx, wy, wz] = item.translation || [0, 0, 0];
      const scr = projectPoint([wx, wy + 0.35, wz], width, height);
      if (!scr) continue;
      ctx.beginPath();
      if (item.archetype === 'lantern') {
        ctx.arc(scr.x, scr.y, 4.2, 0, Math.PI * 2);
        ctx.fillStyle = '#ffb84d';
      } else if (item.archetype === 'foliage_tree') {
        ctx.arc(scr.x, scr.y, 5.5, 0, Math.PI * 2);
        ctx.fillStyle = '#f49ac2';
      } else {
        ctx.arc(scr.x, scr.y, 3.5, 0, Math.PI * 2);
        ctx.fillStyle = '#9aa5b8';
      }
      ctx.fill();
    }

    // Draw 3D Brush Ring Cursor
    if (uiState.hoverWorld) {
      const [hx, hy, hz] = uiState.hoverWorld;
      ctx.beginPath();
      const segs = 28;
      let first = true;
      for (let i = 0; i <= segs; i++) {
        const ang = (i / segs) * Math.PI * 2;
        const rx = hx + Math.cos(ang) * uiState.brushRadius;
        const rz = hz + Math.sin(ang) * uiState.brushRadius;
        const ry = sampleHeightAtWorld(grid, rx, rz) + 0.12;
        const scr = projectPoint([rx, ry, rz], width, height);
        if (!scr) continue;
        if (first) {
          ctx.moveTo(scr.x, scr.y);
          first = false;
        } else {
          ctx.lineTo(scr.x, scr.y);
        }
      }
      ctx.strokeStyle = '#ffd97d';
      ctx.lineWidth = 2;
      ctx.stroke();
    }
  }

  // ---- Canvas Pointer Events -------------------------------------------------------------------
  canvas.addEventListener('contextmenu', (ev) => ev.preventDefault());
  canvas.addEventListener('pointerdown', (ev) => {
    const rect = canvas.getBoundingClientRect();
    const sx = ((ev.clientX - rect.left) / Math.max(1, rect.width)) * canvas.width;
    const sy = ((ev.clientY - rect.top) / Math.max(1, rect.height)) * canvas.height;
    canvas.setPointerCapture?.(ev.pointerId);

    if (ev.button === 0 && !ev.altKey) {
      const hit = screenToTerrainWorld(sx, sy, canvas.width, canvas.height);
      uiState.hoverWorld = hit;
      uiState.sculpting = true;
      uiState.strokeSeq++;
      void applyBrushStroke({
        x: hit[0],
        z: hit[2],
        mergeKey: `terrain-stroke-${uiState.strokeSeq}`,
      });
    } else {
      uiState.orbiting = {
        x: ev.clientX,
        y: ev.clientY,
        pan: ev.button === 1 || ev.shiftKey,
      };
    }
  });

  canvas.addEventListener('pointermove', (ev) => {
    const rect = canvas.getBoundingClientRect();
    const sx = ((ev.clientX - rect.left) / Math.max(1, rect.width)) * canvas.width;
    const sy = ((ev.clientY - rect.top) / Math.max(1, rect.height)) * canvas.height;

    if (uiState.orbiting) {
      const dx = ev.clientX - uiState.orbiting.x;
      const dy = ev.clientY - uiState.orbiting.y;
      uiState.orbiting.x = ev.clientX;
      uiState.orbiting.y = ev.clientY;
      if (uiState.orbiting.pan) {
        uiState.camera.target[0] -= dx * 0.04;
        uiState.camera.target[2] -= dy * 0.04;
      } else {
        uiState.camera.yaw -= dx * 0.006;
        uiState.camera.pitch = clamp(uiState.camera.pitch + dy * 0.006, 0.12, 1.45);
      }
      drawStage();
      return;
    }

    const hit = screenToTerrainWorld(sx, sy, canvas.width, canvas.height);
    uiState.hoverWorld = hit;
    hudCoordsEl.textContent = `Cursor: ${hit[0].toFixed(1)}m, ${hit[2].toFixed(1)}m · Elev: ${hit[1].toFixed(2)}m · Brush: ${uiState.brushMode.toUpperCase()} (${uiState.brushRadius.toFixed(1)}m)`;
    if (uiState.sculpting && uiState.brushMode !== 'spline') {
      void applyBrushStroke({
        x: hit[0],
        z: hit[2],
        mergeKey: `terrain-stroke-${uiState.strokeSeq}`,
      });
    } else {
      drawStage();
    }
  });

  const stopPointer = () => {
    uiState.sculpting = false;
    uiState.orbiting = null;
  };
  canvas.addEventListener('pointerup', stopPointer);
  canvas.addEventListener('pointercancel', stopPointer);
  canvas.addEventListener('wheel', (ev) => {
    ev.preventDefault();
    uiState.camera.distance = clamp(uiState.camera.distance * Math.exp(ev.deltaY * 0.001), 10, 140);
    drawStage();
  }, { passive: false });

  // ---- Header & Sidebars -----------------------------------------------------------------------
  const presetSelect = el('select', {
    class: 'terrain-studio-btn',
    'aria-label': 'Landscape Preset',
    onchange: (ev) => {
      const nextId = ev.target.value;
      terrainDoc = createDefaultTerrainDoc(
        TERRAIN_LANDSCAPE_PRESETS[nextId]?.label || 'Landscape',
        nextId,
        { assets: studioApi.listAssets?.() || [] },
      );
      splatWeights = computeSplatmapWeights(
        terrainDoc.grid,
        terrainDoc.splatLayers,
        terrainDoc.water,
        terrainDoc.splines,
      );
      updateInspectorWidgets();
      drawStage();
    },
  },
    ...Object.values(TERRAIN_LANDSCAPE_PRESETS).map((p) =>
      el('option', { value: p.id }, p.label),
    ),
  );

  const headerEl = el('header', { class: 'terrain-studio-header' },
    el('span', { class: 'terrain-studio-title' }, '⛰️ Landscape & Terrain Studio'),
    presetSelect,
    el('div', { class: 'terrain-studio-pill-group', role: 'group', 'aria-label': 'Camera View Mode' },
      el('button', {
        type: 'button',
        class: 'terrain-studio-btn on',
        'data-view-mode': '3d',
        onclick: (ev) => {
          uiState.viewMode = '3d';
          ev.currentTarget.classList.add('on');
          ev.currentTarget.nextElementSibling?.classList.remove('on');
          drawStage();
        },
      }, '3D Perspective'),
      el('button', {
        type: 'button',
        class: 'terrain-studio-btn',
        'data-view-mode': 'top',
        onclick: (ev) => {
          uiState.viewMode = 'top';
          ev.currentTarget.classList.add('on');
          ev.currentTarget.previousElementSibling?.classList.remove('on');
          drawStage();
        },
      }, 'Top-Down Map'),
    ),
    el('div', { class: 'terrain-studio-pill-group', role: 'group', 'aria-label': 'Sky & Time of Day' },
      ...Object.values(TERRAIN_SKY_PRESETS).map((sky) => {
        const btn = el('button', {
          type: 'button',
          class: `terrain-studio-btn${sky.id === terrainDoc.sky?.preset ? ' on' : ''}`,
          'data-sky-preset': sky.id,
          onclick: () => { void applySkyPreset(sky.id); },
        }, `${sky.label} (${sky.timeHours}h)`);
        skyButtons.set(sky.id, btn);
        return btn;
      }),
    ),
    el('button', {
      type: 'button',
      class: 'terrain-studio-btn primary',
      'data-action': 'sync-to-scene',
      onclick: () => { void syncToScene(); },
    }, '✨ Sync Landscape to 3D Scene'),
  );

  const leftSidebar = el('aside', { class: 'terrain-studio-sidebar' },
    el('section', { class: 'terrain-studio-card' },
      el('h3', {}, '1. Sculpting Brushes'),
      el('div', { class: 'terrain-brush-grid' },
        ...TERRAIN_BRUSH_MODES.map((b) => {
          const btn = el('button', {
            type: 'button',
            class: `terrain-studio-btn${b.id === uiState.brushMode ? ' on' : ''}`,
            'data-brush-mode': b.id,
            onclick: () => {
              uiState.brushMode = b.id;
              updateInspectorWidgets();
            },
          }, `${b.icon} ${b.label}`);
          brushButtons.set(b.id, btn);
          return btn;
        }),
      ),
      el('label', { class: 'terrain-field' },
        el('span', {}, 'Brush Radius'),
        el('input', {
          type: 'range',
          min: 1,
          max: 24,
          step: 0.5,
          value: uiState.brushRadius,
          oninput: (ev) => {
            uiState.brushRadius = Number(ev.target.value) || 5.5;
            radiusValEl.textContent = `${uiState.brushRadius.toFixed(1)}m`;
            drawStage();
          },
        }),
        radiusValEl,
      ),
      el('label', { class: 'terrain-field' },
        el('span', {}, 'Strength'),
        el('input', {
          type: 'range',
          min: 0.05,
          max: 1.0,
          step: 0.05,
          value: uiState.brushStrength,
          oninput: (ev) => {
            uiState.brushStrength = Number(ev.target.value) || 0.5;
            strengthValEl.textContent = `${Math.round(uiState.brushStrength * 100)}%`;
          },
        }),
        strengthValEl,
      ),
    ),
    el('section', { class: 'terrain-studio-card' },
      el('h3', {}, '2. 4-Layer Splatmap Surface'),
      el('div', { class: 'terrain-layer-list' },
        ...(terrainDoc.splatLayers || DEFAULT_SPLAT_LAYERS).map((layer, idx) =>
          el('div', { class: 'terrain-layer-row', 'data-layer-id': layer.id },
            el('span', { class: 'terrain-layer-swatch', style: { background: layer.hex } }),
            el('span', {}, `${idx + 1}. ${layer.name}`),
            el('span', { class: 'val' }, `R:${layer.roughness}`),
          ),
        ),
      ),
    ),
    el('section', { class: 'terrain-studio-card' },
      el('h3', {}, '3. Water Plane & Pond'),
      el('label', { class: 'terrain-field' },
        el('span', {}, 'Water Level'),
        el('input', {
          type: 'range',
          min: -4.0,
          max: 3.0,
          step: 0.05,
          value: terrainDoc.water.level,
          oninput: (ev) => {
            const lvl = Number(ev.target.value);
            void commitTerrainMutation('Adjust water level', 'water-level', (doc) => {
              doc.water.level = lvl;
            });
          },
        }),
        waterValEl,
      ),
    ),
  );

  const rightSidebar = el('aside', { class: 'terrain-studio-sidebar right' },
    el('section', { class: 'terrain-studio-card' },
      el('h3', {},
        el('span', {}, '4. Spline Walkways & PCG Scatter'),
        el('button', {
          type: 'button',
          class: 'terrain-studio-btn',
          'data-action': 'resed-scatter',
          onclick: () => {
            void commitTerrainMutation('Regenerate PCG Edge Scatter', undefined, (doc) => {
              doc.scatter = generateSplineEdgeScatter(
                doc.grid,
                doc.splines,
                doc.water,
                studioApi.listAssets?.() || [],
                Math.floor(Math.random() * 99999) + 1,
              );
            });
          },
        }, '🎲 Scatter'),
      ),
      splineListEl,
    ),
    el('section', { class: 'terrain-studio-card' },
      el('h3', {}, '5. AI Semantic Digest (<120 tok)'),
      aiDigestEl,
    ),
    el('section', { class: 'terrain-studio-card' },
      el('h3', {}, '6. Relational Graph & Solver (x̂ → x*)'),
      el('div', { class: 'terrain-studio-pill-group' },
        el('button', {
          type: 'button',
          class: 'terrain-studio-btn',
          'data-action': 'repair-scatter-constraints',
          onclick: () => { void repairScatterConstraints(); },
        }, '📐 Project (x̂ → x*)'),
        el('button', {
          type: 'button',
          class: 'terrain-studio-btn',
          'data-action': 'run-two-agent-codesign',
          onclick: () => { void runTwoAgentCoDesign(); },
        }, '🤝 2-Agent Co-Design'),
      ),
      constraintReadoutEl,
    ),
  );

  const stageWrap = el('div', { class: 'terrain-stage-wrap' },
    canvas,
    el('div', { class: 'terrain-stage-hud' },
      hudCoordsEl,
      el('div', { class: 'terrain-stage-badge' },
        'Left-Drag: Sculpt / Place Walkway Pt · Right-Drag: Orbit · Shift+Right: Pan · Wheel: Zoom',
      ),
    ),
  );

  const rootEl = el('div', { class: 'terrain-studio-root' },
    headerEl,
    leftSidebar,
    stageWrap,
    rightSidebar,
  );

  container.replaceChildren(rootEl);
  updateInspectorWidgets();
  drawStage();

  return {
    destroy() {
      container.replaceChildren();
    },
    refresh() {
      drawStage();
      updateInspectorWidgets();
    },
    setDocument(id, docSnap) {
      activeDocId = id || null;
      const root = docSnap?.root || docSnap;
      if (root && root.ganbate === 'terrain' && root.grid) {
        terrainDoc = structuredClone(root);
        splatWeights = computeSplatmapWeights(
          terrainDoc.grid,
          terrainDoc.splatLayers,
          terrainDoc.water,
          terrainDoc.splines,
        );
        updateInspectorWidgets();
        drawStage();
      }
    },
    getState() {
      return {
        activeDocId,
        doc: structuredClone(terrainDoc),
        ui: structuredClone(uiState),
        splatWeights,
      };
    },
    applyBrushStroke,
    addSplinePoint,
    applySkyPreset,
    generateScatter(seed = 1337) {
      terrainDoc.scatter = generateSplineEdgeScatter(
        terrainDoc.grid,
        terrainDoc.splines,
        terrainDoc.water,
        studioApi.listAssets?.() || [],
        seed,
      );
      drawStage();
      updateInspectorWidgets();
      return terrainDoc.scatter;
    },
    repairScatterConstraints,
    runTwoAgentCoDesign,
    syncToScene,
    summarizeForAi: () => summarizeTerrainForAi(terrainDoc),
    exportGbs: () => exportTerrainToGbs(terrainDoc),
  };
}

export default mountTerrainStudio;
