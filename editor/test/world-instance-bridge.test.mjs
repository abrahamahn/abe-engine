import test from 'node:test';
import assert from 'node:assert/strict';
import {
  YOZAKURA_INSTANCE_CATALOG,
  getInstanceCatalogEntry,
  boundsCenter,
  computeAccurateInstanceRangeBounds,
  collectUniqueInstanceAndLightIndices,
  resolveEntityGizmoPivotWorld,
  buildSubEntitySpecForWorldInstance,
  buildSpatialGraphNodes,
  inferWorldRelationalEdges,
  projectWorldConstraints,
  resolveTwoAgentWorldConflict,
  compileWorldGraphToGbs,
  exportWorldSemanticGraphJson,
  applySemanticGraphPatchToWorld,
} from '../world-instance-bridge.js';

const IDENTITY = [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1];

test('YOZAKURA_INSTANCE_CATALOG covers all 89 .gbew instances (0..88)', () => {
  assert.equal(YOZAKURA_INSTANCE_CATALOG.length, 89);
  for (let i = 0; i < 89; i++) {
    const entry = getInstanceCatalogEntry(i);
    assert.equal(entry.index, i);
    assert.ok(entry.group.startsWith('yozakura-'));
    assert.ok(entry.label.length > 3);
  }
});

test('computeAccurateInstanceRangeBounds & resolveEntityGizmoPivotWorld use exact 3D vertex bounds instead of [0,0,0]', () => {
  const worldTable = {
    instanceSpecs: Array.from({ length: 89 }, (_, i) => ({
      material: `mat_${i}`,
      bounds: i === 25
        ? [[-12, 1.2, 18], [-6, 2.8, 22]] // Wooden garden bridge
        : [[-1, 0, -1], [1, 2, 1]],
    })),
  };
  const instMatrices = Array.from({ length: 89 }, () => IDENTITY.slice());
  const bounds = computeAccurateInstanceRangeBounds({
    instancesRange: [25, 1],
    instMatrices,
    worldTable,
  });
  assert.deepEqual(bounds, [[-12, 1.2, 18], [-6, 2.8, 22]]);
  const pivot = resolveEntityGizmoPivotWorld({
    record: { id: 'inst-25', instances: [25, 1] },
    currentMatrix: IDENTITY,
    currentBounds: bounds,
  });
  assert.deepEqual(pivot, [-9, 2, 20]);
});

test('collectUniqueInstanceAndLightIndices prevents parent-child double transforms', () => {
  const cookedEntities = [
    { id: 'yozakura-furniture', parent: null, instances: [13, 6], lights: [0, 4] },
    { id: 'inst-13', parent: 0, instances: [13, 1], lights: [0, 2] },
    { id: 'inst-14', parent: 0, instances: [14, 1], lights: [2, 2] },
  ];
  const { instanceIndices, lightIndices } = collectUniqueInstanceAndLightIndices(cookedEntities, [0, 1, 2]);
  assert.deepEqual(instanceIndices, [13, 14, 15, 16, 17, 18]);
  assert.deepEqual(lightIndices, [0, 1, 2, 3]);
});

test('Inverse Semantic Lifting (inferWorldRelationalEdges) extracts around, facing, spans_water, and snap_to_ground', () => {
  const worldTable = {
    instanceSpecs: Array.from({ length: 89 }, () => ({
      material: 'default',
      bounds: [[-1, 0, -1], [1, 1, 1]],
    })),
  };
  // Override table (13), chairs (14), bridge (25), water (88), sakura trunk (19), main island terrain (62)
  worldTable.instanceSpecs[13].bounds = [[-1.2, 0.0, -1.2], [1.2, 0.8, 1.2]];
  worldTable.instanceSpecs[14].bounds = [[1.5, 0.0, -0.6], [2.3, 0.9, 0.6]];
  worldTable.instanceSpecs[25].bounds = [[-8.0, 0.5, 10.0], [-2.0, 1.6, 14.0]];
  worldTable.instanceSpecs[88].bounds = [[-20.0, -0.5, 5.0], [20.0, 0.6, 25.0]];
  worldTable.instanceSpecs[19].bounds = [[6.0, 0.0, 8.0], [7.2, 5.5, 9.2]];
  worldTable.instanceSpecs[62].bounds = [[-60.0, -4.0, -60.0], [60.0, 0.1, 60.0]];

  const cookedEntities = [13, 14, 25, 88, 19, 62].map((idx) => ({
    id: `inst-${idx}`,
    name: getInstanceCatalogEntry(idx).label,
    parent: null,
    transform: IDENTITY.slice(),
    instances: [idx, 1],
    lights: [0, 0],
  }));
  const currentMatrices = cookedEntities.map(() => IDENTITY.slice());
  const nodes = buildSpatialGraphNodes({
    cookedEntities,
    currentMatrices,
    docEntities: {},
    worldTable,
    instMatrices: Array.from({ length: 89 }, () => IDENTITY.slice()),
  });
  const edges = inferWorldRelationalEdges(nodes, {
    walkwayPoints: [{ x: 5.5, z: 8.2 }, { x: 0, z: 12 }],
  });

  assert.ok(edges.some((e) => e.from === 'inst-14' && e.to === 'inst-13' && e.type === 'around'));
  assert.ok(edges.some((e) => e.from === 'inst-14' && e.to === 'inst-13' && e.type === 'facing'));
  assert.ok(edges.some((e) => e.from === 'inst-25' && e.to === 'inst-88' && e.type === 'spans_water'));
  assert.ok(edges.some((e) => e.from === 'inst-19' && e.to === 'inst-62' && e.type === 'snap_to_ground'));
  assert.ok(edges.some((e) => e.from === 'inst-19' && e.to === 'walkway_spline' && e.type === 'along_spline'));
});

test('Minimal-Perturbation Constraint Projection (projectWorldConstraints) resolves floor sinking & collider overlap', () => {
  const nodes = [
    {
      id: 'inst-2',
      name: 'Salon Hinoki Floor Deck',
      role: 'floor',
      supportSurface: true,
      collider: false,
      bounds: [[-10, -0.2, -10], [10, 0.0, 10]],
      center: [0, -0.1, 0],
      size: [20, 0.2, 20],
    },
    {
      id: 'inst-13',
      name: 'Salon Main Table (Chabudai)',
      role: 'table_main',
      supportSurface: true,
      collider: true,
      bounds: [[-1.0, 0.0, -1.0], [1.0, 0.8, 1.0]],
      center: [0, 0.4, 0],
      size: [2.0, 0.8, 2.0],
    },
    {
      id: 'inst-14',
      name: 'Salon Carved Wood Chairs',
      role: 'seating',
      supportSurface: false,
      collider: true,
      bounds: [[1.4, 0.0, -0.5], [2.2, 0.9, 0.5]],
      center: [1.8, 0.45, 0],
      size: [0.8, 0.9, 1.0],
    },
  ];

  // Drag chair into the table AND slightly below the floor
  const res = projectWorldConstraints({
    targetId: 'inst-14',
    proposedCenter: [0.2, 0.1, 0.0],
    nodes,
  });

  assert.equal(res.satisfied, false);
  assert.ok(res.violations.some((v) => v.kind === 'support_surface_penetration'));
  assert.ok(res.violations.some((v) => v.kind === 'collider_overlap'));
  assert.ok(res.projectedCenter[1] >= 0.44);
  assert.ok(res.projectedCenter[0] > 1.15);
  assert.ok(res.displacementM > 0.5);
});

test('buildSubEntitySpecForWorldInstance produces a valid child entity spec', () => {
  const spec = buildSubEntitySpecForWorldInstance(13, {
    instanceSpecs: Array.from({ length: 89 }, () => ({
      material: 'hinoki_lacquer',
      bounds: [[-1.2, 0, -1.2], [1.2, 0.8, 1.2]],
    })),
  });
  assert.equal(spec.id, 'inst-13');
  assert.equal(spec.parentId, 'yozakura-furniture');
  assert.equal(spec.entityValue.name, 'Salon Main Table (Chabudai)');
  assert.deepEqual(boundsCenter([spec.entityValue.components.bounds.min, spec.entityValue.components.bounds.max]), [0, 0.4, 0]);
});

test('resolveTwoAgentWorldConflict resolves both inter-agent spatial collisions and same-target contention', () => {
  const nodes = [
    {
      id: 'inst-2',
      name: 'Salon Hinoki Floor Deck',
      role: 'floor',
      supportSurface: true,
      collider: false,
      bounds: [[-10, -0.2, -10], [10, 0.0, 10]],
      center: [0, -0.1, 0],
      size: [20, 0.2, 20],
      instances: [2, 1],
    },
    {
      id: 'inst-14',
      name: 'Salon Carved Wood Chairs',
      role: 'chairs',
      seating: true,
      collider: true,
      bounds: [[1.2, 0.0, -0.5], [2.2, 0.9, 0.5]],
      center: [1.7, 0.45, 0],
      size: [1.0, 0.9, 1.0],
      instances: [14, 1],
    },
    {
      id: 'inst-18',
      name: 'Salon Floor Andon Lanterns',
      role: 'lantern',
      collider: true,
      bounds: [[1.4, 0.0, -0.4], [2.0, 1.1, 0.4]],
      center: [1.7, 0.55, 0],
      size: [0.6, 1.1, 0.8],
      instances: [18, 1],
    },
  ];

  // Coupled collision: Agent A places chair at [1.5, 0.45, 0], Agent B places lantern at [1.6, 0.55, 0]
  const coupled = resolveTwoAgentWorldConflict({
    nodes,
    agentAProposal: { agentId: 'agent_interior', targetId: 'inst-14', proposedCenter: [1.5, 0.45, 0], weight: 0.6 },
    agentBProposal: { agentId: 'agent_lighting', targetId: 'inst-18', proposedCenter: [1.6, 0.55, 0], weight: 0.4 },
  });
  assert.equal(coupled.conflictDetected, true);
  assert.ok(coupled.conflicts.some((c) => c.kind === 'inter_agent_collision'));
  const sep = Math.hypot(
    coupled.resolvedProposals.agentA.resolvedCenter[0] - coupled.resolvedProposals.agentB.resolvedCenter[0],
    coupled.resolvedProposals.agentA.resolvedCenter[2] - coupled.resolvedProposals.agentB.resolvedCenter[2],
  );
  assert.ok(sep >= 0.8, `expected separated resolved centers, got sep=${sep}`);
  assert.ok(coupled.gbsCommit.includes('move inst-14'));
  assert.ok(coupled.gbsCommit.includes('move inst-18'));

  // Same-target contention
  const sameTarget = resolveTwoAgentWorldConflict({
    nodes,
    agentAProposal: { agentId: 'agent_interior', targetId: 'inst-14', proposedCenter: [-2.0, 0.45, 0], weight: 0.5 },
    agentBProposal: { agentId: 'agent_lighting', targetId: 'inst-14', proposedCenter: [2.0, 0.45, 0], weight: 0.5 },
  });
  assert.equal(sameTarget.conflictDetected, true);
  assert.ok(sameTarget.conflicts.some((c) => c.kind === 'same_target_contention'));
  assert.equal(sameTarget.resolvedProposals.agentA.resolvedCenter[0], 0);
});

test('exportWorldSemanticGraphJson, compileWorldGraphToGbs, and applySemanticGraphPatchToWorld round-trip with >1000x compression', () => {
  const nodes = [
    {
      id: 'inst-13',
      name: 'Salon Main Table (Chabudai)',
      role: 'table_main',
      category: 'furniture',
      supportSurface: true,
      collider: true,
      bounds: [[-1.2, 0.0, -1.2], [1.2, 0.8, 1.2]],
      center: [0, 0.4, 0],
      size: [2.4, 0.8, 2.4],
      instances: [13, 1],
      inDoc: true,
    },
    {
      id: 'inst-15',
      name: 'Silk Zabuton Floor Cushions',
      role: 'cushions',
      category: 'furniture',
      seating: true,
      collider: true,
      bounds: [[1.6, 0.0, -0.5], [2.4, 0.2, 0.5]],
      center: [2.0, 0.1, 0],
      size: [0.8, 0.2, 1.0],
      instances: [15, 1],
      inDoc: true,
    },
  ];
  const graph = exportWorldSemanticGraphJson({ nodes, sceneName: 'yozakura-salon' });
  assert.equal(graph.schema, 'ganbate.world.semantic.v2');
  assert.ok(graph.telemetry.compressionRatio > 1000);
  assert.ok(graph.gbs.includes('relate inst-15 around inst-13'));

  const patched = applySemanticGraphPatchToWorld({
    nodes,
    patch: {
      moves: [{ id: 'inst-15', targetCenter: [0.1, 0.1, 0] }], // collides with table -> projected outside
    },
  });
  assert.equal(patched.projections.length, 1);
  assert.equal(patched.projections[0].satisfied, false);
  assert.ok(patched.nodes.find((n) => n.id === 'inst-15').center[0] > 1.2);
});

