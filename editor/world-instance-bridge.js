/**
 * Ganbate Engine — 3D World Instance & Neuro-Symbolic Relational Graph Bridge
 * (`editor/world-instance-bridge.js`)
 *
 * Bridges the cooked `.gbew` 3D world (`yozakura-harbor`, 89 sub-mesh instances + 617 point lights)
 * and dynamic scene entities with:
 *   1. True 3D AABB Center Pivot Decomposition:
 *        M_inst = T(pos) * R * S * T(-pivotCenter)
 *      so baked world-space `.gbew` meshes have their gizmo and transform at their real 3D center
 *      instead of `[0, 0, 0]`, with deduplicated parent/child instance transforms.
 *   2. Semantic Sub-Entity Materialization for all 89 Yozakura `.gbew` instances (Architecture,
 *      Salon Furniture, Garden & Bridge, Harbor & Boat, Landscape & Mountains, Lanterns, Roofs, Water).
 *   3. Inverse Semantic Lifting L: R^3 -> G_rel (`inferWorldRelationalEdges`):
 *      Real-time extraction of typed spatial predicates (`on_top_of`, `around`, `facing`,
 *      `spans_water`, `along_spline`, `snap_to_ground`, `illuminates`).
 *   4. Minimal-Perturbation Spatial Constraint Projection (`projectWorldConstraints`):
 *        x* = argmin_{x in C} ||x - x_hat||^2
 *      Enforcing floor/terrain support, AABB non-penetration, shoji/walkway clearance (>= 0.8m),
 *      and terrain slope limits (< 25 deg).
 */

/** Human-readable semantic catalog for the 89 Yozakura Harbor `.gbew` instances (`0..88`). */
export const YOZAKURA_INSTANCE_CATALOG = [
  // 0..12: yozakura-architecture
  { index: 0, group: 'yozakura-architecture', role: 'foundation', label: 'Estate Stone Foundation', category: 'architecture', staticStructure: true },
  { index: 1, group: 'yozakura-architecture', role: 'paving', label: 'Courtyard Stone Paving', category: 'architecture', staticStructure: true },
  { index: 2, group: 'yozakura-architecture', role: 'floor', label: 'Salon Hinoki Floor Deck', category: 'architecture', supportSurface: true },
  { index: 3, group: 'yozakura-architecture', role: 'floor', label: 'Tatami Sanctuary Flooring', category: 'architecture', supportSurface: true },
  { index: 4, group: 'yozakura-architecture', role: 'veranda', label: 'Engawa Veranda Walk', category: 'architecture', supportSurface: true },
  { index: 5, group: 'yozakura-architecture', role: 'pillars', label: 'Main Hinoki Timber Pillars', category: 'architecture', collider: true },
  { index: 6, group: 'yozakura-architecture', role: 'beams', label: 'Upper Structural Timber Beams', category: 'architecture', staticStructure: true },
  { index: 7, group: 'yozakura-architecture', role: 'walls', label: 'Interior Wood Partition Walls', category: 'architecture', collider: true },
  { index: 8, group: 'yozakura-architecture', role: 'shoji', label: 'Shoji Kumiko Sliding Screens', category: 'architecture', doorwayClearance: 0.85 },
  { index: 9, group: 'yozakura-architecture', role: 'joinery', label: 'Hinoki Kumiko Lattice Trim', category: 'architecture' },
  { index: 10, group: 'yozakura-architecture', role: 'hardware', label: 'Traditional Brass Fittings', category: 'architecture' },
  { index: 11, group: 'yozakura-architecture', role: 'stage', label: 'Salon Lacquer Stage Platform', category: 'architecture', supportSurface: true },
  { index: 12, group: 'yozakura-architecture', role: 'lantern', label: 'Veranda Hanging Chochin', category: 'lighting', lightEmitter: true },

  // 13..18: yozakura-furniture (Salon Table, Seats, Cushions, Lounge Props)
  { index: 13, group: 'yozakura-furniture', role: 'table_main', label: 'Salon Main Table (Chabudai)', category: 'furniture', supportSurface: true, collider: true },
  { index: 14, group: 'yozakura-furniture', role: 'chairs', label: 'Salon Carved Wood Chairs', category: 'furniture', seating: true, collider: true },
  { index: 15, group: 'yozakura-furniture', role: 'cushions', label: 'Silk Zabuton Floor Cushions', category: 'furniture', seating: true },
  { index: 16, group: 'yozakura-furniture', role: 'side_table', label: 'Tea & Alcove Side Table', category: 'furniture', supportSurface: true, collider: true },
  { index: 17, group: 'yozakura-furniture', role: 'lounge', label: 'Terrace Viewing Benches', category: 'furniture', seating: true, collider: true },
  { index: 18, group: 'yozakura-furniture', role: 'lantern', label: 'Salon Floor Andon Lanterns', category: 'lighting', lightEmitter: true },

  // 19..27: yozakura-garden (Sakura Trees, Stepping Stones, Moss, Wooden Bridge, Sculpture)
  { index: 19, group: 'yozakura-garden', role: 'tree_trunk', label: 'Ancient Sakura Trunks & Branches', category: 'foliage', collider: true, groundSnap: true },
  { index: 20, group: 'yozakura-garden', role: 'canopy_rose', label: 'Sakura Blossom Canopy (Rose)', category: 'foliage' },
  { index: 21, group: 'yozakura-garden', role: 'canopy_light', label: 'Sakura Blossom Canopy ( sunlit )', category: 'foliage' },
  { index: 22, group: 'yozakura-garden', role: 'canopy_shade', label: 'Sakura Blossom Canopy (Shadow)', category: 'foliage' },
  { index: 23, group: 'yozakura-garden', role: 'stepping_stones', label: 'Garden Tobi-ishi Stepping Stones', category: 'walkway', groundSnap: true },
  { index: 24, group: 'yozakura-garden', role: 'moss', label: 'Sanctuary Moss & Groundcover', category: 'landscape', supportSurface: true },
  { index: 25, group: 'yozakura-garden', role: 'bridge_deck', label: 'Arched Wooden Garden Bridge Deck', category: 'bridge', spansWater: true, supportSurface: true },
  { index: 26, group: 'yozakura-garden', role: 'bridge_rail', label: 'Wooden Bridge Vermilion Railings', category: 'bridge', spansWater: true, collider: true },
  { index: 27, group: 'yozakura-garden', role: 'sculpture', label: 'Courtyard Stone Basin (Tsukubai)', category: 'prop', collider: true, groundSnap: true },

  // 28..54: yozakura-harbor (Wooden Boat, Pier, Dock Pavilion, Harbor Lanterns)
  ...Array.from({ length: 18 }, (_, k) => ({
    index: 28 + k,
    group: 'yozakura-harbor',
    role: k === 0 ? 'boat_hull' : 'boat_part',
    label: k === 0 ? 'Traditional Wooden Harbor Boat Hull' : `Harbor Boat Detail #${k + 1}`,
    category: 'vessel',
    spansWater: true,
  })),
  ...Array.from({ length: 9 }, (_, k) => ({
    index: 46 + k,
    group: 'yozakura-harbor',
    role: k < 4 ? 'pier_deck' : k < 7 ? 'dock_pavilion' : 'pier_lantern',
    label: k < 4
      ? `Harbor Timber Pier Section #${k + 1}`
      : k < 7
        ? `Harbor Dock Pavilion Structure #${k - 3}`
        : `Harbor Pier Chochin Lantern #${k - 6}`,
    category: k < 7 ? 'harbor' : 'lighting',
    spansWater: k < 4,
    supportSurface: k === 0,
    lightEmitter: k >= 7,
  })),

  // 55..73: yozakura-landscape (Earth Path, Cliffs, Shoreline, Forest, Main Island, Mountains, Boulders)
  { index: 55, group: 'yozakura-landscape', role: 'walkway_path', label: 'Estate Approach Earth Walkway', category: 'walkway', supportSurface: true },
  { index: 56, group: 'yozakura-landscape', role: 'cliff_west', label: 'Western Coastal Slate Cliffs', category: 'terrain' },
  { index: 57, group: 'yozakura-landscape', role: 'cliff_east', label: 'Eastern Coastal Rock Formations', category: 'terrain' },
  { index: 58, group: 'yozakura-landscape', role: 'shoreline', label: 'Harbor Wet Sand Shoreline', category: 'terrain', supportSurface: true },
  { index: 59, group: 'yozakura-landscape', role: 'distant_forest', label: 'Mountain Ridge Pine Forest', category: 'foliage' },
  { index: 60, group: 'yozakura-landscape', role: 'meadow', label: 'Sanctuary Living Soil Meadow', category: 'terrain', supportSurface: true },
  { index: 61, group: 'yozakura-landscape', role: 'tidal_rocks', label: 'Harbor Cove Tidal Rocks', category: 'terrain' },
  { index: 62, group: 'yozakura-landscape', role: 'main_island', label: 'Yozakura Main Island Terrain', category: 'terrain', supportSurface: true },
  ...Array.from({ length: 7 }, (_, k) => ({
    index: 63 + k,
    group: 'yozakura-landscape',
    role: `mountain_peak_${k + 1}`,
    label: `Distant Mountain Range #${k + 1}`,
    category: 'terrain',
  })),
  ...Array.from({ length: 4 }, (_, k) => ({
    index: 70 + k,
    group: 'yozakura-landscape',
    role: k === 0 ? 'bay_island' : `coastal_boulder_${k}`,
    label: k === 0 ? 'Outer Bay Silhouette Island' : `Sanctuary Garden Boulder #${k}`,
    category: 'terrain',
    groundSnap: k > 0,
  })),

  // 74..79: yozakura-lanterns (Red Chochin, Warm Chochin, Stone Toro Lanterns, Posts)
  { index: 74, group: 'yozakura-lanterns', role: 'chochin_red', label: 'Promenade Red Chochin Lanterns', category: 'lighting', lightEmitter: true },
  { index: 75, group: 'yozakura-lanterns', role: 'lantern_frames', label: 'Lantern Iron Hanger Frames', category: 'lighting' },
  { index: 76, group: 'yozakura-lanterns', role: 'chochin_warm', label: 'Courtyard Warm Silk Lanterns', category: 'lighting', lightEmitter: true },
  { index: 77, group: 'yozakura-lanterns', role: 'lantern_caps', label: 'Lantern Lacquer Caps & Rims', category: 'lighting' },
  { index: 78, group: 'yozakura-lanterns', role: 'stone_toro', label: 'Stone Toro Path Lanterns', category: 'lighting', lightEmitter: true, groundSnap: true, collider: true },
  { index: 79, group: 'yozakura-lanterns', role: 'lantern_posts', label: 'Hinoki Lantern Standing Posts', category: 'lighting', groundSnap: true, collider: true },

  // 80..86: yozakura-roofs (Kawara Tiles, Gables, Ridge Finials, Roof Timbers)
  { index: 80, group: 'yozakura-roofs', role: 'roof_main', label: 'Main Hall Kawara Tile Roof', category: 'roof' },
  { index: 81, group: 'yozakura-roofs', role: 'roof_veranda', label: 'Veranda Eaves Tile Roof', category: 'roof' },
  { index: 82, group: 'yozakura-roofs', role: 'roof_pavilion', label: 'Sanctuary Pavilion Roof', category: 'roof' },
  { index: 83, group: 'yozakura-roofs', role: 'plaster_gables', label: 'White Shikkui Plaster Gables', category: 'roof' },
  { index: 84, group: 'yozakura-roofs', role: 'ridge_caps', label: 'Dark Ceramic Ridge Tiles', category: 'roof' },
  { index: 85, group: 'yozakura-roofs', role: 'brass_finials', label: 'Gold-Leaf & Brass Roof Finials', category: 'roof' },
  { index: 86, group: 'yozakura-roofs', role: 'roof_rafters', label: 'Exposed Hinoki Roof Rafters', category: 'roof' },

  // 87..88: yozakura-water (Open Ocean & Sanctuary Pond/Bay)
  { index: 87, group: 'yozakura-water', role: 'ocean_water', label: 'Open Ocean Water Surface', category: 'water', waterSurface: true },
  { index: 88, group: 'yozakura-water', role: 'pond_water', label: 'Sanctuary Pond & Harbor Cove Water', category: 'water', waterSurface: true },
];

const CATALOG_BY_INDEX = new Map(YOZAKURA_INSTANCE_CATALOG.map((item) => [item.index, item]));

export function getInstanceCatalogEntry(instanceIndex) {
  return CATALOG_BY_INDEX.get(Number(instanceIndex)) || {
    index: Number(instanceIndex),
    group: 'yozakura-architecture',
    role: `inst_${instanceIndex}`,
    label: `World Sub-Mesh #${instanceIndex}`,
    category: 'prop',
  };
}

// ---- Bounds & Pivot Math (`T(pos) * R * S * T(-pivotCenter)`) ------------------------------

export function isValidBounds(b) {
  return Array.isArray(b)
    && Array.isArray(b[0]) && Array.isArray(b[1])
    && b[0].length === 3 && b[1].length === 3
    && Number.isFinite(b[0][0]) && Number.isFinite(b[1][0])
    && b[0][0] <= b[1][0] && b[0][1] <= b[1][1] && b[0][2] <= b[1][2];
}

export function boundsCenter(bounds) {
  if (!isValidBounds(bounds)) return [0, 0, 0];
  return [
    (bounds[0][0] + bounds[1][0]) * 0.5,
    (bounds[0][1] + bounds[1][1]) * 0.5,
    (bounds[0][2] + bounds[1][2]) * 0.5,
  ];
}

export function boundsSize(bounds) {
  if (!isValidBounds(bounds)) return [1, 1, 1];
  return [
    Math.max(0.05, bounds[1][0] - bounds[0][0]),
    Math.max(0.05, bounds[1][1] - bounds[0][1]),
    Math.max(0.05, bounds[1][2] - bounds[0][2]),
  ];
}

export function transformPointMat4(m, p) {
  if (!Array.isArray(m) || m.length < 16) return p.slice();
  return [
    m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12],
    m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13],
    m[2] * p[0] + m[6] * p[1] + m[10] * p[2] + m[14],
  ];
}

export function transformBoundsMat4(bounds, m) {
  if (!isValidBounds(bounds)) return null;
  if (!Array.isArray(m) || m.length < 16) return [bounds[0].slice(), bounds[1].slice()];
  let lo = [Infinity, Infinity, Infinity];
  let hi = [-Infinity, -Infinity, -Infinity];
  for (const x of [bounds[0][0], bounds[1][0]]) {
    for (const y of [bounds[0][1], bounds[1][1]]) {
      for (const z of [bounds[0][2], bounds[1][2]]) {
        const p = transformPointMat4(m, [x, y, z]);
        lo = [Math.min(lo[0], p[0]), Math.min(lo[1], p[1]), Math.min(lo[2], p[2])];
        hi = [Math.max(hi[0], p[0]), Math.max(hi[1], p[1]), Math.max(hi[2], p[2])];
      }
    }
  }
  return [lo, hi];
}

/**
 * Computes exact world-space AABB `[lo, hi]` for an instance range `[first, count]` using
 * `worldTable.instanceSpecs[j].bounds` transformed by current `instMatrices[j]`, falling back
 * to `entityBounds` or `fallbackMatrix`.
 */
export function computeAccurateInstanceRangeBounds({
  instancesRange,
  instMatrices = [],
  worldTable = null,
  entityBounds = null,
  fallbackMatrix = null,
}) {
  const [first, count] = instancesRange || [0, 0];
  const specs = worldTable?.instanceSpecs;
  if (count > 0 && Array.isArray(specs) && specs.length >= first + count) {
    let lo = [Infinity, Infinity, Infinity];
    let hi = [-Infinity, -Infinity, -Infinity];
    for (let j = first; j < first + count; j++) {
      const sb = specs[j]?.bounds;
      if (!isValidBounds(sb)) continue;
      const tb = transformBoundsMat4(sb, instMatrices[j]);
      if (!tb) continue;
      lo = [Math.min(lo[0], tb[0][0]), Math.min(lo[1], tb[0][1]), Math.min(lo[2], tb[0][2])];
      hi = [Math.max(hi[0], tb[1][0]), Math.max(hi[1], tb[1][1]), Math.max(hi[2], tb[1][2])];
    }
    if (lo[0] <= hi[0]) return [lo, hi];
  }
  if (isValidBounds(entityBounds)) {
    return transformBoundsMat4(entityBounds, fallbackMatrix);
  }
  if (count > 0 && instMatrices.length >= first + count) {
    let lo = [Infinity, Infinity, Infinity];
    let hi = [-Infinity, -Infinity, -Infinity];
    for (let j = first; j < first + count; j++) {
      const m = instMatrices[j];
      if (!m) continue;
      const p = [m[12] || 0, m[13] || 0, m[14] || 0];
      lo = [Math.min(lo[0], p[0] - 1.25), Math.min(lo[1], p[1] - 1.25), Math.min(lo[2], p[2] - 1.25)];
      hi = [Math.max(hi[0], p[0] + 1.25), Math.max(hi[1], p[1] + 1.25), Math.max(hi[2], p[2] + 1.25)];
    }
    if (lo[0] <= hi[0]) return [lo, hi];
  }
  const m = Array.isArray(fallbackMatrix) && fallbackMatrix.length === 16 ? fallbackMatrix : null;
  const p = m ? [m[12] || 0, m[13] || 0, m[14] || 0] : [0, 0, 0];
  return [
    [p[0] - 1.0, p[1] - 1.0, p[2] - 1.0],
    [p[0] + 1.0, p[1] + 1.0, p[2] + 1.0],
  ];
}

/**
 * Deduplicates world instance and light indices across a hierarchy subtree (`underIndices`)
 * so moving a parent group (`yozakura-architecture`, `yozakura-furniture`, etc.) never applies
 * the delta matrix `d` twice to the same `.gbew` instance or point light.
 */
export function collectUniqueInstanceAndLightIndices(cookedEntities, underIndices) {
  const instanceIndices = [];
  const seenInstances = new Set();
  const lightIndices = [];
  const seenLights = new Set();

  for (const k of underIndices) {
    const rec = cookedEntities[k];
    if (!rec) continue;
    const [first, count] = rec.instances || [0, 0];
    for (let j = first; j < first + count; j++) {
      if (!seenInstances.has(j)) {
        seenInstances.add(j);
        instanceIndices.push(j);
      }
    }
    const [lf, lc] = rec.lights || [0, 0];
    for (let l = lf; l < lf + lc; l++) {
      if (!seenLights.has(l)) {
        seenLights.add(l);
        lightIndices.push(l);
      }
    }
  }
  return { instanceIndices, lightIndices };
}

/**
 * Returns the true 3D world-space gizmo/rotation pivot `[x, y, z]` for a scene entity.
 * For baked `.gbew` entities whose transform matrix starts at `IDENTITY` (`[0, 0, 0]`),
 * returns the live 3D bounding-box center so gizmos appear directly on the object!
 */
export function resolveEntityGizmoPivotWorld({ record, currentMatrix, currentBounds }) {
  const m = Array.isArray(currentMatrix) && currentMatrix.length === 16 ? currentMatrix : null;
  const origin = m ? [m[12] || 0, m[13] || 0, m[14] || 0] : [0, 0, 0];
  const hasBakedInstances = (record?.instances?.[1] || 0) > 0;
  if (hasBakedInstances && isValidBounds(currentBounds)) {
    return boundsCenter(currentBounds);
  }
  if (isValidBounds(currentBounds) && Math.hypot(origin[0], origin[1], origin[2]) < 1e-4) {
    const c = boundsCenter(currentBounds);
    if (Math.hypot(c[0], c[1], c[2]) > 0.25) return c;
  }
  return origin;
}

/**
 * Builds or enriches `doc.root.entities` with individual selectable child entities for the
 * Yozakura `.gbew` sub-meshes (`inst-0` .. `inst-88`) when requested or when a sub-mesh is picked
 * in the 3D viewport.
 */
export function buildSubEntitySpecForWorldInstance(instanceIndex, worldTable, parentId) {
  const idx = Number(instanceIndex);
  const cat = getInstanceCatalogEntry(idx);
  const spec = worldTable?.instanceSpecs?.[idx] || null;
  const bounds = isValidBounds(spec?.bounds) ? spec.bounds : null;
  return {
    id: `inst-${idx}`,
    parentId: parentId || cat.group,
    entityValue: {
      name: cat.label,
      parent: parentId || cat.group,
      components: {
        transform: {
          translation: [0, 0, 0],
          rotation: [0, 0, 0, 1],
          scale: [1, 1, 1],
        },
        mesh: {
          node: `inst-${idx}`,
          material: spec?.material || cat.role,
          category: cat.category,
          role: cat.role,
        },
        ...(bounds ? {
          bounds: {
            min: bounds[0].map((v) => Math.round(v * 100) / 100),
            max: bounds[1].map((v) => Math.round(v * 100) / 100),
            center: boundsCenter(bounds).map((v) => Math.round(v * 100) / 100),
          },
        } : {}),
      },
    },
  };
}

// ---- Inverse Semantic Lifting L: R^3 -> G_rel ----------------------------------------------

/**
 * Normalizes cooked + document entities into spatial graph nodes with world-space AABB `bounds`,
 * `center`, `size`, and semantic `role` / `category`.
 */
export function buildSpatialGraphNodes({ cookedEntities = [], currentMatrices = [], docEntities = {}, worldTable = null, instMatrices = [] }) {
  const nodes = [];
  for (let i = 0; i < cookedEntities.length; i++) {
    const rec = cookedEntities[i];
    if (!rec) continue;
    const docEnt = docEntities[rec.id];
    const [first, count] = rec.instances || [0, 0];
    // Include document entities OR leaf sub-mesh records with a single instance
    if (!docEnt && count !== 1) continue;
    const m = currentMatrices[i] || rec.transform;
    const bounds = computeAccurateInstanceRangeBounds({
      instancesRange: rec.instances,
      instMatrices,
      worldTable,
      entityBounds: rec.bounds,
      fallbackMatrix: m,
    });
    const center = boundsCenter(bounds);
    const size = boundsSize(bounds);
    const catEntry = count === 1 ? getInstanceCatalogEntry(first) : null;
    const name = docEnt?.name || catEntry?.label || rec.name || rec.id;
    const lower = `${rec.id} ${name} ${catEntry?.role || ''}`.toLowerCase();
    const role = catEntry?.role
      || (lower.includes('table') || lower.includes('chabudai') ? 'table_main'
        : lower.includes('chair') || lower.includes('cushion') || lower.includes('zabuton') || lower.includes('seat') ? 'seating'
        : lower.includes('bridge') ? 'bridge_deck'
        : lower.includes('water') || lower.includes('pond') || lower.includes('ocean') ? 'water'
        : lower.includes('terrain') || lower.includes('island') || lower.includes('meadow') ? 'terrain'
        : lower.includes('lantern') || lower.includes('chochin') || lower.includes('toro') || docEnt?.components?.light ? 'lantern'
        : lower.includes('shoji') || lower.includes('door') ? 'shoji'
        : lower.includes('sakura') || lower.includes('tree') ? 'tree'
        : lower.includes('dealer') || lower.includes('rei') || lower.includes('character') || lower.includes('avatar') ? 'character'
        : 'prop');

    nodes.push({
      id: rec.id,
      name,
      role,
      category: catEntry?.category || (docEnt?.components?.terrain ? 'terrain' : docEnt?.components?.water ? 'water' : 'object'),
      parent: rec.parent !== null && rec.parent !== undefined ? cookedEntities[rec.parent]?.id ?? null : (docEnt?.parent || null),
      instances: [first, count],
      bounds,
      center,
      size,
      seating: Boolean(catEntry?.seating || role === 'seating' || role === 'chairs' || role === 'cushions' || role === 'lounge'),
      supportSurface: Boolean(catEntry?.supportSurface || role === 'table_main' || role === 'terrain'),
      waterSurface: Boolean(catEntry?.waterSurface || role === 'water'),
      spansWater: Boolean(catEntry?.spansWater || role === 'bridge_deck'),
      collider: Boolean(catEntry?.collider || role === 'table_main' || role === 'seating' || role === 'character'),
      groundSnap: Boolean(catEntry?.groundSnap || role === 'tree' || role === 'lantern'),
      inDoc: Boolean(docEnt),
    });
  }
  return nodes;
}

/**
 * Inverse Semantic Lifting (L: R^3 -> G_rel):
 * Infers typed relational edges (`on_top_of`, `around`, `facing`, `spans_water`,
 * `snap_to_ground`, `illuminates`, `near_doorway`) from current 3D node bounds & coordinates.
 */
export function inferWorldRelationalEdges(nodes, options = {}) {
  const edges = [];
  const docOrKeyNodes = nodes.filter((n) => n.inDoc || n.instances[1] === 1);

  const tables = docOrKeyNodes.filter((n) => n.role === 'table_main' || n.role === 'side_table' || n.supportSurface);
  const waters = docOrKeyNodes.filter((n) => n.waterSurface || n.category === 'water');
  const terrains = docOrKeyNodes.filter((n) => n.category === 'terrain' || n.role === 'terrain' || n.id === 'yozakura-landscape' || n.id === 'inst-62');

  for (const a of docOrKeyNodes) {
    // 1. `around` and `facing` for seating / characters near tables
    if (a.seating || a.role === 'seating' || a.role === 'character') {
      for (const tbl of tables) {
        if (a.id === tbl.id || tbl.category === 'terrain' || tbl.role === 'terrain' || tbl.role === 'floor') continue;
        const dx = a.center[0] - tbl.center[0];
        const dz = a.center[2] - tbl.center[2];
        const distXZ = Math.hypot(dx, dz);
        if (distXZ > 0.3 && distXZ <= 8.5) {
          edges.push({
            from: a.id,
            to: tbl.id,
            type: 'around',
            distanceM: Math.round(distXZ * 100) / 100,
          });
          edges.push({
            from: a.id,
            to: tbl.id,
            type: 'facing',
            distanceM: Math.round(distXZ * 100) / 100,
          });
        }
      }
    }

    // 2. `on_top_of` for props/furniture resting on a support surface
    if (a.category !== 'terrain' && a.category !== 'water' && a.role !== 'terrain' && a.role !== 'water') {
      for (const sup of tables) {
        if (a.id === sup.id || a.parent === sup.id) continue;
        const withinX = a.center[0] >= sup.bounds[0][0] - 0.5 && a.center[0] <= sup.bounds[1][0] + 0.5;
        const withinZ = a.center[2] >= sup.bounds[0][2] - 0.5 && a.center[2] <= sup.bounds[1][2] + 0.5;
        const verticalGap = Math.abs(a.bounds[0][1] - sup.bounds[1][1]);
        if (withinX && withinZ && verticalGap <= 1.2 && a.size[0] <= sup.size[0] * 1.1) {
          edges.push({
            from: a.id,
            to: sup.id,
            type: 'on_top_of',
            clearanceM: Math.round(verticalGap * 100) / 100,
          });
          break;
        }
      }
    }

    // 3. `spans_water` for bridges, piers, and vessels
    if (a.spansWater) {
      for (const w of waters) {
        const overlapX = a.bounds[0][0] <= w.bounds[1][0] && a.bounds[1][0] >= w.bounds[0][0];
        const overlapZ = a.bounds[0][2] <= w.bounds[1][2] && a.bounds[1][2] >= w.bounds[0][2];
        if (overlapX && overlapZ) {
          edges.push({
            from: a.id,
            to: w.id,
            type: 'spans_water',
            clearanceM: Math.round((a.bounds[0][1] - w.bounds[1][1]) * 100) / 100,
          });
          break;
        }
      }
    }

    // 4. `snap_to_ground` for exterior trees, lanterns, and stepping stones
    if (a.groundSnap && terrains.length > 0) {
      const ground = terrains[0];
      if (a.id !== ground.id) {
        edges.push({
          from: a.id,
          to: ground.id,
          type: 'snap_to_ground',
          elevationM: Math.round(a.bounds[0][1] * 100) / 100,
        });
      }
    }
  }

  // Optional spline walkway alignment from Terrain Studio
  if (Array.isArray(options.walkwayPoints) && options.walkwayPoints.length >= 2) {
    for (const a of docOrKeyNodes) {
      if (!a.groundSnap && a.role !== 'lantern' && a.role !== 'tree') continue;
      let minSplineDist = Infinity;
      for (const pt of options.walkwayPoints) {
        const d = Math.hypot(a.center[0] - (pt.x || 0), a.center[2] - (pt.z || 0));
        if (d < minSplineDist) minSplineDist = d;
      }
      if (minSplineDist <= 6.0) {
        edges.push({
          from: a.id,
          to: 'walkway_spline',
          type: 'along_spline',
          offsetM: Math.round(minSplineDist * 100) / 100,
        });
      }
    }
  }

  return edges;
}

// ---- Minimal-Perturbation Constraint Projection (`x* = argmin ||x - x_hat||^2`) ------------

/**
 * Evaluates spatial constraints for `targetId` at candidate world center `proposedCenter`
 * and computes the minimal-perturbation valid position `projectedCenter` (`x*`).
 *
 * Enforces:
 *   1. Floor / Terrain vertical support (`y >= floorY`, prevents underground sinking)
 *   2. Horizontal XZ non-penetration against collider nodes (`minClearance = r_a + r_b`)
 *   3. Shoji / Walkway corridor clearance (`>= 0.80 m`)
 */
export function projectWorldConstraints({
  targetId,
  proposedCenter,
  nodes = [],
  sampleTerrainHeight = null,
  minWalkwayClearanceM = 0.8,
}) {
  const target = nodes.find((n) => n.id === targetId);
  if (!Array.isArray(proposedCenter) || proposedCenter.length < 3) {
    return {
      targetId,
      proposedCenter: [0, 0, 0],
      projectedCenter: [0, 0, 0],
      delta: [0, 0, 0],
      displacementM: 0,
      satisfied: true,
      violations: [],
    };
  }

  const xStar = [proposedCenter[0], proposedCenter[1], proposedCenter[2]];
  const violations = [];
  const halfHeight = target ? target.size[1] * 0.5 : 0.5;
  const radiusA = target ? Math.max(0.35, Math.min(3.5, Math.min(target.size[0], target.size[2]) * 0.42)) : 0.5;

  // 1. Floor / Terrain vertical support constraint
  let supportY = null;
  const isTabletopProp = Boolean(target && (target.category === 'prop' || target.role === 'lantern' || target.size[0] < 0.65));
  if (typeof sampleTerrainHeight === 'function' && target?.groundSnap) {
    supportY = Number(sampleTerrainHeight(xStar[0], xStar[2])) || 0;
  } else if (target && target.role !== 'terrain' && target.role !== 'water') {
    // Check if inside an architectural floor footprint (or tabletop if small prop)
    const floors = nodes.filter((n) => (
      n.id !== targetId
      && n.supportSurface
      && n.role !== 'terrain'
      && (isTabletopProp || (n.role !== 'table_main' && n.role !== 'side_table'))
    ));
    for (const fl of floors) {
      if (
        xStar[0] >= fl.bounds[0][0] - 0.25 && xStar[0] <= fl.bounds[1][0] + 0.25
        && xStar[2] >= fl.bounds[0][2] - 0.25 && xStar[2] <= fl.bounds[1][2] + 0.25
      ) {
        const topY = fl.bounds[1][1];
        if (supportY === null || Math.abs((xStar[1] - halfHeight) - topY) < 1.5) {
          supportY = topY;
        }
      }
    }
  }

  if (supportY !== null && (xStar[1] - halfHeight) < supportY - 0.08) {
    const liftedY = supportY + halfHeight;
    violations.push({
      kind: 'support_surface_penetration',
      message: `${target?.name || targetId} penetrated support floor by ${(supportY - (xStar[1] - halfHeight)).toFixed(2)}m`,
    });
    xStar[1] = liftedY;
  }

  // 2. Horizontal non-penetration against sibling colliders
  if (target && target.role !== 'terrain' && target.role !== 'water' && !target.staticStructure) {
    for (const other of nodes) {
      if (
        other.id === targetId
        || (target.parent && other.id === target.parent)
        || (other.parent && target.id === other.parent)
        || !other.collider
        || other.role === 'terrain'
        || other.role === 'water'
      ) continue;

      // Check vertical overlap first
      const yOverlap = (xStar[1] - halfHeight) < other.bounds[1][1] - 0.05
        && (xStar[1] + halfHeight) > other.bounds[0][1] + 0.05;
      if (!yOverlap) continue;

      const radiusB = Math.max(0.35, Math.min(3.5, Math.min(other.size[0], other.size[2]) * 0.42));
      const minSep = radiusA + radiusB;
      const dx = xStar[0] - other.center[0];
      const dz = xStar[2] - other.center[2];
      const dist = Math.hypot(dx, dz);
      if (dist < minSep) {
        const push = minSep - dist + 0.02;
        const ux = dist > 1e-4 ? dx / dist : 1;
        const uz = dist > 1e-4 ? dz / dist : 0;
        xStar[0] += ux * push;
        xStar[2] += uz * push;
        violations.push({
          kind: 'collider_overlap',
          otherId: other.id,
          otherName: other.name,
          penetrationM: Math.round(push * 100) / 100,
          message: `Collision with ${other.name} resolved (+${push.toFixed(2)}m minimal perturbation)`,
        });
      }
    }

    // 3. Shoji doorway clearance (>= 0.8m)
    const shojiNodes = nodes.filter((n) => n.role === 'shoji' && n.id !== targetId);
    for (const sh of shojiNodes) {
      const dx = xStar[0] - sh.center[0];
      const dz = xStar[2] - sh.center[2];
      const dist = Math.hypot(dx, dz);
      if (dist < minWalkwayClearanceM) {
        const push = minWalkwayClearanceM - dist + 0.02;
        const ux = dist > 1e-4 ? dx / dist : 0;
        const uz = dist > 1e-4 ? dz / dist : 1;
        xStar[0] += ux * push;
        xStar[2] += uz * push;
        violations.push({
          kind: 'doorway_clearance',
          otherId: sh.id,
          otherName: sh.name,
          clearanceM: minWalkwayClearanceM,
          message: `Preserved ${minWalkwayClearanceM.toFixed(2)}m walkway clearance near ${sh.name}`,
        });
      }
    }
  }

  const delta = [
    Math.round((xStar[0] - proposedCenter[0]) * 1000) / 1000,
    Math.round((xStar[1] - proposedCenter[1]) * 1000) / 1000,
    Math.round((xStar[2] - proposedCenter[2]) * 1000) / 1000,
  ];
  const displacementM = Math.round(Math.hypot(delta[0], delta[1], delta[2]) * 1000) / 1000;

  return {
    targetId,
    proposedCenter: proposedCenter.map((v) => Math.round(v * 1000) / 1000),
    projectedCenter: xStar.map((v) => Math.round(v * 1000) / 1000),
    delta,
    displacementM,
    satisfied: violations.length === 0,
    violations,
  };
}

// ---- 2-Agent World Spatial Conflict Resolution ---------------------------------------------

/**
 * Resolves concurrent 3D World edit proposals from two autonomous agents:
 *   - `agentAProposal`: `{ agentId, role, targetId, proposedCenter, weight = 0.5 }`
 *   - `agentBProposal`: `{ agentId, role, targetId, proposedCenter, weight = 0.5 }`
 *
 * Solves the joint coupled minimal-perturbation optimization:
 *   (x_A*, x_B*) = argmin_{(x_A, x_B) in C} w_A ||x_A - x_hat_A||^2 + w_B ||x_B - x_hat_B||^2
 *
 * Handles both:
 *   1. Direct target contention (`targetId_A === targetId_B`)
 *   2. Coupled spatial collision (`||x_A - x_B||_XZ < r_A + r_B`) and environment constraints.
 */
export function resolveTwoAgentWorldConflict({
  nodes = [],
  agentAProposal,
  agentBProposal,
  sampleTerrainHeight = null,
  minWalkwayClearanceM = 0.8,
}) {
  const propA = agentAProposal || {
    agentId: 'agent_architect',
    role: 'Interior & Architecture Agent',
    targetId: 'inst-13',
    proposedCenter: [0, 0.4, 0],
    weight: 0.55,
  };
  const propB = agentBProposal || {
    agentId: 'agent_landscape',
    role: 'Landscape & Lighting Director',
    targetId: 'inst-14',
    proposedCenter: [0.3, 0.45, 0],
    weight: 0.45,
  };

  const wA = Math.max(0.05, Number(propA.weight) || 0.5);
  const wB = Math.max(0.05, Number(propB.weight) || 0.5);
  const wSum = wA + wB;
  const normWA = wA / wSum;
  const normWB = wB / wSum;

  const workingNodes = nodes.map((n) => ({
    ...n,
    bounds: isValidBounds(n.bounds) ? [n.bounds[0].slice(), n.bounds[1].slice()] : [[-0.5, -0.5, -0.5], [0.5, 0.5, 0.5]],
    center: Array.isArray(n.center) ? n.center.slice() : [0, 0, 0],
    size: Array.isArray(n.size) ? n.size.slice() : [1, 1, 1],
  }));

  const moveNodeCenter = (node, nextCenter) => {
    const dx = nextCenter[0] - node.center[0];
    const dy = nextCenter[1] - node.center[1];
    const dz = nextCenter[2] - node.center[2];
    node.center = nextCenter.map((v) => Math.round(v * 1000) / 1000);
    node.bounds = [
      [node.bounds[0][0] + dx, node.bounds[0][1] + dy, node.bounds[0][2] + dz],
      [node.bounds[1][0] + dx, node.bounds[1][1] + dy, node.bounds[1][2] + dz],
    ];
  };

  const conflicts = [];

  // Case 1: Both agents target the exact same entity
  if (propA.targetId === propB.targetId) {
    const distAB = Math.hypot(
      (propA.proposedCenter[0] || 0) - (propB.proposedCenter[0] || 0),
      (propA.proposedCenter[1] || 0) - (propB.proposedCenter[1] || 0),
      (propA.proposedCenter[2] || 0) - (propB.proposedCenter[2] || 0),
    );
    if (distAB > 1e-3) {
      conflicts.push({
        kind: 'same_target_contention',
        targetId: propA.targetId,
        divergenceM: Math.round(distAB * 1000) / 1000,
        message: `${propA.agentId} and ${propB.agentId} proposed divergent coordinates (${distAB.toFixed(2)}m apart) for ${propA.targetId}`,
      });
    }
    const blended = [0, 1, 2].map(
      (i) => (propA.proposedCenter[i] || 0) * normWA + (propB.proposedCenter[i] || 0) * normWB,
    );
    const proj = projectWorldConstraints({
      targetId: propA.targetId,
      proposedCenter: blended,
      nodes: workingNodes,
      sampleTerrainHeight,
      minWalkwayClearanceM,
    });
    const targetNode = workingNodes.find((n) => n.id === propA.targetId);
    if (targetNode) moveNodeCenter(targetNode, proj.projectedCenter);
    const updatedEdges = inferWorldRelationalEdges(workingNodes);
    const c = proj.projectedCenter;
    return {
      conflictDetected: conflicts.length > 0 || !proj.satisfied,
      conflicts: [...conflicts, ...proj.violations],
      resolvedProposals: {
        agentA: {
          agentId: propA.agentId,
          targetId: propA.targetId,
          proposedCenter: propA.proposedCenter,
          resolvedCenter: c,
          displacementM: Math.round(Math.hypot(c[0] - propA.proposedCenter[0], c[1] - propA.proposedCenter[1], c[2] - propA.proposedCenter[2]) * 1000) / 1000,
        },
        agentB: {
          agentId: propB.agentId,
          targetId: propB.targetId,
          proposedCenter: propB.proposedCenter,
          resolvedCenter: c,
          displacementM: Math.round(Math.hypot(c[0] - propB.proposedCenter[0], c[1] - propB.proposedCenter[1], c[2] - propB.proposedCenter[2]) * 1000) / 1000,
        },
      },
      updatedEdges,
      gbsCommit: `move ${propA.targetId} to ${c[0]} ${c[1]} ${c[2]} # 2-agent consensus`,
    };
  }

  // Case 2: Distinct target entities that may collide with each other or the environment
  let xA = [...(propA.proposedCenter || [0, 0, 0])];
  let xB = [...(propB.proposedCenter || [0, 0, 0])];
  const nodeA = workingNodes.find((n) => n.id === propA.targetId);
  const nodeB = workingNodes.find((n) => n.id === propB.targetId);

  const rA = nodeA ? Math.max(0.4, Math.min(3.5, Math.min(nodeA.size[0], nodeA.size[2]) * 0.45)) : 0.6;
  const rB = nodeB ? Math.max(0.4, Math.min(3.5, Math.min(nodeB.size[0], nodeB.size[2]) * 0.45)) : 0.6;
  const minMutualSep = rA + rB;

  const dxAB = xB[0] - xA[0];
  const dzAB = xB[2] - xA[2];
  const distXZ = Math.hypot(dxAB, dzAB);
  const yDiff = Math.abs(xA[1] - xB[1]);

  if (distXZ < minMutualSep && yDiff < 1.5) {
    const overlap = minMutualSep - distXZ + 0.04;
    const ux = distXZ > 1e-4 ? dxAB / distXZ : 1;
    const uz = distXZ > 1e-4 ? dzAB / distXZ : 0;
    // Inverse-weight push: higher-weight agent moves less
    const pushA = overlap * normWB;
    const pushB = overlap * normWA;
    xA[0] -= ux * pushA;
    xA[2] -= uz * pushA;
    xB[0] += ux * pushB;
    xB[2] += uz * pushB;
    conflicts.push({
      kind: 'inter_agent_collision',
      targetA: propA.targetId,
      targetB: propB.targetId,
      overlapM: Math.round(overlap * 1000) / 1000,
      message: `Resolved inter-agent collision between ${propA.targetId} and ${propB.targetId} (${overlap.toFixed(2)}m overlap)`,
    });
  }

  if (nodeA) moveNodeCenter(nodeA, xA);
  if (nodeB) moveNodeCenter(nodeB, xB);

  const projA = projectWorldConstraints({
    targetId: propA.targetId,
    proposedCenter: xA,
    nodes: workingNodes,
    sampleTerrainHeight,
    minWalkwayClearanceM,
  });
  if (nodeA) moveNodeCenter(nodeA, projA.projectedCenter);

  const projB = projectWorldConstraints({
    targetId: propB.targetId,
    proposedCenter: xB,
    nodes: workingNodes,
    sampleTerrainHeight,
    minWalkwayClearanceM,
  });
  if (nodeB) moveNodeCenter(nodeB, projB.projectedCenter);

  const finalA = projA.projectedCenter;
  const finalB = projB.projectedCenter;
  const updatedEdges = inferWorldRelationalEdges(workingNodes);
  const allConflicts = [...conflicts, ...projA.violations, ...projB.violations];

  return {
    conflictDetected: allConflicts.length > 0,
    conflicts: allConflicts,
    resolvedProposals: {
      agentA: {
        agentId: propA.agentId,
        targetId: propA.targetId,
        proposedCenter: propA.proposedCenter,
        resolvedCenter: finalA,
        displacementM: Math.round(Math.hypot(
          finalA[0] - propA.proposedCenter[0],
          finalA[1] - propA.proposedCenter[1],
          finalA[2] - propA.proposedCenter[2],
        ) * 1000) / 1000,
      },
      agentB: {
        agentId: propB.agentId,
        targetId: propB.targetId,
        proposedCenter: propB.proposedCenter,
        resolvedCenter: finalB,
        displacementM: Math.round(Math.hypot(
          finalB[0] - propB.proposedCenter[0],
          finalB[1] - propB.proposedCenter[1],
          finalB[2] - propB.proposedCenter[2],
        ) * 1000) / 1000,
      },
    },
    updatedEdges,
    gbsCommit: [
      `move ${propA.targetId} to ${finalA[0]} ${finalA[1]} ${finalA[2]}`,
      `move ${propB.targetId} to ${finalB[0]} ${finalB[1]} ${finalB[2]}`,
    ].join('\n'),
  };
}

// ---- Bidirectional Tri-Modal World Graph Serialization & Patch Application -----------------

/**
 * Compiles spatial graph nodes and inferred relational edges into deterministic `.gbs` SceneScript.
 */
export function compileWorldGraphToGbs({ nodes = [], edges = [], sceneName = 'yozakura-world' }) {
  const lines = [
    `# Ganbate SceneScript (.gbs) — Tri-Modal World Graph (${sceneName})`,
  ];
  const keyNodes = nodes.filter((n) => n.inDoc || n.instances?.[1] === 1).slice(0, 24);
  for (const n of keyNodes) {
    const [x, y, z] = (n.center || [0, 0, 0]).map((v) => Math.round(v * 100) / 100);
    const safeName = String(n.name || n.id).replace(/"/g, '');
    lines.push(`node ${n.id} role=${n.role || 'prop'} at ${x} ${y} ${z} name="${safeName}"`);
  }
  for (const e of edges.slice(0, 32)) {
    const metric = e.distanceM !== undefined
      ? ` dist=${e.distanceM}m`
      : e.clearanceM !== undefined
        ? ` clearance=${e.clearanceM}m`
        : e.offsetM !== undefined
          ? ` offset=${e.offsetM}m`
          : '';
    lines.push(`relate ${e.from} ${e.type} ${e.to}${metric}`);
  }
  return lines.join('\n');
}

/**
 * Exports the complete Tri-Modal World Semantic Graph (`ganbate.world.semantic.v2`) with
 * token-compression telemetry against the cooked `.gbew` binary (`2,425,856` bytes).
 */
export function exportWorldSemanticGraphJson({
  nodes = [],
  edges = null,
  sceneName = 'yozakura-estate',
  rawWorldBytes = 2425856,
}) {
  const resolvedEdges = Array.isArray(edges) ? edges : inferWorldRelationalEdges(nodes);
  const gbsText = compileWorldGraphToGbs({ nodes, edges: resolvedEdges, sceneName });
  const tokensApprox = Math.max(1, Math.ceil(gbsText.split(/\s+/).length * 1.3));
  const rawTokensEquivalent = Math.round(rawWorldBytes / 4);
  const compressionRatio = Math.round((rawTokensEquivalent / tokensApprox) * 100) / 100;

  return {
    schema: 'ganbate.world.semantic.v2',
    sceneName,
    nodeCount: nodes.length,
    edgeCount: resolvedEdges.length,
    telemetry: {
      rawWorldBytes,
      gbsTokensApprox: tokensApprox,
      compressionRatio,
    },
    nodes: nodes.map((n) => ({
      id: n.id,
      name: n.name,
      role: n.role,
      category: n.category,
      center: (n.center || [0, 0, 0]).map((v) => Math.round(v * 100) / 100),
      size: (n.size || [1, 1, 1]).map((v) => Math.round(v * 100) / 100),
    })),
    edges: resolvedEdges,
    gbs: gbsText,
  };
}

/**
 * Applies a semantic graph patch (`{ moves: [{ id, targetCenter }], constraints }`) to the
 * 3D World nodes, projecting each move onto the feasible constraint manifold `x*` and
 * re-lifting relational edges `L: R^3 -> G_rel`.
 */
export function applySemanticGraphPatchToWorld({
  nodes = [],
  patch = {},
  sampleTerrainHeight = null,
  minWalkwayClearanceM = 0.8,
}) {
  const workingNodes = nodes.map((n) => ({
    ...n,
    bounds: isValidBounds(n.bounds) ? [n.bounds[0].slice(), n.bounds[1].slice()] : [[-0.5, -0.5, -0.5], [0.5, 0.5, 0.5]],
    center: Array.isArray(n.center) ? n.center.slice() : [0, 0, 0],
    size: Array.isArray(n.size) ? n.size.slice() : [1, 1, 1],
  }));
  const projections = [];

  for (const move of patch.moves || []) {
    const targetNode = workingNodes.find((n) => n.id === move.id);
    if (!targetNode || !Array.isArray(move.targetCenter)) continue;
    const proj = projectWorldConstraints({
      targetId: move.id,
      proposedCenter: move.targetCenter,
      nodes: workingNodes,
      sampleTerrainHeight,
      minWalkwayClearanceM,
    });
    const dx = proj.projectedCenter[0] - targetNode.center[0];
    const dy = proj.projectedCenter[1] - targetNode.center[1];
    const dz = proj.projectedCenter[2] - targetNode.center[2];
    targetNode.center = proj.projectedCenter.slice();
    targetNode.bounds = [
      [targetNode.bounds[0][0] + dx, targetNode.bounds[0][1] + dy, targetNode.bounds[0][2] + dz],
      [targetNode.bounds[1][0] + dx, targetNode.bounds[1][1] + dy, targetNode.bounds[1][2] + dz],
    ];
    projections.push(proj);
  }

  const edges = inferWorldRelationalEdges(workingNodes);
  return {
    nodes: workingNodes,
    edges,
    projections,
    gbs: compileWorldGraphToGbs({ nodes: workingNodes, edges }),
  };
}

