/**
 * VFX Studio workbench (`*.vfx.json`, schema `ganbate.vfx/1`).
 *
 * Authoring surface for deterministic 3D particle emitters & Yozakura table VFX:
 * - Yozakura Table VFX Document Switcher (`table-win-burst`, `all-in-chip-sparks`,
 *   `yozakura-sakura-petals`, `yozakura-lantern-embers`) + Live Table Broadcast Bridge
 * - Table Placement Anchor, Game Trigger Condition & Sequence Timing Inspector Banner
 * - 2.5D Yozakura Poker Table Context Viewport with Seat Anchor Targeter & 60fps Playback/Burst Trigger
 * - Full Emitter Parameter Inspector (Burst Count/Time, Spawn Rate, Shape, Offset XYZ, Speed,
 *   Gravity, Drag, Curl Turbulence, Lifetime, Size, Color Over Life)
 */

export const VFX_SCHEMA = 'ganbate.vfx/1';
const FIXED_DT = 1 / 60;
const LIVE_VFX_STORAGE_KEY = 'ganbate.studio.vfxOverrides.v1';
const LIVE_VFX_CHANNEL_NAME = 'ganbate-studio-vfx';
const LIVE_VFX_EVENT_NAME = 'ganbate:vfx-update';

export const VFX_EMITTER_PRESETS = Object.freeze({
  blossom_drift: () => ({
    id: 'blossom_drift',
    enabled: true,
    blendMode: 'alpha',
    maxParticles: 160,
    spawnRate: 18,
    bursts: [],
    lifetimeS: [4.5, 7.0],
    shape: { kind: 'box', halfExtents: [3.5, 0.4, 2.5] },
    offset: [0, 3.2, 0],
    direction: [0.25, -0.9, 0.15],
    speed: [0.25, 0.55],
    gravity: [0.08, -0.32, 0.03],
    drag: 0.85,
    curlNoise: { amplitude: 0.55, frequency: 0.65, scrollSpeed: 0.35 },
    initialRotationDeg: [-180, 180],
    angularVelocityDeg: [-95, 95],
    sizeBase: [0.028, 0.048],
    sizeOverLife: [
      { t: 0, value: 0.6 },
      { t: 0.2, value: 1.0 },
      { t: 0.85, value: 0.95 },
      { t: 1.0, value: 0.3 },
    ],
    colorOverLife: [
      { t: 0, rgba: [0.99, 0.82, 0.88, 0.0] },
      { t: 0.12, rgba: [0.99, 0.82, 0.88, 0.92] },
      { t: 0.85, rgba: [0.96, 0.72, 0.82, 0.82] },
      { t: 1.0, rgba: [0.92, 0.64, 0.76, 0.0] },
    ],
  }),
  ember_updraft: () => ({
    id: 'ember_updraft',
    enabled: true,
    blendMode: 'additive',
    maxParticles: 96,
    spawnRate: 22,
    bursts: [],
    lifetimeS: [1.6, 3.0],
    shape: { kind: 'disc', radius: 0.35 },
    offset: [0, 0, 0],
    direction: [0, 1, 0],
    speed: [0.45, 1.15],
    gravity: [0, 0.45, 0],
    drag: 0.4,
    curlNoise: { amplitude: 0.75, frequency: 1.35, scrollSpeed: 0.8 },
    initialRotationDeg: [0, 360],
    angularVelocityDeg: [-140, 140],
    sizeBase: [0.012, 0.026],
    sizeOverLife: [
      { t: 0, value: 0.5 },
      { t: 0.25, value: 1.0 },
      { t: 1.0, value: 0 },
    ],
    colorOverLife: [
      { t: 0, rgba: [1.0, 0.92, 0.58, 0.0] },
      { t: 0.1, rgba: [1.0, 0.78, 0.32, 0.95] },
      { t: 0.7, rgba: [0.96, 0.38, 0.12, 0.7] },
      { t: 1.0, rgba: [0.55, 0.12, 0.04, 0.0] },
    ],
  }),
  radial_burst: () => ({
    id: 'radial_burst',
    enabled: true,
    blendMode: 'additive',
    maxParticles: 120,
    spawnRate: 0,
    bursts: [{ timeS: 0, count: 72 }],
    lifetimeS: [0.65, 1.35],
    shape: { kind: 'cone', angleDeg: 58, radius: 0.08 },
    offset: [0, 0.05, 0],
    direction: [0, 1, 0],
    speed: [1.8, 3.8],
    gravity: [0, -4.8, 0],
    drag: 0.9,
    curlNoise: { amplitude: 0.3, frequency: 1.8, scrollSpeed: 0.6 },
    initialRotationDeg: [-180, 180],
    angularVelocityDeg: [-240, 240],
    sizeBase: [0.018, 0.038],
    sizeOverLife: [
      { t: 0, value: 1.0 },
      { t: 0.5, value: 0.8 },
      { t: 1.0, value: 0.0 },
    ],
    colorOverLife: [
      { t: 0, rgba: [1.0, 0.96, 0.78, 1.0] },
      { t: 0.45, rgba: [0.96, 0.76, 0.28, 0.9] },
      { t: 1.0, rgba: [0.85, 0.42, 0.14, 0.0] },
    ],
  }),
  mist_plume: () => ({
    id: 'mist_plume',
    enabled: true,
    blendMode: 'alpha',
    maxParticles: 64,
    spawnRate: 8,
    bursts: [],
    lifetimeS: [3.0, 5.0],
    shape: { kind: 'sphere', radius: 0.45 },
    offset: [0, 0.2, 0],
    direction: [0, 0.4, 0],
    speed: [0.08, 0.22],
    gravity: [0, 0.04, 0],
    drag: 1.2,
    curlNoise: { amplitude: 0.22, frequency: 0.45, scrollSpeed: 0.18 },
    initialRotationDeg: [-180, 180],
    angularVelocityDeg: [-18, 18],
    sizeBase: [0.22, 0.42],
    sizeOverLife: [
      { t: 0, value: 0.5 },
      { t: 0.5, value: 1.0 },
      { t: 1.0, value: 1.35 },
    ],
    colorOverLife: [
      { t: 0, rgba: [0.76, 0.82, 0.92, 0.0] },
      { t: 0.3, rgba: [0.76, 0.82, 0.92, 0.22] },
      { t: 1.0, rgba: [0.70, 0.76, 0.88, 0.0] },
    ],
  }),
});

export const YOZAKURA_TABLE_VFX_DOCS = Object.freeze({
  'table-win-burst': () => ({
    schema: VFX_SCHEMA,
    id: 'table-win-burst',
    name: 'Showdown Golden Ember & Blossom Fountain',
    seed: 77701,
    durationS: 2.2,
    looping: false,
    warmupS: 0.0,
    budgetMaxParticles: 180,
    emitters: [
      {
        id: 'golden_showdown_embers',
        enabled: true,
        blendMode: 'additive',
        maxParticles: 108,
        spawnRate: 0.0,
        bursts: [{ timeS: 0.0, count: 84 }],
        lifetimeS: [0.85, 1.65],
        shape: { kind: 'cone', angleDeg: 48.0, radius: 0.12 },
        offset: [0.0, 0.735, 0.0],
        direction: [0.0, 1.0, 0.0],
        speed: [1.35, 2.85],
        gravity: [0.0, -2.6, 0.0],
        drag: 0.72,
        curlNoise: { amplitude: 0.34, frequency: 1.6, scrollSpeed: 0.55 },
        initialRotationDeg: [-180.0, 180.0],
        angularVelocityDeg: [-210.0, 210.0],
        sizeBase: [0.014, 0.03],
        sizeOverLife: [
          { t: 0.0, value: 0.9 },
          { t: 0.25, value: 1.0 },
          { t: 1.0, value: 0.0 },
        ],
        colorOverLife: [
          { t: 0.0, rgba: [1.0, 0.96, 0.82, 1.0] },
          { t: 0.45, rgba: [1.0, 0.85, 0.55, 0.9] },
          { t: 1.0, rgba: [0.88, 0.48, 0.16, 0.0] },
        ],
      },
      {
        id: 'showdown_sakura_flurry',
        enabled: true,
        blendMode: 'alpha',
        maxParticles: 72,
        spawnRate: 0.0,
        bursts: [{ timeS: 0.04, count: 42 }],
        lifetimeS: [1.1, 2.05],
        shape: { kind: 'cone', angleDeg: 62.0, radius: 0.16 },
        offset: [0.0, 0.74, 0.0],
        direction: [0.0, 1.0, 0.0],
        speed: [0.95, 1.95],
        gravity: [0.04, -1.35, 0.02],
        drag: 1.05,
        curlNoise: { amplitude: 0.52, frequency: 1.15, scrollSpeed: 0.45 },
        initialRotationDeg: [-180.0, 180.0],
        angularVelocityDeg: [-180.0, 180.0],
        sizeBase: [0.02, 0.036],
        sizeOverLife: [
          { t: 0.0, value: 0.75 },
          { t: 0.3, value: 1.0 },
          { t: 1.0, value: 0.25 },
        ],
        colorOverLife: [
          { t: 0.0, rgba: [1.0, 0.88, 0.92, 0.95] },
          { t: 0.65, rgba: [0.98, 0.74, 0.84, 0.85] },
          { t: 1.0, rgba: [0.94, 0.55, 0.69, 0.0] },
        ],
      },
    ],
  }),
  'all-in-chip-sparks': () => ({
    schema: VFX_SCHEMA,
    id: 'all-in-chip-sparks',
    name: 'All-In Felt Impact Sparks',
    seed: 90911,
    durationS: 1.4,
    looping: false,
    warmupS: 0.0,
    budgetMaxParticles: 150,
    emitters: [
      {
        id: 'rim_gold_sparks',
        enabled: true,
        blendMode: 'additive',
        maxParticles: 110,
        spawnRate: 0.0,
        bursts: [{ timeS: 0.0, count: 90 }],
        lifetimeS: [0.35, 0.85],
        shape: { kind: 'cone', angleDeg: 78.0, radius: 0.09 },
        offset: [0.0, 0.734, 0.0],
        direction: [0.0, 1.0, 0.0],
        speed: [2.2, 4.4],
        gravity: [0.0, -6.2, 0.0],
        drag: 1.15,
        curlNoise: { amplitude: 0.24, frequency: 2.2, scrollSpeed: 0.7 },
        initialRotationDeg: [-180.0, 180.0],
        angularVelocityDeg: [-320.0, 320.0],
        sizeBase: [0.01, 0.024],
        sizeOverLife: [
          { t: 0.0, value: 1.0 },
          { t: 0.4, value: 0.75 },
          { t: 1.0, value: 0.0 },
        ],
        colorOverLife: [
          { t: 0.0, rgba: [1.0, 0.98, 0.86, 1.0] },
          { t: 0.35, rgba: [1.0, 0.78, 0.28, 0.95] },
          { t: 0.75, rgba: [0.92, 0.26, 0.18, 0.65] },
          { t: 1.0, rgba: [0.65, 0.12, 0.08, 0.0] },
        ],
      },
      {
        id: 'felt_shock_dust',
        enabled: true,
        blendMode: 'alpha',
        maxParticles: 40,
        spawnRate: 0.0,
        bursts: [{ timeS: 0.0, count: 28 }],
        lifetimeS: [0.55, 1.1],
        shape: { kind: 'disc', radius: 0.14 },
        offset: [0.0, 0.732, 0.0],
        direction: [0.0, 0.25, 0.0],
        speed: [0.35, 0.85],
        gravity: [0.0, 0.05, 0.0],
        drag: 1.8,
        curlNoise: { amplitude: 0.18, frequency: 1.1, scrollSpeed: 0.25 },
        initialRotationDeg: [-180.0, 180.0],
        angularVelocityDeg: [-45.0, 45.0],
        sizeBase: [0.045, 0.085],
        sizeOverLife: [
          { t: 0.0, value: 0.5 },
          { t: 0.5, value: 1.0 },
          { t: 1.0, value: 1.35 },
        ],
        colorOverLife: [
          { t: 0.0, rgba: [0.95, 0.86, 0.68, 0.28] },
          { t: 1.0, rgba: [0.85, 0.72, 0.56, 0.0] },
        ],
      },
    ],
  }),
  'yozakura-sakura-petals': () => ({
    schema: VFX_SCHEMA,
    id: 'yozakura-sakura-petals',
    name: 'Yozakura Night Sakura Canopy',
    seed: 20261102,
    durationS: 8.0,
    looping: true,
    warmupS: 2.5,
    budgetMaxParticles: 260,
    emitters: [
      {
        id: 'canopy_petal_fall',
        enabled: true,
        blendMode: 'alpha',
        maxParticles: 180,
        spawnRate: 20.0,
        bursts: [],
        lifetimeS: [5.2, 7.8],
        shape: { kind: 'box', halfExtents: [4.2, 0.45, 3.2] },
        offset: [0.0, 3.6, -0.4],
        direction: [0.22, -0.92, 0.18],
        speed: [0.22, 0.48],
        gravity: [0.06, -0.26, 0.04],
        drag: 0.92,
        curlNoise: { amplitude: 0.58, frequency: 0.55, scrollSpeed: 0.32 },
        initialRotationDeg: [-180.0, 180.0],
        angularVelocityDeg: [-105.0, 105.0],
        sizeBase: [0.024, 0.042],
        sizeOverLife: [
          { t: 0.0, value: 0.65 },
          { t: 0.15, value: 1.0 },
          { t: 0.88, value: 0.96 },
          { t: 1.0, value: 0.35 },
        ],
        colorOverLife: [
          { t: 0.0, rgba: [1.0, 0.9, 0.94, 0.0] },
          { t: 0.12, rgba: [1.0, 0.9, 0.94, 0.88] },
          { t: 0.72, rgba: [0.94, 0.55, 0.69, 0.82] },
          { t: 1.0, rgba: [0.94, 0.55, 0.69, 0.0] },
        ],
      },
      {
        id: 'dealer_breeze_gust',
        enabled: true,
        blendMode: 'alpha',
        maxParticles: 80,
        spawnRate: 6.0,
        bursts: [{ timeS: 1.2, count: 18 }],
        lifetimeS: [3.6, 5.4],
        shape: { kind: 'disc', radius: 0.95 },
        offset: [0.0, 1.85, -1.35],
        direction: [0.12, -0.35, 0.92],
        speed: [0.42, 0.78],
        gravity: [0.02, -0.22, 0.06],
        drag: 0.85,
        curlNoise: { amplitude: 0.72, frequency: 0.78, scrollSpeed: 0.48 },
        initialRotationDeg: [-180.0, 180.0],
        angularVelocityDeg: [-135.0, 135.0],
        sizeBase: [0.022, 0.036],
        sizeOverLife: [
          { t: 0.0, value: 0.5 },
          { t: 0.2, value: 1.0 },
          { t: 1.0, value: 0.2 },
        ],
        colorOverLife: [
          { t: 0.0, rgba: [1.0, 0.88, 0.92, 0.0] },
          { t: 0.15, rgba: [1.0, 0.88, 0.92, 0.85] },
          { t: 1.0, rgba: [0.94, 0.55, 0.69, 0.0] },
        ],
      },
    ],
  }),
  'yozakura-lantern-embers': () => ({
    schema: VFX_SCHEMA,
    id: 'yozakura-lantern-embers',
    name: 'Yozakura Lantern Warm Embers',
    seed: 10842,
    durationS: 6.0,
    looping: true,
    warmupS: 1.5,
    budgetMaxParticles: 160,
    emitters: [
      {
        id: 'lantern_core_updraft',
        enabled: true,
        blendMode: 'additive',
        maxParticles: 110,
        spawnRate: 18.0,
        bursts: [],
        lifetimeS: [2.2, 3.8],
        shape: { kind: 'sphere', radius: 0.28 },
        offset: [0.0, 1.45, 0.0],
        direction: [0.0, 1.0, 0.0],
        speed: [0.24, 0.68],
        gravity: [0.0, 0.32, 0.0],
        drag: 0.55,
        curlNoise: { amplitude: 0.64, frequency: 1.25, scrollSpeed: 0.62 },
        initialRotationDeg: [0.0, 360.0],
        angularVelocityDeg: [-90.0, 90.0],
        sizeBase: [0.01, 0.022],
        sizeOverLife: [
          { t: 0.0, value: 0.4 },
          { t: 0.25, value: 1.0 },
          { t: 1.0, value: 0.0 },
        ],
        colorOverLife: [
          { t: 0.0, rgba: [1.0, 0.94, 0.72, 0.0] },
          { t: 0.14, rgba: [1.0, 0.85, 0.55, 0.92] },
          { t: 0.68, rgba: [0.96, 0.46, 0.16, 0.68] },
          { t: 1.0, rgba: [0.58, 0.16, 0.05, 0.0] },
        ],
      },
      {
        id: 'lantern_halo_motes',
        enabled: true,
        blendMode: 'additive',
        maxParticles: 50,
        spawnRate: 6.0,
        bursts: [],
        lifetimeS: [3.0, 4.6],
        shape: { kind: 'box', halfExtents: [1.2, 0.8, 1.2] },
        offset: [0.0, 1.6, 0.0],
        direction: [0.0, 0.6, 0.0],
        speed: [0.06, 0.18],
        gravity: [0.0, 0.08, 0.0],
        drag: 0.95,
        curlNoise: { amplitude: 0.38, frequency: 0.85, scrollSpeed: 0.28 },
        initialRotationDeg: [0.0, 360.0],
        angularVelocityDeg: [-35.0, 35.0],
        sizeBase: [0.014, 0.028],
        sizeOverLife: [
          { t: 0.0, value: 0.2 },
          { t: 0.5, value: 1.0 },
          { t: 1.0, value: 0.0 },
        ],
        colorOverLife: [
          { t: 0.0, rgba: [1.0, 0.86, 0.52, 0.0] },
          { t: 0.4, rgba: [1.0, 0.82, 0.44, 0.55] },
          { t: 1.0, rgba: [0.92, 0.52, 0.2, 0.0] },
        ],
      },
    ],
  }),
});

export const VFX_ACADEMIC_PRESET_DOCS = Object.freeze({
  'yozakura-tea-steam': () => ({
    schema: VFX_SCHEMA,
    id: 'yozakura-tea-steam',
    name: 'Yozakura Ceramic Tea Steam Plume',
    seed: 40418,
    durationS: 5.0,
    looping: true,
    warmupS: 1.2,
    budgetMaxParticles: 120,
    emitters: [
      {
        id: 'ceramic_spout_steam',
        enabled: true,
        blendMode: 'alpha',
        maxParticles: 84,
        spawnRate: 16.0,
        bursts: [],
        lifetimeS: [2.4, 4.2],
        shape: { kind: 'disc', radius: 0.06 },
        offset: [0.15, 0.44, -0.05],
        direction: [0.0, 1.0, 0.0],
        speed: [0.14, 0.34],
        gravity: [0.02, 0.28, 0.01],
        drag: 0.95,
        curlNoise: { amplitude: 0.28, frequency: 0.95, scrollSpeed: 0.35 },
        initialRotationDeg: [-180.0, 180.0],
        angularVelocityDeg: [-28.0, 28.0],
        sizeBase: [0.022, 0.044],
        sizeOverLife: [
          { t: 0.0, value: 0.45 },
          { t: 0.35, value: 1.0 },
          { t: 1.0, value: 1.45 },
        ],
        colorOverLife: [
          { t: 0.0, rgba: [0.92, 0.95, 0.98, 0.0] },
          { t: 0.2, rgba: [0.92, 0.95, 0.98, 0.38] },
          { t: 0.75, rgba: [0.84, 0.89, 0.95, 0.22] },
          { t: 1.0, rgba: [0.78, 0.84, 0.92, 0.0] },
        ],
      },
      {
        id: 'warm_tea_aroma_motes',
        enabled: true,
        blendMode: 'additive',
        maxParticles: 36,
        spawnRate: 6.0,
        bursts: [],
        lifetimeS: [1.8, 3.2],
        shape: { kind: 'sphere', radius: 0.12 },
        offset: [0.15, 0.48, -0.05],
        direction: [0.0, 1.0, 0.0],
        speed: [0.08, 0.22],
        gravity: [0.0, 0.18, 0.0],
        drag: 0.85,
        curlNoise: { amplitude: 0.36, frequency: 1.15, scrollSpeed: 0.42 },
        initialRotationDeg: [0.0, 360.0],
        angularVelocityDeg: [-45.0, 45.0],
        sizeBase: [0.008, 0.016],
        sizeOverLife: [
          { t: 0.0, value: 0.3 },
          { t: 0.4, value: 1.0 },
          { t: 1.0, value: 0.0 },
        ],
        colorOverLife: [
          { t: 0.0, rgba: [0.98, 0.92, 0.76, 0.0] },
          { t: 0.3, rgba: [0.96, 0.86, 0.62, 0.55] },
          { t: 1.0, rgba: [0.85, 0.68, 0.42, 0.0] },
        ],
      },
    ],
  }),
  'ik-contact-glint': () => ({
    schema: VFX_SCHEMA,
    id: 'ik-contact-glint',
    name: 'Articulated End-Effector Surface Contact Glint',
    seed: 50529,
    durationS: 1.2,
    looping: false,
    warmupS: 0.0,
    budgetMaxParticles: 110,
    emitters: [
      {
        id: 'surface_contact_ring_pulse',
        enabled: true,
        blendMode: 'additive',
        maxParticles: 64,
        spawnRate: 0.0,
        bursts: [{ timeS: 0.0, count: 48 }],
        lifetimeS: [0.35, 0.75],
        shape: { kind: 'disc', radius: 0.11 },
        offset: [0.22, 0.735, -0.18],
        direction: [0.0, 0.35, 0.0],
        speed: [0.45, 1.15],
        gravity: [0.0, 0.05, 0.0],
        drag: 1.65,
        curlNoise: { amplitude: 0.16, frequency: 1.8, scrollSpeed: 0.45 },
        initialRotationDeg: [-180.0, 180.0],
        angularVelocityDeg: [-90.0, 90.0],
        sizeBase: [0.012, 0.024],
        sizeOverLife: [
          { t: 0.0, value: 0.9 },
          { t: 0.35, value: 1.0 },
          { t: 1.0, value: 0.0 },
        ],
        colorOverLife: [
          { t: 0.0, rgba: [0.44, 0.96, 0.78, 1.0] },
          { t: 0.45, rgba: [0.28, 0.85, 0.96, 0.85] },
          { t: 1.0, rgba: [0.16, 0.52, 0.88, 0.0] },
        ],
      },
      {
        id: 'ik_constraint_lock_sparks',
        enabled: true,
        blendMode: 'additive',
        maxParticles: 46,
        spawnRate: 0.0,
        bursts: [{ timeS: 0.02, count: 28 }],
        lifetimeS: [0.4, 0.85],
        shape: { kind: 'cone', angleDeg: 64.0, radius: 0.05 },
        offset: [0.22, 0.738, -0.18],
        direction: [0.0, 1.0, 0.0],
        speed: [1.1, 2.2],
        gravity: [0.0, -3.2, 0.0],
        drag: 1.1,
        curlNoise: { amplitude: 0.22, frequency: 1.6, scrollSpeed: 0.5 },
        initialRotationDeg: [-180.0, 180.0],
        angularVelocityDeg: [-180.0, 180.0],
        sizeBase: [0.009, 0.018],
        sizeOverLife: [
          { t: 0.0, value: 1.0 },
          { t: 1.0, value: 0.0 },
        ],
        colorOverLife: [
          { t: 0.0, rgba: [0.86, 1.0, 0.94, 1.0] },
          { t: 0.5, rgba: [0.38, 0.92, 0.74, 0.88] },
          { t: 1.0, rgba: [0.18, 0.62, 0.54, 0.0] },
        ],
      },
    ],
  }),
});

export const ALL_VFX_STUDIO_DOCS = Object.freeze({
  ...YOZAKURA_TABLE_VFX_DOCS,
  ...VFX_ACADEMIC_PRESET_DOCS,
});

export const VFX_ACADEMIC_SCENE_PRESETS = Object.freeze({
  yozakura_pavilion: {
    id: 'yozakura_pavilion',
    label: '🏯 Yozakura Pavilion (Academic Interior)',
    domain: 'Architectural Layout & Buoyant Plume Constraints',
    floorY: 0.0,
    defaultDocId: 'yozakura-tea-steam',
    sockets: [
      {
        id: 'tea_set',
        label: 'Ceramic Tea Set (Spout · y=0.44m)',
        nx: 0.52,
        ny: 0.62,
        world: [0.15, 0.44, -0.05],
        surfaceY: 0.44,
        tetherRadius: 0.38,
        defaultRelation: 'rises_from',
        solidBox: [-0.48, 0.0, -0.36, 0.48, 0.43, 0.36],
      },
      {
        id: 'stone_lantern',
        label: 'Garden Stone Lantern (Firebox · y=1.45m)',
        nx: 0.23,
        ny: 0.30,
        world: [-1.25, 1.45, -1.10],
        surfaceY: 1.42,
        tetherRadius: 0.48,
        defaultRelation: 'emits_from',
        solidBox: [-1.52, 0.0, -1.36, -0.98, 1.40, -0.84],
      },
      {
        id: 'sakura_bonsai',
        label: 'Sakura Canopy / Bonsai (y=2.60m)',
        nx: 0.76,
        ny: 0.24,
        world: [0.85, 2.60, -0.80],
        surfaceY: 0.42,
        tetherRadius: 1.85,
        defaultRelation: 'drifts_from',
      },
      {
        id: 'tatami_alcove',
        label: 'Tatami Floor Alcove (y=0.05m)',
        nx: 0.34,
        ny: 0.78,
        world: [-0.55, 0.05, 0.45],
        surfaceY: 0.05,
        tetherRadius: 0.65,
        defaultRelation: 'on_surface',
      },
    ],
  },
  character_ik_studio: {
    id: 'character_ik_studio',
    label: '🦾 Character IK & Contact Studio',
    domain: 'Articulated Rigging & Surface Contact Validation',
    floorY: 0.0,
    defaultDocId: 'ik-contact-glint',
    sockets: [
      {
        id: 'hand_target_R',
        label: 'Right Hand IK Target (Surface y=0.735m)',
        nx: 0.58,
        ny: 0.58,
        world: [0.22, 0.735, -0.18],
        surfaceY: 0.735,
        tetherRadius: 0.28,
        defaultRelation: 'contacts_at',
        solidBox: [-0.85, 0.0, -0.65, 0.85, 0.732, 0.65],
      },
      {
        id: 'hand_target_L',
        label: 'Left Hand IK Target (Surface y=0.735m)',
        nx: 0.42,
        ny: 0.58,
        world: [-0.24, 0.735, -0.16],
        surfaceY: 0.735,
        tetherRadius: 0.28,
        defaultRelation: 'contacts_at',
        solidBox: [-0.85, 0.0, -0.65, 0.85, 0.732, 0.65],
      },
      {
        id: 'rei_collar_anchor',
        label: 'Upper Torso / Collar Socket (y=1.18m)',
        nx: 0.50,
        ny: 0.30,
        world: [0.0, 1.18, -0.34],
        surfaceY: 1.12,
        tetherRadius: 0.32,
        defaultRelation: 'attached_to_bone',
      },
      {
        id: 'gaze_target',
        label: 'Gaze Focus Target (y=0.88m)',
        nx: 0.50,
        ny: 0.74,
        world: [0.0, 0.88, 0.24],
        surfaceY: 0.735,
        tetherRadius: 0.42,
        defaultRelation: 'focused_on',
      },
    ],
  },
  tabletop_salon: {
    id: 'tabletop_salon',
    label: '🎴 Ganbate Live Salon (Production Game)',
    domain: 'Multi-Agent Tabletop Sequencing & Live WebGPU Sync',
    floorY: 0.732,
    defaultDocId: 'table-win-burst',
    sockets: [
      {
        id: 'seat-0',
        label: 'Hero Seat 0 (하단 중앙 · 우승/올인 앵커)',
        nx: 0.5,
        ny: 0.81,
        world: [0.0, 0.735, 0.56],
        surfaceY: 0.735,
        tetherRadius: 0.45,
        defaultRelation: 'rises_from',
      },
      {
        id: 'seat-1',
        label: 'Seat 1 (좌측 하단 좌석)',
        nx: 0.21,
        ny: 0.73,
        world: [-1.15, 0.735, 0.36],
        surfaceY: 0.735,
        tetherRadius: 0.45,
        defaultRelation: 'rises_from',
      },
      {
        id: 'seat-2',
        label: 'Seat 2 (좌측 상단 도전자 좌석)',
        nx: 0.24,
        ny: 0.44,
        world: [-1.05, 0.734, -0.22],
        surfaceY: 0.734,
        tetherRadius: 0.45,
        defaultRelation: 'contacts_at',
      },
      {
        id: 'seat-3',
        label: 'Seat 3 (우측 상단 좌석)',
        nx: 0.76,
        ny: 0.44,
        world: [1.05, 0.735, -0.22],
        surfaceY: 0.735,
        tetherRadius: 0.45,
        defaultRelation: 'rises_from',
      },
      {
        id: 'seat-4',
        label: 'Seat 4 (우측 하단 좌석)',
        nx: 0.79,
        ny: 0.73,
        world: [1.15, 0.735, 0.36],
        surfaceY: 0.735,
        tetherRadius: 0.45,
        defaultRelation: 'rises_from',
      },
      {
        id: 'board-center',
        label: 'Board & Canopy (중앙 보드 / 벚꽃 캐노피)',
        nx: 0.5,
        ny: 0.58,
        world: [0.0, 0.74, 0.05],
        surfaceY: 0.74,
        tetherRadius: 1.6,
        defaultRelation: 'drifts_from',
      },
      {
        id: 'dealer-lanterns',
        label: 'Dealer Rei & Lanterns (딜러/정원 등불)',
        nx: 0.5,
        ny: 0.26,
        world: [0.0, 1.45, -1.35],
        surfaceY: 1.42,
        tetherRadius: 0.85,
        defaultRelation: 'emits_from',
      },
    ],
  },
});

export const YOZAKURA_VFX_TABLE_GUIDES = Object.freeze({
  'table-win-burst': {
    id: 'table-win-burst',
    path: 'content/vfx/table-win-burst.vfx.json',
    label: '🏆 쇼다운 팟 승리 분수 (table-win-burst)',
    scenePresetId: 'tabletop_salon',
    anchorId: 'winner_seat_anchor',
    defaultSeatAnchor: 'seat-0',
    semanticRelation: 'rises_from',
    semanticTarget: 'seat-0',
    placementText:
      '우승자 좌석 앞 펠트 스포트라이트 (pokerThreeTurnSpotlightPoint, y = 0.735m)',
    triggerText:
      "detectSeatMoments → kind: 'win' (winTier: 'showdown' | 'big' 경합/빅팟 승리 확정 시)",
    timingText:
      'poker-pot-award.seq.json @ 1320ms (딜러 Rei가 민 팟 칩이 우승 좌석에 도착하는 순간 · 2.20s 1회 폭발)',
    sequencePath: 'content/sequences/poker-pot-award.seq.json',
    runtimePath: 'ganbateParticles.ts → createGanbateWinBurst()',
  },
  'all-in-chip-sparks': {
    id: 'all-in-chip-sparks',
    path: 'content/vfx/all-in-chip-sparks.vfx.json',
    label: '⚡ 올인 칩 임팩트 스파크 (all-in-chip-sparks)',
    scenePresetId: 'tabletop_salon',
    anchorId: 'wager_stack_all_in',
    defaultSeatAnchor: 'seat-2',
    semanticRelation: 'contacts_at',
    semanticTarget: 'seat-2',
    placementText:
      '올인 선언 플레이어 좌석 베팅 스팟 (pokerThreeTurnSpotlightPoint, y = 0.734m)',
    triggerText:
      "detectSeatMoments → kind: 'all-in' | 're-all-in' (allInMoment.key 변경 순간 1회)",
    timingText:
      'poker-all-in-spotlight.seq.json @ 720ms (올인 칩 스택이 펠트에 착지하는 임팩트 순간 · 1.40s 1회 폭발)',
    sequencePath: 'content/sequences/poker-all-in-spotlight.seq.json',
    runtimePath: 'ganbateSeatMoments.ts → createGanbateSeatMoments()',
  },
  'yozakura-sakura-petals': {
    id: 'yozakura-sakura-petals',
    path: 'content/vfx/yozakura-sakura-petals.vfx.json',
    label: '🌸 요자쿠라 희귀 족보 벚꽃 낙화 (yozakura-sakura-petals)',
    scenePresetId: 'yozakura_pavilion',
    anchorId: 'rare_winner_seats + dealer_deck_gust',
    defaultSeatAnchor: 'board-center',
    semanticRelation: 'drifts_from',
    semanticTarget: 'sakura_bonsai',
    placementText:
      '희귀 족보 우승 좌석(canopy_petal_fall) + 딜러 Rei 덱 뒤편 돌풍(dealer_breeze_gust)',
    triggerText:
      "detectRareMoment → kind: 'petals' | 'lanterns' (포카드 이상 / 스트레이트 플러시 승리 시)",
    timingText:
      '칩 도착 직후 2800ms 동안 테이블 전역 낙화 (마지막 700ms 소프트 페이드아웃)',
    sequencePath: 'content/sequences/poker-pot-award.seq.json',
    runtimePath: 'ganbateParticles.ts → createGanbatePetals()',
  },
  'yozakura-lantern-embers': {
    id: 'yozakura-lantern-embers',
    path: 'content/vfx/yozakura-lantern-embers.vfx.json',
    label: '🏮 요자쿠라 정원 등불 불씨 (yozakura-lantern-embers)',
    scenePresetId: 'yozakura_pavilion',
    anchorId: 'yozakura-lanterns',
    defaultSeatAnchor: 'dealer-lanterns',
    semanticRelation: 'emits_from',
    semanticTarget: 'stone_lantern',
    placementText:
      '딜러 Rei 등 뒤 정원 석등/홍등 3D 좌표 (LANTERN_PAPERS, y = 1.45m..1.60m)',
    triggerText:
      "상시 앰비언트 루프 + detectRareMoment → kind: 'lanterns' (스트레이트 플러시 플레어)",
    timingText:
      '룸 진입 시 1.5s 예열 후 6.0s 주기 상시 루프 · 스트레이트 플러시 시 4.2s 부스트',
    sequencePath: 'content/sequences/poker-pot-award.seq.json',
    runtimePath: 'ganbateHallFlare.ts → createGanbateHallFlare()',
  },
  'yozakura-tea-steam': {
    id: 'yozakura-tea-steam',
    path: 'content/vfx/yozakura-tea-steam.vfx.json',
    label: '🍵 다기 세트 온열 수증기 플룸 (yozakura-tea-steam)',
    scenePresetId: 'yozakura_pavilion',
    anchorId: 'tea_set.spout_top',
    defaultSeatAnchor: 'tea_set',
    semanticRelation: 'rises_from',
    semanticTarget: 'tea_set',
    placementText:
      '요자쿠라 다실 낮은 느티나무 다탁 위 다기 주전자 주둥이 소켓 (pos = [0.15, 0.44, -0.05]m)',
    triggerText:
      'rises_from(vfx_tea_steam, tea_set) ∧ on_surface(tea_set, tea_table) 만족 시 상시 열부력 방출',
    timingText:
      '1.2s 열평형 예열 후 5.0s 주기 연속 부력 상승 · 다기 이동 시 역추론(L_vfx)으로 소켓 자동 추적',
    sequencePath: 'content/sequences/yozakura-pavilion-ambient.seq.json',
    runtimePath: 'vfx-studio.js → projectVfxConstraints() / simulateVfxAt()',
  },
  'ik-contact-glint': {
    id: 'ik-contact-glint',
    path: 'content/vfx/ik-contact-glint.vfx.json',
    label: '🦾 캐릭터 IK 표면 접점 검증 파동 (ik-contact-glint)',
    scenePresetId: 'character_ik_studio',
    anchorId: 'hand_target_R',
    defaultSeatAnchor: 'hand_target_R',
    semanticRelation: 'contacts_at',
    semanticTarget: 'hand_target_R',
    placementText:
      '관절 캐릭터 손끝 IK 엔드이펙터 표면 접점 (hand_target_R, y = 0.735m)',
    triggerText:
      'contacts_at(hand_target_R, table_surface) 제약 오차 ||p_wrist - p_surface||_2 < 1mm 도달 순간',
    timingText:
      'IK 수렴 즉시 0ms 링 펄스(48개) + 20ms 고정 스파크(28개) 1.20s 방출',
    sequencePath: 'content/sequences/character-ik-contact.seq.json',
    runtimePath: 'vfx-studio.js → projectVfxConstraints() / renderEmbeddedVfxInspector()',
  },
});

const TABLE_ANCHOR_SPOTS = Object.freeze([
  { id: 'seat-0', label: 'Hero Seat 0 (하단 중앙 · 우승/올인 앵커)', nx: 0.5, ny: 0.81, world: [0.0, 0.735, 0.56] },
  { id: 'seat-1', label: 'Seat 1 (좌측 하단 좌석)', nx: 0.21, ny: 0.73, world: [-1.15, 0.735, 0.36] },
  { id: 'seat-2', label: 'Seat 2 (좌측 상단 도전자 좌석)', nx: 0.24, ny: 0.44, world: [-1.05, 0.735, -0.22] },
  { id: 'seat-3', label: 'Seat 3 (우측 상단 좌석)', nx: 0.76, ny: 0.44, world: [1.05, 0.735, -0.22] },
  { id: 'seat-4', label: 'Seat 4 (우측 하단 좌석)', nx: 0.79, ny: 0.73, world: [1.15, 0.735, 0.36] },
  { id: 'board-center', label: 'Board & Canopy (중앙 보드 / 벚꽃 캐노피)', nx: 0.5, ny: 0.58, world: [0.0, 0.74, 0.05] },
  { id: 'dealer-lanterns', label: 'Dealer Rei & Lanterns (딜러/정원 등불)', nx: 0.5, ny: 0.26, world: [0.0, 1.45, -1.35] },
]);

/**
 * Inverse Semantic Lifting (L_vfx : R^3 -> G_rel).
 * Maps a continuous 3D emitter position to symbolic relational edges in the active scene graph.
 */
export function liftVfxSpatialToSemantic(
  position = [0, 0.735, 0],
  scenePresetOrNodes = 'yozakura_pavilion',
  vfxId = 'yozakura-tea-steam',
) {
  const pos = [
    Number(position?.[0] ?? 0),
    Number(position?.[1] ?? 0.44),
    Number(position?.[2] ?? 0),
  ];
  let sockets = [];
  let presetId = 'yozakura_pavilion';

  if (typeof scenePresetOrNodes === 'string') {
    presetId = VFX_ACADEMIC_SCENE_PRESETS[scenePresetOrNodes]
      ? scenePresetOrNodes
      : 'yozakura_pavilion';
    sockets = VFX_ACADEMIC_SCENE_PRESETS[presetId].sockets;
  } else if (Array.isArray(scenePresetOrNodes) && scenePresetOrNodes.length > 0) {
    sockets = scenePresetOrNodes.map((n, idx) => ({
      id: String(n.id ?? `node_${idx}`),
      label: String(n.label ?? n.id ?? `Node ${idx}`),
      world: Array.isArray(n.position)
        ? [Number(n.position[0] ?? 0), Number(n.position[1] ?? 0), Number(n.position[2] ?? 0)]
        : [0, 0.44, 0],
      surfaceY: Number(n.surfaceY ?? n.position?.[1] ?? 0.44),
      tetherRadius: Number(n.tetherRadius ?? 0.45),
      defaultRelation: String(n.defaultRelation ?? 'emits_from'),
      solidBox: Array.isArray(n.solidBox) ? n.solidBox : undefined,
    }));
  } else {
    sockets = VFX_ACADEMIC_SCENE_PRESETS.yozakura_pavilion.sockets;
  }

  let nearest = sockets[0];
  let minDistance = Infinity;
  for (const sock of sockets) {
    const d = Math.hypot(
      pos[0] - sock.world[0],
      pos[1] - sock.world[1],
      pos[2] - sock.world[2],
    );
    if (d < minDistance) {
      minDistance = d;
      nearest = sock;
    }
  }

  const dy = pos[1] - nearest.world[1];
  let relation = nearest.defaultRelation ?? 'emits_from';
  if (minDistance > (nearest.tetherRadius ?? 0.45) * 2.2) {
    relation = pos[1] > 1.8 ? 'drifts_from' : 'anchored_near';
  } else if (dy > 0.25 && relation === 'contacts_at') {
    relation = 'rises_from';
  }

  const cleanNodeId = String(vfxId || 'vfx_emitter').replace(/^.*\/|\.(vfx\.json|json)$/g, '');
  const edges = [
    {
      from: cleanNodeId,
      to: nearest.id,
      relation,
      distanceM: Number(minDistance.toFixed(3)),
    },
  ];
  const guide = YOZAKURA_VFX_TABLE_GUIDES[cleanNodeId];
  if (guide?.sequencePath) {
    edges.push({
      from: cleanNodeId,
      to: guide.sequencePath.replace(/^.*\//, ''),
      relation: 'triggered_by',
      distanceM: 0,
    });
  }

  return {
    scenePresetId: presetId,
    nearestSocket: nearest,
    distanceM: Number(minDistance.toFixed(3)),
    relation,
    edges,
    symbolicFormula: `${relation}(${cleanNodeId}, ${nearest.id}) [d=${minDistance.toFixed(2)}m]`,
  };
}

/**
 * Dual Spatial + GPU Frame-Budget Minimal-Perturbation Projection (\hat{\theta} -> \theta^*).
 * 1) Spatial Manifold Projection: \hat{x} -> x^* = argmin_{x \in C_spatial} ||x - \hat{x}||_2^2
 * 2) GPU Overdraw & Frame-Budget Projection: \hat{b} -> b^* = argmin_{b \in C_GPU} ||b - \hat{b}||_W^2
 */
export function projectVfxConstraints(nodeOrDoc, options = {}) {
  const isDoc =
    nodeOrDoc &&
    typeof nodeOrDoc === 'object' &&
    (nodeOrDoc.schema === VFX_SCHEMA || Array.isArray(nodeOrDoc.emitters));

  const vfxId = String(
    nodeOrDoc?.vfxId ??
      nodeOrDoc?.presetId ??
      nodeOrDoc?.id ??
      options.vfxId ??
      'yozakura-tea-steam',
  ).replace(/^.*\/|\.(vfx\.json|json)$/g, '');

  const guide = YOZAKURA_VFX_TABLE_GUIDES[vfxId];
  const scenePresetId =
    options.scenePresetId ??
    nodeOrDoc?.scenePresetId ??
    guide?.scenePresetId ??
    'yozakura_pavilion';
  const preset =
    VFX_ACADEMIC_SCENE_PRESETS[scenePresetId] ?? VFX_ACADEMIC_SCENE_PRESETS.yozakura_pavilion;

  const primaryEmitter = isDoc ? nodeOrDoc.emitters?.[0] : nodeOrDoc?.emitter ?? null;
  const rawPosSource =
    nodeOrDoc?.rawPosition ??
    nodeOrDoc?.position ??
    primaryEmitter?.offset ??
    preset.sockets[0].world;
  const rawPosition = [
    Number(rawPosSource[0] ?? 0),
    Number(rawPosSource[1] ?? preset.floorY),
    Number(rawPosSource[2] ?? 0),
  ];

  const lifted = liftVfxSpatialToSemantic(rawPosition, scenePresetId, vfxId);
  const sock = lifted.nearestSocket;
  const projectedPosition = [...rawPosition];
  const reasons = [];

  // Constraint 1: Floor elevation non-penetration (y >= floorY)
  if (projectedPosition[1] < preset.floorY) {
    projectedPosition[1] = preset.floorY;
    reasons.push(`floor_clearance(y >= ${preset.floorY.toFixed(3)}m)`);
  }

  // Constraint 2: Solid furniture / pedestal bounding box non-penetration
  if (Array.isArray(sock.solidBox)) {
    const [minX, minY, minZ, maxX, maxY, maxZ] = sock.solidBox;
    if (
      projectedPosition[0] >= minX &&
      projectedPosition[0] <= maxX &&
      projectedPosition[2] >= minZ &&
      projectedPosition[2] <= maxZ &&
      projectedPosition[1] >= minY &&
      projectedPosition[1] < sock.surfaceY
    ) {
      projectedPosition[1] = sock.surfaceY;
      reasons.push(`on_surface(${sock.id}, y=${sock.surfaceY.toFixed(3)}m)`);
    }
  }

  // Constraint 3: Host socket tether radius clamping for bound emitters
  const enforceTether = options.enforceSocketTether !== false;
  if (enforceTether && sock) {
    const dx = projectedPosition[0] - sock.world[0];
    const dy = projectedPosition[1] - sock.world[1];
    const dz = projectedPosition[2] - sock.world[2];
    const dist = Math.hypot(dx, dy, dz);
    const maxR = Number(options.maxTetherRadius ?? sock.tetherRadius ?? 0.5);
    if (dist > maxR && dist > 1e-6) {
      const scale = maxR / dist;
      projectedPosition[0] = Number((sock.world[0] + dx * scale).toFixed(4));
      projectedPosition[1] = Number(
        Math.max(preset.floorY, sock.world[1] + dy * scale).toFixed(4),
      );
      projectedPosition[2] = Number((sock.world[2] + dz * scale).toFixed(4));
      reasons.push(`socket_tether(${sock.id}, r<=${maxR.toFixed(2)}m)`);
    }
  }

  projectedPosition[0] = Number(projectedPosition[0].toFixed(4));
  projectedPosition[1] = Number(projectedPosition[1].toFixed(4));
  projectedPosition[2] = Number(projectedPosition[2].toFixed(4));

  const spatialDeltaL2 = Number(
    Math.hypot(
      projectedPosition[0] - rawPosition[0],
      projectedPosition[1] - rawPosition[1],
      projectedPosition[2] - rawPosition[2],
    ).toFixed(4),
  );

  // GPU Frame-Budget & Overdraw Manifold Projection (\hat{b} -> b^*)
  const rawSpawnRate = Math.max(
    0,
    Number(nodeOrDoc?.spawnRate ?? primaryEmitter?.spawnRate ?? 16),
  );
  const rawBurstCount = Math.max(
    0,
    Number(
      nodeOrDoc?.burstCount ??
        primaryEmitter?.bursts?.[0]?.count ??
        0,
    ),
  );
  const rawSizeMax = Math.max(
    0.004,
    Number(
      Array.isArray(nodeOrDoc?.sizeBase)
        ? nodeOrDoc.sizeBase[1]
        : (nodeOrDoc?.sizeBase ?? primaryEmitter?.sizeBase?.[1] ?? 0.036),
    ),
  );
  const rawMaxParticles = Math.max(
    8,
    Math.round(
      Number(
        nodeOrDoc?.maxParticles ??
          primaryEmitter?.maxParticles ??
          nodeOrDoc?.budgetMaxParticles ??
          120,
      ),
    ),
  );
  const blendMode =
    (nodeOrDoc?.blendMode ?? primaryEmitter?.blendMode) === 'additive'
      ? 'additive'
      : 'alpha';
  const lifeTuple = primaryEmitter?.lifetimeS ?? [1.2, 2.4];
  const avgLifeS = 0.5 * (Number(lifeTuple[0] ?? 1.2) + Number(lifeTuple[1] ?? 2.4));

  const maxParticlesBudget = Number(options.maxParticlesBudget ?? 180);
  const maxGpuMs = Number(options.maxGpuMs ?? 1.65);

  const computeBudgetMetrics = (spawnRate, burstCount, sizeMax, maxParticles) => {
    const activeEst = Math.min(maxParticles, spawnRate * avgLifeS + burstCount);
    const blendWeight = blendMode === 'additive' ? 1.35 : 1.0;
    const overdrawScore = activeEst * Math.pow(sizeMax * 18, 2) * blendWeight;
    const gpuMs = 0.08 + 0.0042 * activeEst + 0.055 * overdrawScore;
    return {
      activeEst: Number(activeEst.toFixed(1)),
      overdrawScore: Number(overdrawScore.toFixed(3)),
      gpuMs: Number(gpuMs.toFixed(3)),
    };
  };

  const rawMetrics = computeBudgetMetrics(
    rawSpawnRate,
    rawBurstCount,
    rawSizeMax,
    rawMaxParticles,
  );

  let projSpawnRate = rawSpawnRate;
  let projBurstCount = rawBurstCount;
  let projSizeMax = rawSizeMax;
  let projMaxParticles = Math.min(rawMaxParticles, maxParticlesBudget);
  let budgetClamped = false;

  if (rawMetrics.gpuMs > maxGpuMs || rawMaxParticles > maxParticlesBudget) {
    budgetClamped = true;
    if (projSizeMax > 0.048) {
      projSizeMax = Number(Math.min(projSizeMax, 0.044).toFixed(3));
    }
    const blendWeight = blendMode === 'additive' ? 1.35 : 1.0;
    const perParticleMs = 0.0042 + 0.055 * Math.pow(projSizeMax * 18, 2) * blendWeight;
    const maxAllowedActive = Math.max(
      8,
      Math.min(
        maxParticlesBudget,
        Math.floor(Math.max(0.05, maxGpuMs - 0.085) / Math.max(1e-5, perParticleMs)),
      ),
    );
    const rawUnboundedActive = projSpawnRate * avgLifeS + projBurstCount;
    if (rawUnboundedActive > maxAllowedActive && rawUnboundedActive > 1e-4) {
      const scale = (maxAllowedActive * 0.96) / rawUnboundedActive;
      projSpawnRate = Number((projSpawnRate * scale).toFixed(1));
      projBurstCount = Math.floor(projBurstCount * scale);
    }
    projMaxParticles = Math.min(projMaxParticles, maxAllowedActive);
    reasons.push(`gpu_frame_budget(T_gpu <= ${maxGpuMs.toFixed(2)}ms)`);
  }

  const projMetrics = computeBudgetMetrics(
    projSpawnRate,
    projBurstCount,
    projSizeMax,
    projMaxParticles,
  );

  const budgetDeltaL2 = Number(
    Math.hypot(
      (projSpawnRate - rawSpawnRate) / 60,
      (projBurstCount - rawBurstCount) / 80,
      (projSizeMax - rawSizeMax) / 0.04,
    ).toFixed(4),
  );

  const postLift = liftVfxSpatialToSemantic(projectedPosition, scenePresetId, vfxId);
  const spatialClamped = spatialDeltaL2 > 1e-4;
  const totalPerturbationL2 = Number(
    Math.hypot(spatialDeltaL2, budgetDeltaL2).toFixed(4),
  );

  const spatialProjection = {
    rawPosition,
    projectedPosition,
    deltaL2: spatialDeltaL2,
    clamped: spatialClamped,
    reasons: reasons.filter((r) => !r.startsWith('gpu_')),
  };

  const budgetProjection = {
    rawParams: {
      spawnRate: rawSpawnRate,
      burstCount: rawBurstCount,
      sizeMax: rawSizeMax,
      maxParticles: rawMaxParticles,
    },
    projectedParams: {
      spawnRate: projSpawnRate,
      burstCount: projBurstCount,
      sizeMax: projSizeMax,
      maxParticles: projMaxParticles,
    },
    rawGpuMs: rawMetrics.gpuMs,
    projectedGpuMs: projMetrics.gpuMs,
    budgetMaxGpuMs: maxGpuMs,
    rawOverdraw: rawMetrics.overdrawScore,
    projectedOverdraw: projMetrics.overdrawScore,
    deltaL2: budgetDeltaL2,
    clamped: budgetClamped,
  };

  let projectedDoc = null;
  if (isDoc) {
    projectedDoc = normalizeVfxDocument(nodeOrDoc);
    if (projectedDoc.emitters[0]) {
      const e0 = projectedDoc.emitters[0];
      e0.offset = [...projectedPosition];
      e0.spawnRate = projSpawnRate;
      e0.maxParticles = projMaxParticles;
      e0.sizeBase = [Number((projSizeMax * 0.55).toFixed(3)), projSizeMax];
      if (projBurstCount > 0 || e0.bursts.length > 0) {
        const t0 = e0.bursts[0]?.timeS ?? 0;
        e0.bursts = projBurstCount > 0 ? [{ timeS: t0, count: projBurstCount }] : [];
      }
    }
    projectedDoc.budgetMaxParticles = Math.min(
      projectedDoc.budgetMaxParticles,
      maxParticlesBudget,
    );
  }

  const projectedNode = !isDoc && nodeOrDoc && typeof nodeOrDoc === 'object'
    ? {
        ...nodeOrDoc,
        type: 'vfx_emitter',
        vfxId,
        scenePresetId,
        rawPosition,
        position: [...projectedPosition],
        spawnRate: projSpawnRate,
        burstCount: projBurstCount,
        sizeBase: projSizeMax,
        maxParticles: projMaxParticles,
        semanticEdges: postLift.edges,
        liftedFormula: postLift.symbolicFormula,
        constraintTelemetry: {
          spatialProjection,
          budgetProjection,
          totalPerturbationL2,
          isSatisfied: !spatialClamped && !budgetClamped,
        },
      }
    : null;

  if (!isDoc && nodeOrDoc && typeof nodeOrDoc === 'object') {
    nodeOrDoc.position = [...projectedPosition];
    nodeOrDoc.spawnRate = projSpawnRate;
    nodeOrDoc.burstCount = projBurstCount;
    nodeOrDoc.sizeBase = projSizeMax;
    nodeOrDoc.maxParticles = projMaxParticles;
    nodeOrDoc.semanticEdges = postLift.edges;
    nodeOrDoc.liftedFormula = postLift.symbolicFormula;
    nodeOrDoc.constraintTelemetry = projectedNode.constraintTelemetry;
  }

  return {
    node: projectedNode,
    document: projectedDoc,
    scenePresetId,
    vfxId,
    spatialProjection,
    budgetProjection,
    semanticEdges: postLift.edges,
    liftedRelation: postLift,
    isSatisfied: !spatialClamped && !budgetClamped,
    wasProjected: spatialClamped || budgetClamped,
    totalPerturbationL2,
    reasons,
  };
}

/**
 * Embedded Tri-Modal VFX Inspector widget for `editor/studio.js` (`#inspector-body`)
 * when a `type: "vfx_emitter"` node is selected in the unified Scene Graph.
 */
export function renderEmbeddedVfxInspector(container, node = {}, hooks = {}) {
  if (!container) return null;
  const dom = container.ownerDocument || globalThis.document;
  const stateNode = {
    id: node.id ?? 'vfx_tea_steam',
    type: 'vfx_emitter',
    vfxId: node.vfxId ?? node.presetId ?? 'yozakura-tea-steam',
    scenePresetId: node.scenePresetId ?? 'yozakura_pavilion',
    position: Array.isArray(node.position) ? [...node.position] : [0.15, 0.44, -0.05],
    rawPosition: Array.isArray(node.rawPosition) ? [...node.rawPosition] : undefined,
    spawnRate: Number(node.spawnRate ?? 16),
    burstCount: Number(node.burstCount ?? 0),
    sizeBase: Number(Array.isArray(node.sizeBase) ? node.sizeBase[1] : (node.sizeBase ?? 0.044)),
    maxParticles: Number(node.maxParticles ?? 84),
    blendMode: node.blendMode === 'additive' ? 'additive' : 'alpha',
  };

  const el = (tag, className, text) => {
    const elem = dom.createElement(tag);
    if (className) elem.className = className;
    if (text != null) elem.textContent = text;
    return elem;
  };

  const syncToCaller = (projectionResult) => {
    if (node && typeof node === 'object') {
      Object.assign(node, stateNode);
    }
    if (typeof hooks.onChange === 'function') {
      hooks.onChange(stateNode, projectionResult);
    }
    if (typeof hooks.onUpdateNode === 'function') {
      hooks.onUpdateNode(stateNode, projectionResult);
    }
  };

  const renderWidget = () => {
    container.innerHTML = '';
    const wrap = el('div', 'gb-vfx-embedded-inspector');
    wrap.dataset.role = 'embedded-vfx-inspector';

    const lift = liftVfxSpatialToSemantic(
      stateNode.rawPosition ?? stateNode.position,
      stateNode.scenePresetId,
      stateNode.vfxId,
    );
    const proj = projectVfxConstraints(
      { ...stateNode, rawPosition: stateNode.rawPosition ?? stateNode.position },
      { scenePresetId: stateNode.scenePresetId },
    );

    const header = el('div', 'gb-vfx-card gb-vfx-guide-card');
    header.appendChild(
      el('div', 'gb-vfx-title', `Tri-Modal VFX Node · ${stateNode.id}`),
    );

    const presetSelect = el('select', 'gb-vfx-select');
    presetSelect.dataset.role = 'embedded-vfx-preset-select';
    for (const [id, meta] of Object.entries(YOZAKURA_VFX_TABLE_GUIDES)) {
      const opt = el('option', '', meta.label);
      opt.value = id;
      if (id === stateNode.vfxId) opt.selected = true;
      presetSelect.appendChild(opt);
    }
    presetSelect.addEventListener('change', (ev) => {
      const nextId = ev.target.value;
      const factory = ALL_VFX_STUDIO_DOCS[nextId];
      const nextGuide = YOZAKURA_VFX_TABLE_GUIDES[nextId];
      stateNode.vfxId = nextId;
      if (nextGuide?.scenePresetId) stateNode.scenePresetId = nextGuide.scenePresetId;
      if (factory) {
        const doc = factory();
        const e0 = doc.emitters[0];
        stateNode.position = [...e0.offset];
        stateNode.rawPosition = undefined;
        stateNode.spawnRate = e0.spawnRate;
        stateNode.burstCount = e0.bursts?.[0]?.count ?? 0;
        stateNode.sizeBase = e0.sizeBase[1];
        stateNode.maxParticles = e0.maxParticles;
        stateNode.blendMode = e0.blendMode;
      }
      const nextProj = projectVfxConstraints(stateNode, {
        scenePresetId: stateNode.scenePresetId,
      });
      syncToCaller(nextProj);
      renderWidget();
    });
    header.appendChild(presetSelect);

    const relBadge = el(
      'div',
      'gb-vfx-guide-text',
      `L_vfx Relation: ${lift.symbolicFormula}`,
    );
    relBadge.dataset.role = 'embedded-vfx-relation';
    header.appendChild(relBadge);

    const statusText = proj.wasProjected
      ? `⚠️ Violation Detected: Spatial Δ=${proj.spatialProjection.deltaL2.toFixed(3)}m · GPU=${proj.budgetProjection.rawGpuMs.toFixed(2)}ms (Max ${proj.budgetProjection.budgetMaxGpuMs}ms)`
      : `✅ Manifold Satisfied: GPU=${proj.budgetProjection.projectedGpuMs.toFixed(2)}ms / ${proj.budgetProjection.budgetMaxGpuMs}ms · Spatial Δ=0.000m`;
    const statusBadge = el(
      'div',
      `gb-vfx-hud${proj.wasProjected ? ' is-over' : ''}`,
      statusText,
    );
    statusBadge.dataset.role = 'embedded-vfx-projection-status';
    header.appendChild(statusBadge);
    wrap.appendChild(header);

    const controls = el('div', 'gb-vfx-card');
    const addSlider = (label, role, min, max, step, val, onVal) => {
      const row = el('div', 'gb-vfx-row');
      row.appendChild(el('span', 'gb-vfx-row-label', label));
      const input = el('input');
      input.type = 'range';
      input.min = String(min);
      input.max = String(max);
      input.step = String(step);
      input.value = String(val);
      input.dataset.role = role;
      input.addEventListener('input', (ev) => {
        onVal(Number(ev.target.value));
      });
      row.appendChild(input);
      row.appendChild(el('span', 'gb-vfx-val', String(val)));
      controls.appendChild(row);
    };

    addSlider('Height Y (m)', 'embedded-vfx-pos-y', -0.4, 3.2, 0.02, stateNode.position[1], (v) => {
      stateNode.rawPosition = [stateNode.position[0], v, stateNode.position[2]];
      stateNode.position = [stateNode.position[0], v, stateNode.position[2]];
      syncToCaller(proj);
      renderWidget();
    });
    addSlider('Spawn Rate (/s)', 'embedded-vfx-spawn-rate', 0, 240, 2, stateNode.spawnRate, (v) => {
      stateNode.spawnRate = v;
      syncToCaller(proj);
      renderWidget();
    });
    addSlider('Burst Count', 'embedded-vfx-burst-count', 0, 260, 4, stateNode.burstCount, (v) => {
      stateNode.burstCount = v;
      syncToCaller(proj);
      renderWidget();
    });

    const btnRow = el('div', 'gb-vfx-inline-row');
    const hallucinateBtn = el(
      'button',
      'gb-vfx-btn',
      '⚡ Inject Hallucination (θ̂)',
    );
    hallucinateBtn.dataset.role = 'embedded-vfx-inject-hallucination';
    hallucinateBtn.addEventListener('click', () => {
      stateNode.rawPosition = [stateNode.position[0], -0.24, stateNode.position[2]];
      stateNode.position = [...stateNode.rawPosition];
      stateNode.spawnRate = 210;
      stateNode.burstCount = 190;
      stateNode.sizeBase = 0.095;
      syncToCaller(
        projectVfxConstraints(
          { ...stateNode },
          { scenePresetId: stateNode.scenePresetId },
        ),
      );
      renderWidget();
    });
    btnRow.appendChild(hallucinateBtn);

    const projectBtn = el(
      'button',
      'gb-vfx-btn is-active',
      '🛡️ Project to Manifold (θ̂ → θ*)',
    );
    projectBtn.dataset.role = 'embedded-vfx-project-btn';
    projectBtn.addEventListener('click', () => {
      const res = projectVfxConstraints(stateNode, {
        scenePresetId: stateNode.scenePresetId,
      });
      stateNode.rawPosition = undefined;
      syncToCaller(res);
      if (typeof hooks.onProject === 'function') hooks.onProject(res);
      renderWidget();
    });
    btnRow.appendChild(projectBtn);
    controls.appendChild(btnRow);
    wrap.appendChild(controls);
    container.appendChild(wrap);
  };

  renderWidget();
  return {
    refresh: renderWidget,
    getState: () => ({ ...stateNode }),
    project: () => projectVfxConstraints(stateNode, { scenePresetId: stateNode.scenePresetId }),
  };
}

export function createDefaultVfxDocument(id = 'ambient_canopy', name = 'Ambient Canopy') {
  if (ALL_VFX_STUDIO_DOCS[id]) {
    return ALL_VFX_STUDIO_DOCS[id]();
  }
  return {
    schema: VFX_SCHEMA,
    id,
    name,
    seed: 42,
    durationS: 5.0,
    looping: true,
    warmupS: 0.5,
    budgetMaxParticles: 320,
    emitters: [VFX_EMITTER_PRESETS.blossom_drift(), VFX_EMITTER_PRESETS.ember_updraft()],
  };
}

export function normalizeVfxDocument(raw) {
  const fallback = createDefaultVfxDocument();
  if (!raw || typeof raw !== 'object') return fallback;
  return {
    schema: VFX_SCHEMA,
    id: String(raw.id ?? fallback.id),
    name: String(raw.name ?? raw.id ?? fallback.name),
    seed: Math.max(0, Math.floor(Number(raw.seed ?? 1))),
    durationS: Math.max(0.1, Number(raw.durationS ?? 4.0)),
    looping: raw.looping !== false,
    warmupS: Math.max(0, Number(raw.warmupS ?? 0)),
    budgetMaxParticles: Math.max(1, Math.floor(Number(raw.budgetMaxParticles ?? 512))),
    emitters: Array.isArray(raw.emitters) && raw.emitters.length
      ? raw.emitters.map((e, idx) => ({
          id: String(e.id ?? `emitter_${idx}`),
          enabled: e.enabled !== false,
          blendMode: e.blendMode === 'additive' ? 'additive' : 'alpha',
          maxParticles: Math.max(1, Math.floor(Number(e.maxParticles ?? 128))),
          spawnRate: Math.max(0, Number(e.spawnRate ?? 0)),
          bursts: Array.isArray(e.bursts)
            ? e.bursts.map((b) => ({ timeS: Number(b.timeS ?? 0), count: Number(b.count ?? 0) }))
            : [],
          lifetimeS: Array.isArray(e.lifetimeS)
            ? [Number(e.lifetimeS[0] ?? 1), Number(e.lifetimeS[1] ?? 2)]
            : [1, 2],
          shape: e.shape ? { ...e.shape } : { kind: 'point' },
          offset: Array.isArray(e.offset)
            ? [Number(e.offset[0] ?? 0), Number(e.offset[1] ?? 0), Number(e.offset[2] ?? 0)]
            : [0, 0, 0],
          direction: Array.isArray(e.direction)
            ? [Number(e.direction[0] ?? 0), Number(e.direction[1] ?? 1), Number(e.direction[2] ?? 0)]
            : [0, 1, 0],
          speed: Array.isArray(e.speed)
            ? [Number(e.speed[0] ?? 0.5), Number(e.speed[1] ?? 1.0)]
            : [0.5, 1.0],
          gravity: Array.isArray(e.gravity)
            ? [Number(e.gravity[0] ?? 0), Number(e.gravity[1] ?? 0), Number(e.gravity[2] ?? 0)]
            : [0, 0, 0],
          drag: Math.max(0, Number(e.drag ?? 0)),
          curlNoise: {
            amplitude: Number(e.curlNoise?.amplitude ?? 0),
            frequency: Number(e.curlNoise?.frequency ?? 1),
            scrollSpeed: Number(e.curlNoise?.scrollSpeed ?? 0.4),
          },
          initialRotationDeg: Array.isArray(e.initialRotationDeg)
            ? [...e.initialRotationDeg]
            : [-180, 180],
          angularVelocityDeg: Array.isArray(e.angularVelocityDeg)
            ? [...e.angularVelocityDeg]
            : [0, 0],
          sizeBase: Array.isArray(e.sizeBase)
            ? [Number(e.sizeBase[0] ?? 0.03), Number(e.sizeBase[1] ?? 0.05)]
            : [0.03, 0.05],
          sizeOverLife: Array.isArray(e.sizeOverLife)
            ? e.sizeOverLife.map((p) => ({ ...p }))
            : [
                { t: 0, value: 1 },
                { t: 1, value: 0 },
              ],
          colorOverLife: Array.isArray(e.colorOverLife)
            ? e.colorOverLife.map((c) => ({
                t: Number(c.t ?? 0),
                rgba: [...(c.rgba ?? [1, 1, 1, 1])],
              }))
            : [
                { t: 0, rgba: [1, 1, 1, 1] },
                { t: 1, rgba: [1, 1, 1, 0] },
              ],
        }))
      : fallback.emitters,
  };
}

function makePrng(seed) {
  let state = (Number(seed) >>> 0) ^ 0x9e3779b9;
  const nextU32 = () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let t = Math.imul(state ^ (state >>> 15), 1 | state);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return (t ^ (t >>> 14)) >>> 0;
  };
  const nextF = () => nextU32() / 4294967296;
  return {
    nextF,
    range: (min, max) => min + (max - min) * nextF(),
  };
}

function curlField(pos, freq, timeS, scroll) {
  const f = Math.max(1e-4, freq);
  const t = timeS * scroll;
  const x = pos[0] * f + t * 0.37;
  const y = pos[1] * f + t * 0.73;
  const z = pos[2] * f - t * 0.29;
  return [
    Math.cos(y * 2.1 + z * 1.3) - Math.sin(z * 1.9 - x * 0.7),
    Math.cos(z * 2.3 + x * 1.1) - Math.sin(x * 1.7 - y * 0.9),
    Math.cos(x * 1.9 + y * 1.5) - Math.sin(y * 2.1 - z * 0.8),
  ];
}

function sampleScalar(points, t) {
  if (!points?.length) return 1;
  const u = Math.min(1, Math.max(0, t));
  if (u <= points[0].t) return points[0].value;
  for (let i = 0; i < points.length - 1; i++) {
    const a = points[i];
    const b = points[i + 1];
    if (u <= b.t) {
      const k = (u - a.t) / Math.max(1e-6, b.t - a.t);
      return a.value + (b.value - a.value) * k;
    }
  }
  return points[points.length - 1].value;
}

function sampleColor(stops, t) {
  if (!stops?.length) return [1, 1, 1, 1];
  const u = Math.min(1, Math.max(0, t));
  if (u <= stops[0].t) return [...stops[0].rgba];
  for (let i = 0; i < stops.length - 1; i++) {
    const a = stops[i];
    const b = stops[i + 1];
    if (u <= b.t) {
      const k = (u - a.t) / Math.max(1e-6, b.t - a.t);
      return [
        a.rgba[0] + (b.rgba[0] - a.rgba[0]) * k,
        a.rgba[1] + (b.rgba[1] - a.rgba[1]) * k,
        a.rgba[2] + (b.rgba[2] - a.rgba[2]) * k,
        a.rgba[3] + (b.rgba[3] - a.rgba[3]) * k,
      ];
    }
  }
  return [...stops[stops.length - 1].rgba];
}

function normalize3(v) {
  const len = Math.hypot(v[0], v[1], v[2]);
  return len > 1e-6 ? [v[0] / len, v[1] / len, v[2] / len] : [0, 1, 0];
}

export function simulateVfxAt(rawDoc, targetTimeS = 0) {
  const doc = normalizeVfxDocument(rawDoc);
  const rng = makePrng(doc.seed);
  const particles = [];
  const states = doc.emitters.map((e) => ({
    accum: 0,
    fired: new Array(e.bursts.length).fill(false),
  }));
  let simTime = 0;

  const spawnOne = (eIdx) => {
    if (particles.length >= doc.budgetMaxParticles) return;
    const spec = doc.emitters[eIdx];
    const ownCount = particles.filter((p) => p.emitterIndex === eIdx).length;
    if (ownCount >= spec.maxParticles) return;

    const dir = normalize3(spec.direction);
    let local = [0, 0, 0];
    let velDir = dir;
    if (spec.shape.kind === 'box') {
      const he = spec.shape.halfExtents ?? [0.5, 0.5, 0.5];
      local = [rng.range(-he[0], he[0]), rng.range(-he[1], he[1]), rng.range(-he[2], he[2])];
    } else if (spec.shape.kind === 'sphere' || spec.shape.kind === 'disc') {
      const r = Number(spec.shape.radius ?? 0.3) * Math.sqrt(rng.nextF());
      const th = rng.range(0, Math.PI * 2);
      local = [Math.cos(th) * r, spec.shape.kind === 'sphere' ? rng.range(-r, r) : 0, Math.sin(th) * r];
    } else if (spec.shape.kind === 'cone') {
      const spread = ((spec.shape.angleDeg ?? 35) * 0.5 * Math.PI) / 180;
      velDir = normalize3([
        dir[0] + rng.range(-spread, spread),
        dir[1] + rng.range(-spread * 0.5, spread * 0.5),
        dir[2] + rng.range(-spread, spread),
      ]);
    }
    const spd = rng.range(spec.speed[0], spec.speed[1]);
    const life = Math.max(0.01, rng.range(spec.lifetimeS[0], spec.lifetimeS[1]));
    const sizeBase = rng.range(spec.sizeBase[0], spec.sizeBase[1]);
    particles.push({
      emitterIndex: eIdx,
      blendMode: spec.blendMode,
      position: [spec.offset[0] + local[0], spec.offset[1] + local[1], spec.offset[2] + local[2]],
      velocity: [velDir[0] * spd, velDir[1] * spd, velDir[2] * spd],
      ageS: 0,
      lifetimeS: life,
      sizeBase,
      size: sizeBase * sampleScalar(spec.sizeOverLife, 0),
      rgba: sampleColor(spec.colorOverLife, 0),
    });
  };

  const fireBursts = (cycleT) => {
    doc.emitters.forEach((spec, eIdx) => {
      if (!spec.enabled) return;
      spec.bursts.forEach((b, bIdx) => {
        if (!states[eIdx].fired[bIdx] && cycleT >= b.timeS) {
          states[eIdx].fired[bIdx] = true;
          for (let i = 0; i < b.count; i++) spawnOne(eIdx);
        }
      });
    });
  };

  const step = (dt) => {
    const dur = Math.max(0.01, doc.durationS);
    const prevCycle = Math.floor(simTime / dur);
    simTime += dt;
    const nextCycle = Math.floor(simTime / dur);
    if (doc.looping && nextCycle > prevCycle) {
      for (const st of states) st.fired.fill(false);
    }
    const cycleT = doc.looping ? simTime % dur : simTime;
    if (doc.looping || simTime <= dur) {
      fireBursts(cycleT);
      doc.emitters.forEach((spec, eIdx) => {
        if (!spec.enabled || spec.spawnRate <= 0) return;
        states[eIdx].accum += spec.spawnRate * dt;
        const cnt = Math.floor(states[eIdx].accum);
        states[eIdx].accum -= cnt;
        for (let i = 0; i < cnt; i++) spawnOne(eIdx);
      });
    }
    for (let i = particles.length - 1; i >= 0; i--) {
      const p = particles[i];
      p.ageS += dt;
      if (p.ageS >= p.lifetimeS) {
        particles.splice(i, 1);
        continue;
      }
      const spec = doc.emitters[p.emitterIndex];
      const damp = Math.min(1, Math.max(0, 1 - spec.drag * dt));
      const curl = spec.curlNoise.amplitude
        ? curlField(p.position, spec.curlNoise.frequency, simTime, spec.curlNoise.scrollSpeed)
        : [0, 0, 0];
      p.velocity[0] = (p.velocity[0] + (spec.gravity[0] + curl[0] * spec.curlNoise.amplitude) * dt) * damp;
      p.velocity[1] = (p.velocity[1] + (spec.gravity[1] + curl[1] * spec.curlNoise.amplitude) * dt) * damp;
      p.velocity[2] = (p.velocity[2] + (spec.gravity[2] + curl[2] * spec.curlNoise.amplitude) * dt) * damp;
      p.position[0] += p.velocity[0] * dt;
      p.position[1] += p.velocity[1] * dt;
      p.position[2] += p.velocity[2] * dt;
      const u = p.ageS / p.lifetimeS;
      p.size = p.sizeBase * sampleScalar(spec.sizeOverLife, u);
      p.rgba = sampleColor(spec.colorOverLife, u);
    }
  };

  const advance = (total) => {
    let rem = Math.max(0, total);
    while (rem > FIXED_DT) {
      step(FIXED_DT);
      rem -= FIXED_DT;
    }
    if (rem > 1e-6) step(rem);
  };

  if (doc.warmupS > 0) {
    advance(doc.warmupS);
    simTime = 0;
    for (const st of states) st.fired.fill(false);
  }
  fireBursts(0);
  if (targetTimeS > 0) advance(targetTimeS);

  let additiveParticles = 0;
  let alphaParticles = 0;
  let overdraw = 0;
  for (const p of particles) {
    if (p.blendMode === 'additive') {
      additiveParticles++;
      overdraw += p.size * p.size * 1.35;
    } else {
      alphaParticles++;
      overdraw += p.size * p.size;
    }
  }

  return {
    particles,
    stats: {
      activeParticles: particles.length,
      additiveParticles,
      alphaParticles,
      estimatedOverdrawScore: Number(overdraw.toFixed(4)),
      withinBudget: particles.length <= doc.budgetMaxParticles,
    },
  };
}

export function buildAddPresetEmitterOps(doc, presetId) {
  const factory = VFX_EMITTER_PRESETS[presetId] ?? VFX_EMITTER_PRESETS.blossom_drift;
  const spec = factory();
  const nextIndex = doc.emitters?.length ?? 0;
  spec.id = `${presetId}_${nextIndex}`;
  return [{ set: ['emitters', String(nextIndex)], value: spec }];
}

export function buildEmitterFieldOps(emitterIndex, field, value) {
  return [{ set: ['emitters', String(emitterIndex), field], value }];
}

export function buildCurlNoiseOps(emitterIndex, patch) {
  return Object.entries(patch).map(([k, v]) => ({
    set: ['emitters', String(emitterIndex), 'curlNoise', k],
    value: Number(v),
  }));
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

function rgbaToHex(rgba) {
  const toHex = (c) =>
    Math.min(255, Math.max(0, Math.round(Number(c ?? 1) * 255)))
      .toString(16)
      .padStart(2, '0');
  return `#${toHex(rgba?.[0])}${toHex(rgba?.[1])}${toHex(rgba?.[2])}`;
}

function hexToRgb01(hex) {
  const clean = String(hex ?? '#ffffff').replace('#', '');
  if (clean.length !== 6) return [1, 1, 1];
  return [
    parseInt(clean.slice(0, 2), 16) / 255,
    parseInt(clean.slice(2, 4), 16) / 255,
    parseInt(clean.slice(4, 6), 16) / 255,
  ];
}

function broadcastLiveVfxUpdate(docId, docData) {
  const cleanId = String(docId ?? docData?.id ?? '').replace(/^.*\/|\.(vfx\.json|json)$/g, '');
  if (!ALL_VFX_STUDIO_DOCS[cleanId]) return;
  try {
    if (typeof globalThis.localStorage !== 'undefined') {
      const raw = globalThis.localStorage.getItem(LIVE_VFX_STORAGE_KEY);
      const map = raw ? JSON.parse(raw) : {};
      map[cleanId] = docData;
      globalThis.localStorage.setItem(LIVE_VFX_STORAGE_KEY, JSON.stringify(map));
    }
  } catch {
    // Ignore storage errors in sandboxed/test contexts.
  }
  try {
    if (typeof globalThis.CustomEvent === 'function' && typeof globalThis.dispatchEvent === 'function') {
      globalThis.dispatchEvent(
        new globalThis.CustomEvent(LIVE_VFX_EVENT_NAME, {
          detail: { id: cleanId, document: docData },
        }),
      );
    }
  } catch {
    // Ignore custom event errors.
  }
  try {
    if (typeof globalThis.BroadcastChannel === 'function') {
      const ch = new globalThis.BroadcastChannel(LIVE_VFX_CHANNEL_NAME);
      ch.postMessage({ id: cleanId, document: docData });
      ch.close();
    }
  } catch {
    // Ignore BroadcastChannel errors.
  }
  try {
    if (globalThis.parent && globalThis.parent !== globalThis) {
      globalThis.parent.postMessage(
        { type: 'studio:vfx-update', id: cleanId, document: docData },
        '*',
      );
    }
  } catch {
    // Ignore cross-frame postMessage errors.
  }
}

function loadSavedVfxOverride(docId) {
  const cleanId = String(docId ?? '').replace(/^.*\/|\.(vfx\.json|json)$/g, '');
  if (!ALL_VFX_STUDIO_DOCS[cleanId]) return null;
  try {
    if (typeof globalThis.localStorage !== 'undefined') {
      const raw = globalThis.localStorage.getItem(LIVE_VFX_STORAGE_KEY);
      if (raw) {
        const parsed = JSON.parse(raw);
        if (parsed && typeof parsed === 'object' && parsed[cleanId]) {
          return normalizeVfxDocument(parsed[cleanId]);
        }
      }
    }
  } catch {
    // Ignore storage errors.
  }
  return null;
}

export function mountVfxStudio(container, studioApi = {}) {
  const dom = container.ownerDocument || globalThis.document;
  let activeDocId = 'content/vfx/table-win-burst.vfx.json';
  let docData =
    loadSavedVfxOverride('table-win-burst') ?? YOZAKURA_TABLE_VFX_DOCS['table-win-burst']();
  let activeScenePresetId = 'tabletop_salon';
  let hallucinationRawPos = null;
  let selectedEmitter = 0;
  let scrubTimeS = 0.45;
  let isPlaying = false;
  let loopPreview = true;
  let showTableContext = true;
  let previewSeatId = 'seat-0';
  let rafId = null;
  let lastTickMs = 0;
  let active = true;
  let destroyed = false;
  let liveCanvas = null;
  let liveScrubInput = null;
  let liveScrubVal = null;
  let liveHud = null;
  const unsubscribers = [];

  const syncFromStudio = () => {
    const activeDoc = studioApi.getActiveDocument?.();
    if (
      activeDoc &&
      (String(activeDoc.path ?? activeDoc.id ?? '').endsWith('.vfx.json') ||
        activeDoc.root?.schema === VFX_SCHEMA)
    ) {
      activeDocId = activeDoc.id ?? activeDoc.path;
      const incoming = normalizeVfxDocument(activeDoc.root ?? activeDoc.data ?? activeDoc);
      const saved = loadSavedVfxOverride(incoming.id);
      docData = saved ?? incoming;
      const guide = YOZAKURA_VFX_TABLE_GUIDES[docData.id];
      if (guide) {
        previewSeatId = guide.defaultSeatAnchor;
        if (guide.scenePresetId) activeScenePresetId = guide.scenePresetId;
      }
      if (!docData.looping && scrubTimeS > docData.durationS) {
        scrubTimeS = Math.min(0.45, docData.durationS * 0.35);
      }
      if (selectedEmitter >= docData.emitters.length) selectedEmitter = 0;
    }
  };

  const stopPlaybackLoop = () => {
    isPlaying = false;
    if (rafId != null && typeof globalThis.cancelAnimationFrame === 'function') {
      globalThis.cancelAnimationFrame(rafId);
    }
    rafId = null;
  };

  const stepPlaybackFrame = (nowMs) => {
    if (!isPlaying || destroyed || !active) {
      rafId = null;
      return;
    }
    const dt = lastTickMs > 0 ? Math.min(0.05, Math.max(0.005, (nowMs - lastTickMs) / 1000)) : FIXED_DT;
    lastTickMs = nowMs;
    const dur = Math.max(0.1, docData.durationS);
    scrubTimeS += dt;
    if (scrubTimeS >= dur) {
      if (loopPreview || docData.looping) {
        scrubTimeS = scrubTimeS % dur;
      } else {
        scrubTimeS = dur;
        stopPlaybackLoop();
        render();
        return;
      }
    }
    if (liveCanvas) {
      const sim = simulateVfxAt(docData, scrubTimeS);
      paintPreviewCanvas(liveCanvas, sim);
      if (liveScrubInput) liveScrubInput.value = String(scrubTimeS);
      if (liveScrubVal) liveScrubVal.textContent = `${scrubTimeS.toFixed(2)}s`;
      if (liveHud) {
        liveHud.className = `gb-vfx-hud${sim.stats.withinBudget ? '' : ' is-over'}`;
        liveHud.textContent = `${sim.stats.activeParticles}/${docData.budgetMaxParticles} pts · OD ${sim.stats.estimatedOverdrawScore.toFixed(2)}`;
      }
    }
    if (typeof globalThis.requestAnimationFrame === 'function') {
      rafId = globalThis.requestAnimationFrame(stepPlaybackFrame);
    }
  };

  const startPlaybackLoop = (resetToStart = false) => {
    if (resetToStart || scrubTimeS >= docData.durationS - 0.02) {
      scrubTimeS = docData.looping ? 0.05 : 0.04;
    }
    isPlaying = true;
    lastTickMs = 0;
    if (rafId == null && typeof globalThis.requestAnimationFrame === 'function') {
      rafId = globalThis.requestAnimationFrame(stepPlaybackFrame);
    }
  };

  const commitOps = (ops, label, merge = false) => {
    if (!ops.length) return;
    for (const op of ops) applyOpLocally(docData, op);
    docData = normalizeVfxDocument(docData);
    if (activeDocId && typeof studioApi.edit === 'function') {
      studioApi.edit(activeDocId, ops, label, merge);
    }
    broadcastLiveVfxUpdate(activeDocId ?? docData.id, docData);
    render();
  };

  const switchVfxDocument = (vfxId) => {
    const factory = ALL_VFX_STUDIO_DOCS[vfxId];
    if (!factory) return;
    activeDocId = `content/vfx/${vfxId}.vfx.json`;
    docData = loadSavedVfxOverride(vfxId) ?? normalizeVfxDocument(factory());
    hallucinationRawPos = null;
    selectedEmitter = 0;
    const guide = YOZAKURA_VFX_TABLE_GUIDES[vfxId];
    if (guide) {
      previewSeatId = guide.defaultSeatAnchor;
      if (guide.scenePresetId) activeScenePresetId = guide.scenePresetId;
    }
    scrubTimeS = docData.looping ? 1.2 : 0.36;
    if (typeof studioApi.openDocument === 'function') {
      studioApi.openDocument(activeDocId);
    }
    render();
  };

  const root = dom.createElement('div');
  root.className = 'gb-vfx-studio';
  container.innerHTML = '';
  container.appendChild(root);

  const el = (tag, className, text) => {
    const node = dom.createElement(tag);
    if (className) node.className = className;
    if (text != null) node.textContent = text;
    return node;
  };

  const paintPreviewCanvas = (canvas, sim) => {
    const ctx = canvas.getContext?.('2d');
    if (!ctx) return;
    const w = (canvas.width = 480);
    const h = (canvas.height = 268);
    ctx.clearRect(0, 0, w, h);

    const anchorSpot =
      TABLE_ANCHOR_SPOTS.find((s) => s.id === previewSeatId) ?? TABLE_ANCHOR_SPOTS[0];
    const originX = showTableContext ? w * anchorSpot.nx : w * 0.5;
    const originY = showTableContext ? h * anchorSpot.ny : h * 0.72;
    const selectedEm = docData.emitters[selectedEmitter] ?? docData.emitters[0];
    const baseOffsetY = selectedEm?.offset?.[1] ?? 0;

    if (showTableContext && typeof ctx.stroke === 'function') {
      ctx.save?.();
      // 1. Dealer Rei & Rear Garden Lanterns
      ctx.fillStyle = 'rgba(232, 180, 74, 0.14)';
      for (const lx of [w * 0.24, w * 0.38, w * 0.62, w * 0.76]) {
        ctx.beginPath();
        ctx.arc(lx, h * 0.16, 6, 0, Math.PI * 2);
        ctx.fill();
      }
      ctx.fillStyle = 'rgba(232, 180, 74, 0.78)';
      ctx.font = '600 10px ui-sans-serif, system-ui, sans-serif';
      ctx.fillText?.('🏮 YOZAKURA LANTERNS  ·  DEALER REI (z = -0.34m)', w * 0.26, h * 0.13);

      // 2. Yozakura Table Outer Rail & Felt Oval
      ctx.strokeStyle = 'rgba(218, 169, 78, 0.42)';
      ctx.lineWidth = 2;
      ctx.beginPath();
      if (typeof ctx.ellipse === 'function') {
        ctx.ellipse(w * 0.5, h * 0.6, w * 0.37, h * 0.28, 0, 0, Math.PI * 2);
      } else {
        ctx.arc(w * 0.5, h * 0.6, h * 0.28, 0, Math.PI * 2);
      }
      ctx.stroke?.();

      ctx.fillStyle = 'rgba(20, 34, 30, 0.55)';
      ctx.beginPath();
      if (typeof ctx.ellipse === 'function') {
        ctx.ellipse(w * 0.5, h * 0.6, w * 0.34, h * 0.25, 0, 0, Math.PI * 2);
      } else {
        ctx.arc(w * 0.5, h * 0.6, h * 0.25, 0, Math.PI * 2);
      }
      ctx.fill();

      // 3. 5 Community Card Slots on Center Felt
      ctx.strokeStyle = 'rgba(232, 180, 74, 0.28)';
      ctx.lineWidth = 1;
      for (let c = 0; c < 5; c++) {
        const cx = w * 0.5 + (c - 2) * 22 - 8;
        const cy = h * 0.54;
        ctx.strokeRect?.(cx, cy, 16, 22);
      }

      // 4. Seat Anchors around the table
      for (const spot of TABLE_ANCHOR_SPOTS) {
        if (spot.id === 'board-center' || spot.id === 'dealer-lanterns') continue;
        const sx = w * spot.nx;
        const sy = h * spot.ny;
        const isTarget = spot.id === anchorSpot.id;
        ctx.fillStyle = isTarget ? 'rgba(232, 180, 74, 0.32)' : 'rgba(255, 255, 255, 0.08)';
        ctx.beginPath();
        ctx.arc(sx, sy, isTarget ? 13 : 8, 0, Math.PI * 2);
        ctx.fill();
        ctx.strokeStyle = isTarget ? 'rgba(232, 180, 74, 0.95)' : 'rgba(255, 255, 255, 0.22)';
        ctx.lineWidth = isTarget ? 1.8 : 1;
        ctx.stroke?.();
        ctx.fillStyle = isTarget ? '#f6d27a' : 'rgba(217, 220, 227, 0.55)';
        ctx.font = '600 9px ui-monospace, monospace';
        ctx.fillText?.(spot.id.toUpperCase(), sx - 16, sy + 22);
      }

      // 5. Active Spawn Anchor Crosshair & Offset Label
      ctx.strokeStyle = 'rgba(245, 200, 96, 0.85)';
      ctx.lineWidth = 1.2;
      ctx.beginPath();
      ctx.moveTo?.(originX - 16, originY);
      ctx.lineTo?.(originX + 16, originY);
      ctx.moveTo?.(originX, originY - 16);
      ctx.lineTo?.(originX, originY + 16);
      ctx.stroke?.();
      ctx.restore?.();
    }

    // 6. Render Deterministic 3D Particles Projected from Active Table Anchor
    for (const p of sim.particles) {
      const depth = Math.max(0.6, 3.2 - p.position[2] * 0.35);
      const relY = showTableContext ? p.position[1] - baseOffsetY * 0.85 : p.position[1];
      const sx = originX + (p.position[0] / depth) * 150;
      const sy = originY - (relY / depth) * 150;
      const rad = Math.max(1.5, (p.size / depth) * 220);
      const [r, g, b, a] = p.rgba;
      ctx.fillStyle = `rgba(${Math.round(r * 255)}, ${Math.round(g * 255)}, ${Math.round(b * 255)}, ${a.toFixed(3)})`;
      ctx.beginPath();
      ctx.arc(sx, sy, rad, 0, Math.PI * 2);
      ctx.fill();
    }
  };

  const makeSliderRow = (label, role, min, max, step, value, unit, onInput) => {
    const row = el('div', 'gb-vfx-row');
    row.appendChild(el('span', 'gb-vfx-row-label', label));
    const input = el('input');
    input.type = 'range';
    input.min = String(min);
    input.max = String(max);
    input.step = String(step);
    input.value = String(value);
    if (role) input.dataset.role = role;
    input.addEventListener('input', (ev) => {
      onInput(Number(ev.target.value));
    });
    row.appendChild(input);
    const formatted =
      Number(step) < 0.01
        ? Number(value).toFixed(3)
        : Number(step) < 1
          ? Number(value).toFixed(2)
          : String(Math.round(Number(value)));
    row.appendChild(el('span', 'gb-vfx-val', `${formatted}${unit}`));
    return row;
  };

  const render = () => {
    if (destroyed) return;
    root.innerHTML = '';
    const sim = simulateVfxAt(docData, scrubTimeS);
    const guide = YOZAKURA_VFX_TABLE_GUIDES[docData.id] ?? null;

    // Top Toolbar: Title + Yozakura VFX Effect Switcher + Budget HUD + Reset + Save
    const toolbar = el('div', 'gb-vfx-toolbar');
    toolbar.appendChild(el('span', 'gb-vfx-title', 'VFX Lab'));

    const docSelect = el('select', 'gb-vfx-select gb-vfx-doc-select');
    docSelect.dataset.role = 'vfx-doc-select';
    for (const [id, meta] of Object.entries(YOZAKURA_VFX_TABLE_GUIDES)) {
      const opt = el('option', '', meta.label);
      opt.value = id;
      if (docData.id === id) opt.selected = true;
      docSelect.appendChild(opt);
    }
    if (!YOZAKURA_VFX_TABLE_GUIDES[docData.id]) {
      const customOpt = el('option', '', `${docData.name} (${docData.id})`);
      customOpt.value = docData.id;
      customOpt.selected = true;
      docSelect.appendChild(customOpt);
    }
    docSelect.addEventListener('change', (ev) => {
      switchVfxDocument(ev.target.value);
    });
    toolbar.appendChild(docSelect);

    const syncBadge = el('span', 'gb-vfx-sync-badge', '● Live Table Sync');
    syncBadge.dataset.role = 'vfx-live-sync-badge';
    toolbar.appendChild(syncBadge);

    toolbar.appendChild(el('div', 'gb-vfx-spacer'));

    const hud = el(
      'span',
      `gb-vfx-hud${sim.stats.withinBudget ? '' : ' is-over'}`,
      `${sim.stats.activeParticles}/${docData.budgetMaxParticles} pts · OD ${sim.stats.estimatedOverdrawScore.toFixed(2)}`,
    );
    hud.dataset.role = 'vfx-budget-hud';
    liveHud = hud;
    toolbar.appendChild(hud);

    if (ALL_VFX_STUDIO_DOCS[docData.id]) {
      const resetBtn = el('button', 'gb-vfx-btn', '초기화 (Reset)');
      resetBtn.dataset.role = 'vfx-reset-doc';
      resetBtn.addEventListener('click', () => {
        docData = normalizeVfxDocument(ALL_VFX_STUDIO_DOCS[docData.id]());
        hallucinationRawPos = null;
        broadcastLiveVfxUpdate(docData.id, docData);
        render();
      });
      toolbar.appendChild(resetBtn);
    }

    const saveBtn = el('button', 'gb-vfx-btn is-active', 'Save & Apply');
    saveBtn.dataset.role = 'vfx-save';
    saveBtn.addEventListener('click', () => {
      broadcastLiveVfxUpdate(activeDocId ?? docData.id, docData);
      if (activeDocId) studioApi.save?.(activeDocId);
    });
    toolbar.appendChild(saveBtn);
    root.appendChild(toolbar);

    const grid = el('div', 'gb-vfx-grid');

    // Left col: Table & Academic VFX Quick List + Emitter Stack + Add Emitter Preset
    const leftCol = el('div', 'gb-vfx-col');

    const tableVfxCard = el('div', 'gb-vfx-card');
    tableVfxCard.appendChild(
      el('div', 'gb-vfx-title', 'Tri-Modal VFX Presets (6)'),
    );
    for (const [id, meta] of Object.entries(YOZAKURA_VFX_TABLE_GUIDES)) {
      const btn = el(
        'button',
        `gb-vfx-btn gb-vfx-doc-btn${docData.id === id ? ' is-active' : ''}`,
        meta.label,
      );
      btn.dataset.vfxDoc = id;
      btn.addEventListener('click', () => switchVfxDocument(id));
      tableVfxCard.appendChild(btn);
    }
    leftCol.appendChild(tableVfxCard);

    const stackCard = el('div', 'gb-vfx-card');
    stackCard.appendChild(el('div', 'gb-vfx-title', `Emitters (${docData.emitters.length})`));
    docData.emitters.forEach((em, idx) => {
      const burstCount = em.bursts?.[0]?.count ?? 0;
      const modeTag = burstCount > 0 ? `burst:${burstCount}` : `${em.spawnRate}/s`;
      const btn = el(
        'button',
        `gb-vfx-btn${idx === selectedEmitter ? ' is-active' : ''}`,
        `${em.enabled ? '●' : '○'} ${em.id} [${em.blendMode} · ${modeTag}]`,
      );
      btn.dataset.emitterIndex = String(idx);
      btn.addEventListener('click', () => {
        selectedEmitter = idx;
        render();
      });
      stackCard.appendChild(btn);
    });
    leftCol.appendChild(stackCard);

    const presetCard = el('div', 'gb-vfx-card');
    presetCard.appendChild(el('div', 'gb-vfx-title', 'Add Emitter Preset'));
    for (const presetId of Object.keys(VFX_EMITTER_PRESETS)) {
      const btn = el('button', 'gb-vfx-btn', `+ ${presetId}`);
      btn.dataset.preset = presetId;
      btn.addEventListener('click', () => {
        commitOps(buildAddPresetEmitterOps(docData, presetId), `Add emitter ${presetId}`);
      });
      presetCard.appendChild(btn);
    }
    leftCol.appendChild(presetCard);
    grid.appendChild(leftCol);

    // Center col: Table Placement/Trigger Guide Banner + Tri-Modal Constraint Projection + 2.5D Preview
    const centerCol = el('div', 'gb-vfx-col');

    if (guide) {
      const guideCard = el('div', 'gb-vfx-card gb-vfx-guide-card');
      guideCard.dataset.role = 'vfx-table-guide';
      guideCard.appendChild(
        el(
          'div',
          'gb-vfx-title',
          `배치 앵커 · 트리거 · 시퀀스 재생 시점 (${guide.path})`,
        ),
      );

      const metaList = el('div', 'gb-vfx-guide-list');
      const row1 = el('div', 'gb-vfx-guide-item');
      row1.appendChild(el('span', 'gb-vfx-guide-tag', '① 테이블 배치 (Anchor)'));
      row1.appendChild(
        el('span', 'gb-vfx-guide-text', `[${guide.anchorId}] ${guide.placementText}`),
      );
      metaList.appendChild(row1);

      const row2 = el('div', 'gb-vfx-guide-item');
      row2.appendChild(el('span', 'gb-vfx-guide-tag', '② 트리거 조건 (When)'));
      row2.appendChild(el('span', 'gb-vfx-guide-text', guide.triggerText));
      metaList.appendChild(row2);

      const row3 = el('div', 'gb-vfx-guide-item');
      row3.appendChild(el('span', 'gb-vfx-guide-tag', '③ 재생 시점 (Sequence)'));
      row3.appendChild(
        el('span', 'gb-vfx-guide-text', `${guide.timingText} · 런타임: ${guide.runtimePath}`),
      );
      metaList.appendChild(row3);
      guideCard.appendChild(metaList);
      centerCol.appendChild(guideCard);
    }

    // Tri-Modal Core Engine Bar (Inverse Semantic Lifting L_vfx + Dual Projection \hat{\theta} -> \theta^*)
    const activeEm = docData.emitters[selectedEmitter] ?? docData.emitters[0];
    const currentPos = hallucinationRawPos ?? activeEm?.offset ?? [0, 0.735, 0];
    const liftRes = liftVfxSpatialToSemantic(currentPos, activeScenePresetId, docData.id);
    const projRes = projectVfxConstraints(
      {
        id: docData.id,
        vfxId: docData.id,
        scenePresetId: activeScenePresetId,
        rawPosition: currentPos,
        position: activeEm?.offset ?? [0, 0.735, 0],
        spawnRate: activeEm?.spawnRate ?? 0,
        burstCount: activeEm?.bursts?.[0]?.count ?? 0,
        sizeBase: activeEm?.sizeBase ?? [0.02, 0.036],
        maxParticles: activeEm?.maxParticles ?? 108,
        blendMode: activeEm?.blendMode ?? 'alpha',
        emitter: activeEm,
      },
      { scenePresetId: activeScenePresetId },
    );

    const triCard = el('div', 'gb-vfx-card');
    triCard.dataset.role = 'vfx-trimodal-card';
    const triHead = el('div', 'gb-vfx-preview-header');
    triHead.appendChild(
      el('span', 'gb-vfx-title', 'Tri-Modal Core Engine (L_vfx + Dual Projection)'),
    );
    const sceneSelect = el('select', 'gb-vfx-select');
    sceneSelect.dataset.role = 'vfx-scene-preset-select';
    for (const [presetKey, presetMeta] of Object.entries(VFX_ACADEMIC_SCENE_PRESETS)) {
      const opt = el('option', '', presetMeta.label);
      opt.value = presetKey;
      if (presetKey === activeScenePresetId) opt.selected = true;
      sceneSelect.appendChild(opt);
    }
    sceneSelect.addEventListener('change', (ev) => {
      activeScenePresetId = ev.target.value;
      const presetMeta = VFX_ACADEMIC_SCENE_PRESETS[activeScenePresetId];
      if (presetMeta?.defaultDocId) {
        switchVfxDocument(presetMeta.defaultDocId);
      } else {
        render();
      }
    });
    triHead.appendChild(sceneSelect);
    triCard.appendChild(triHead);

    const formulaRow = el(
      'div',
      'gb-vfx-guide-text',
      `🔗 Inverse Semantic Lifting (L_vfx): ${liftRes.symbolicFormula}`,
    );
    formulaRow.dataset.role = 'vfx-lifted-formula';
    triCard.appendChild(formulaRow);

    const projSummary = el(
      'div',
      `gb-vfx-hud${projRes.wasProjected ? ' is-over' : ''}`,
      projRes.wasProjected
        ? `⚠️ Violation (θ̂): Spatial Δ=${projRes.spatialProjection.deltaL2.toFixed(3)}m · GPU=${projRes.budgetProjection.rawGpuMs.toFixed(2)}ms > ${projRes.budgetProjection.budgetMaxGpuMs}ms`
        : `✅ Constraint Manifold (θ*): Spatial Δ=0.000m · GPU=${projRes.budgetProjection.projectedGpuMs.toFixed(2)}ms / ${projRes.budgetProjection.budgetMaxGpuMs}ms`,
    );
    projSummary.dataset.role = 'vfx-projection-telemetry';
    triCard.appendChild(projSummary);

    const triActions = el('div', 'gb-vfx-inline-row');
    const injectHallucinationBtn = el(
      'button',
      'gb-vfx-btn',
      '⚡ Simulate Hallucination / Over-Budget (θ̂)',
    );
    injectHallucinationBtn.dataset.role = 'vfx-inject-hallucination';
    injectHallucinationBtn.addEventListener('click', () => {
      if (!activeEm) return;
      hallucinationRawPos = [activeEm.offset[0], -0.22, activeEm.offset[2]];
      commitOps(
        [
          ...buildEmitterFieldOps(selectedEmitter, 'offset', [...hallucinationRawPos]),
          ...buildEmitterFieldOps(selectedEmitter, 'spawnRate', 195),
          ...buildEmitterFieldOps(selectedEmitter, 'maxParticles', 280),
        ],
        'Inject hallucinated over-budget VFX state',
      );
    });
    triActions.appendChild(injectHallucinationBtn);

    const projectManifoldBtn = el(
      'button',
      'gb-vfx-btn is-active',
      '🛡️ Project to Manifold (θ̂ → θ*)',
    );
    projectManifoldBtn.dataset.role = 'vfx-project-manifold';
    projectManifoldBtn.addEventListener('click', () => {
      if (!activeEm) return;
      const projected = projectVfxConstraints(docData, {
        scenePresetId: activeScenePresetId,
      });
      hallucinationRawPos = null;
      if (projected.document) {
        const e0 = projected.document.emitters[0];
        commitOps(
          [
            ...buildEmitterFieldOps(0, 'offset', e0.offset),
            ...buildEmitterFieldOps(0, 'spawnRate', e0.spawnRate),
            ...buildEmitterFieldOps(0, 'maxParticles', e0.maxParticles),
            ...buildEmitterFieldOps(0, 'sizeBase', e0.sizeBase),
            ...buildEmitterFieldOps(0, 'bursts', e0.bursts),
          ],
          'Minimal-perturbation VFX manifold projection',
        );
      }
    });
    triActions.appendChild(projectManifoldBtn);
    triCard.appendChild(triActions);
    centerCol.appendChild(triCard);

    const previewCard = el('div', 'gb-vfx-card');
    const previewHeader = el('div', 'gb-vfx-preview-header');
    previewHeader.appendChild(el('span', 'gb-vfx-title', '3D Table Anchor Preview'));

    const anchorSelect = el('select', 'gb-vfx-select');
    anchorSelect.dataset.role = 'vfx-anchor-select';
    for (const spot of TABLE_ANCHOR_SPOTS) {
      const opt = el('option', '', spot.label);
      opt.value = spot.id;
      if (spot.id === previewSeatId) opt.selected = true;
      anchorSelect.appendChild(opt);
    }
    anchorSelect.addEventListener('change', (ev) => {
      previewSeatId = ev.target.value;
      render();
    });
    previewHeader.appendChild(anchorSelect);

    const overlayBtn = el(
      'button',
      `gb-vfx-btn${showTableContext ? ' is-active' : ''}`,
      showTableContext ? 'Table Map: ON' : 'Table Map: OFF',
    );
    overlayBtn.dataset.role = 'vfx-table-overlay-toggle';
    overlayBtn.addEventListener('click', () => {
      showTableContext = !showTableContext;
      render();
    });
    previewHeader.appendChild(overlayBtn);
    previewCard.appendChild(previewHeader);

    const canvas = el('canvas', 'gb-vfx-canvas');
    canvas.dataset.role = 'vfx-canvas';
    liveCanvas = canvas;
    paintPreviewCanvas(canvas, sim);
    previewCard.appendChild(canvas);

    // Playback Transport Controls Row (Play/Pause, Trigger Burst Now, Loop)
    const transportRow = el('div', 'gb-vfx-transport');
    const playBtn = el(
      'button',
      `gb-vfx-btn${isPlaying ? ' is-active' : ''}`,
      isPlaying ? '⏸ Pause' : '▶ Play (60fps)',
    );
    playBtn.dataset.role = 'vfx-play-toggle';
    playBtn.addEventListener('click', () => {
      if (isPlaying) {
        stopPlaybackLoop();
        render();
      } else {
        startPlaybackLoop(false);
        render();
      }
    });
    transportRow.appendChild(playBtn);

    const triggerBtn = el('button', 'gb-vfx-btn', '⚡ Trigger Burst (0.04s)');
    triggerBtn.dataset.role = 'vfx-trigger-burst';
    triggerBtn.addEventListener('click', () => {
      scrubTimeS = 0.04;
      startPlaybackLoop(true);
      render();
    });
    transportRow.appendChild(triggerBtn);

    const loopBtn = el(
      'button',
      `gb-vfx-btn${loopPreview ? ' is-active' : ''}`,
      loopPreview ? '🔁 Loop: ON' : '🔁 Loop: OFF',
    );
    loopBtn.dataset.role = 'vfx-loop-toggle';
    loopBtn.addEventListener('click', () => {
      loopPreview = !loopPreview;
      render();
    });
    transportRow.appendChild(loopBtn);
    previewCard.appendChild(transportRow);

    const scrubRow = el('div', 'gb-vfx-row');
    scrubRow.appendChild(el('span', 'gb-vfx-row-label', 'Scrub Time'));
    const scrub = el('input');
    scrub.type = 'range';
    scrub.min = '0';
    scrub.max = String(docData.durationS);
    scrub.step = '0.02';
    scrub.value = String(scrubTimeS);
    scrub.dataset.role = 'vfx-scrubber';
    liveScrubInput = scrub;
    scrub.addEventListener('input', (ev) => {
      stopPlaybackLoop();
      scrubTimeS = Number(ev.target.value);
      render();
    });
    scrubRow.appendChild(scrub);
    const scrubVal = el('span', 'gb-vfx-val', `${scrubTimeS.toFixed(2)}s`);
    liveScrubVal = scrubVal;
    scrubRow.appendChild(scrubVal);
    previewCard.appendChild(scrubRow);
    centerCol.appendChild(previewCard);
    grid.appendChild(centerCol);

    // Right col: Comprehensive Emitter Inspector
    const rightCol = el('div', 'gb-vfx-col');
    const em = docData.emitters[selectedEmitter];
    if (em) {
      const inspCard = el('div', 'gb-vfx-card');
      inspCard.appendChild(el('div', 'gb-vfx-title', `Inspector: ${em.id}`));

      const topToggleRow = el('div', 'gb-vfx-inline-row');
      const enableBtn = el(
        'button',
        `gb-vfx-btn${em.enabled ? ' is-active' : ''}`,
        em.enabled ? 'Enabled (활성)' : 'Disabled (비활성)',
      );
      enableBtn.dataset.role = 'emitter-enable';
      enableBtn.addEventListener('click', () => {
        commitOps(buildEmitterFieldOps(selectedEmitter, 'enabled', !em.enabled), `Toggle ${em.id}`);
      });
      topToggleRow.appendChild(enableBtn);

      const blendSelect = el('select', 'gb-vfx-select');
      blendSelect.dataset.role = 'emitter-blend';
      for (const mode of ['alpha', 'additive']) {
        const opt = el('option', '', `Blend: ${mode}`);
        opt.value = mode;
        if (mode === em.blendMode) opt.selected = true;
        blendSelect.appendChild(opt);
      }
      blendSelect.addEventListener('change', (ev) => {
        commitOps(
          buildEmitterFieldOps(selectedEmitter, 'blendMode', ev.target.value),
          `Blend mode ${em.id}`,
        );
      });
      topToggleRow.appendChild(blendSelect);
      inspCard.appendChild(topToggleRow);

      // Section 1: Emission (Burst Count, Burst Time, Spawn Rate, Max Particles)
      inspCard.appendChild(el('div', 'gb-vfx-subtitle', '1. 방출량 · 폭발 타이밍 (Emission)'));
      const burstCount = em.bursts?.[0]?.count ?? 0;
      const burstTimeS = em.bursts?.[0]?.timeS ?? 0;
      inspCard.appendChild(
        makeSliderRow(
          'Burst Count',
          'emitter-burst-count',
          0,
          240,
          1,
          burstCount,
          '개',
          (val) => {
            const nextBursts =
              val > 0 ? [{ timeS: burstTimeS, count: Math.round(val) }] : [];
            commitOps(
              buildEmitterFieldOps(selectedEmitter, 'bursts', nextBursts),
              `Burst count ${em.id}`,
              true,
            );
          },
        ),
      );
      inspCard.appendChild(
        makeSliderRow(
          'Burst Delay',
          'emitter-burst-time',
          0,
          Math.max(1.5, docData.durationS),
          0.02,
          burstTimeS,
          's',
          (val) => {
            const nextBursts = [{ timeS: val, count: Math.max(1, burstCount || 48) }];
            commitOps(
              buildEmitterFieldOps(selectedEmitter, 'bursts', nextBursts),
              `Burst delay ${em.id}`,
              true,
            );
          },
        ),
      );
      inspCard.appendChild(
        makeSliderRow(
          'Spawn Rate',
          'emitter-spawn-rate',
          0,
          120,
          1,
          em.spawnRate,
          '/s',
          (val) => {
            commitOps(
              buildEmitterFieldOps(selectedEmitter, 'spawnRate', val),
              `Spawn rate ${em.id}`,
              true,
            );
          },
        ),
      );
      inspCard.appendChild(
        makeSliderRow(
          'Max Particles',
          'emitter-max-particles',
          8,
          320,
          4,
          em.maxParticles,
          '',
          (val) => {
            commitOps(
              buildEmitterFieldOps(selectedEmitter, 'maxParticles', Math.round(val)),
              `Max particles ${em.id}`,
              true,
            );
          },
        ),
      );

      // Section 2: Table Offset & Emitter Shape
      inspCard.appendChild(el('div', 'gb-vfx-subtitle', '2. 테이블 배치 오프셋 · 형태 (Placement)'));
      const shapeRow = el('div', 'gb-vfx-row');
      shapeRow.appendChild(el('span', 'gb-vfx-row-label', 'Shape Kind'));
      const shapeSelect = el('select', 'gb-vfx-select');
      shapeSelect.dataset.role = 'emitter-shape-kind';
      for (const kind of ['cone', 'box', 'disc', 'sphere', 'point']) {
        const opt = el('option', '', kind);
        opt.value = kind;
        if (em.shape?.kind === kind) opt.selected = true;
        shapeSelect.appendChild(opt);
      }
      shapeSelect.addEventListener('change', (ev) => {
        const nextKind = ev.target.value;
        const nextShape = { ...em.shape, kind: nextKind };
        if (nextKind === 'cone' && nextShape.angleDeg == null) nextShape.angleDeg = 52;
        if ((nextKind === 'disc' || nextKind === 'sphere') && nextShape.radius == null)
          nextShape.radius = 0.25;
        if (nextKind === 'box' && !Array.isArray(nextShape.halfExtents))
          nextShape.halfExtents = [1.0, 0.3, 1.0];
        commitOps(buildEmitterFieldOps(selectedEmitter, 'shape', nextShape), `Shape ${em.id}`);
      });
      shapeRow.appendChild(shapeSelect);
      shapeRow.appendChild(el('span', 'gb-vfx-val', em.shape?.kind ?? 'point'));
      inspCard.appendChild(shapeRow);

      if (em.shape?.kind === 'cone') {
        inspCard.appendChild(
          makeSliderRow(
            'Cone Angle',
            'emitter-shape-angle',
            5,
            160,
            1,
            em.shape.angleDeg ?? 48,
            '°',
            (val) => {
              commitOps(
                buildEmitterFieldOps(selectedEmitter, 'shape', { ...em.shape, angleDeg: val }),
                `Cone angle ${em.id}`,
                true,
              );
            },
          ),
        );
      }
      inspCard.appendChild(
        makeSliderRow(
          'Offset X (좌우)',
          'emitter-offset-x',
          -2.5,
          2.5,
          0.02,
          em.offset[0] ?? 0,
          'm',
          (val) => {
            commitOps(
              buildEmitterFieldOps(selectedEmitter, 'offset', [val, em.offset[1], em.offset[2]]),
              `Offset X ${em.id}`,
              true,
            );
          },
        ),
      );
      inspCard.appendChild(
        makeSliderRow(
          'Offset Y (높이)',
          'emitter-offset-y',
          0.0,
          4.0,
          0.02,
          em.offset[1] ?? 0.735,
          'm',
          (val) => {
            commitOps(
              buildEmitterFieldOps(selectedEmitter, 'offset', [em.offset[0], val, em.offset[2]]),
              `Offset Y ${em.id}`,
              true,
            );
          },
        ),
      );
      inspCard.appendChild(
        makeSliderRow(
          'Offset Z (전후)',
          'emitter-offset-z',
          -2.5,
          2.5,
          0.02,
          em.offset[2] ?? 0,
          'm',
          (val) => {
            commitOps(
              buildEmitterFieldOps(selectedEmitter, 'offset', [em.offset[0], em.offset[1], val]),
              `Offset Z ${em.id}`,
              true,
            );
          },
        ),
      );

      // Section 3: Physics & Motion (Speed, Gravity, Drag, Curl Amp)
      inspCard.appendChild(el('div', 'gb-vfx-subtitle', '3. 물리 · 속도 · 중력 · 난류 (Physics)'));
      inspCard.appendChild(
        makeSliderRow(
          'Speed Min',
          'emitter-speed-min',
          0.05,
          6.0,
          0.05,
          em.speed[0] ?? 0.5,
          'm/s',
          (val) => {
            const hi = Math.max(val, em.speed[1] ?? val);
            commitOps(
              buildEmitterFieldOps(selectedEmitter, 'speed', [val, hi]),
              `Speed min ${em.id}`,
              true,
            );
          },
        ),
      );
      inspCard.appendChild(
        makeSliderRow(
          'Speed Max',
          'emitter-speed-max',
          0.05,
          8.0,
          0.05,
          em.speed[1] ?? 1.5,
          'm/s',
          (val) => {
            const lo = Math.min(val, em.speed[0] ?? val);
            commitOps(
              buildEmitterFieldOps(selectedEmitter, 'speed', [lo, val]),
              `Speed max ${em.id}`,
              true,
            );
          },
        ),
      );
      inspCard.appendChild(
        makeSliderRow(
          'Gravity Y',
          'emitter-gravity-y',
          -9.8,
          3.0,
          0.1,
          em.gravity[1] ?? -1.0,
          'm/s²',
          (val) => {
            commitOps(
              buildEmitterFieldOps(selectedEmitter, 'gravity', [em.gravity[0], val, em.gravity[2]]),
              `Gravity Y ${em.id}`,
              true,
            );
          },
        ),
      );
      inspCard.appendChild(
        makeSliderRow(
          'Air Drag',
          'emitter-drag',
          0.0,
          4.0,
          0.05,
          em.drag ?? 0.8,
          '',
          (val) => {
            commitOps(buildEmitterFieldOps(selectedEmitter, 'drag', val), `Drag ${em.id}`, true);
          },
        ),
      );
      inspCard.appendChild(
        makeSliderRow(
          'Curl Amp',
          'emitter-curl-amp',
          0,
          3,
          0.05,
          em.curlNoise.amplitude,
          '',
          (val) => {
            commitOps(
              buildCurlNoiseOps(selectedEmitter, { amplitude: val }),
              `Curl noise ${em.id}`,
              true,
            );
          },
        ),
      );

      // Section 4: Size, Lifetime & Color Over Life
      inspCard.appendChild(el('div', 'gb-vfx-subtitle', '4. 입자 크기 · 수명 · 색상 (Appearance)'));
      inspCard.appendChild(
        makeSliderRow(
          'Life Max',
          'emitter-life-max',
          0.2,
          10.0,
          0.1,
          em.lifetimeS[1] ?? 1.5,
          's',
          (val) => {
            const lo = Math.min(val * 0.65, em.lifetimeS[0] ?? val * 0.5);
            commitOps(
              buildEmitterFieldOps(selectedEmitter, 'lifetimeS', [Number(lo.toFixed(2)), val]),
              `Lifetime ${em.id}`,
              true,
            );
          },
        ),
      );
      inspCard.appendChild(
        makeSliderRow(
          'Size Max',
          'emitter-size-max',
          0.006,
          0.18,
          0.002,
          em.sizeBase[1] ?? 0.035,
          'm',
          (val) => {
            const lo = Number((val * 0.55).toFixed(3));
            commitOps(
              buildEmitterFieldOps(selectedEmitter, 'sizeBase', [lo, val]),
              `Size ${em.id}`,
              true,
            );
          },
        ),
      );

      if (Array.isArray(em.colorOverLife) && em.colorOverLife.length >= 2) {
        const colorRow = el('div', 'gb-vfx-color-row');
        colorRow.appendChild(el('span', 'gb-vfx-row-label', 'Color Stops'));
        const stopsWrap = el('div', 'gb-vfx-color-stops');
        em.colorOverLife.forEach((stop, stopIdx) => {
          const colorInput = el('input', 'gb-vfx-color-input');
          colorInput.type = 'color';
          colorInput.value = rgbaToHex(stop.rgba);
          colorInput.dataset.role = `emitter-color-${stopIdx}`;
          colorInput.addEventListener('input', (ev) => {
            const [r, g, b] = hexToRgb01(ev.target.value);
            const nextStops = em.colorOverLife.map((s, idx) =>
              idx === stopIdx ? { t: s.t, rgba: [r, g, b, s.rgba[3] ?? 1] } : s,
            );
            commitOps(
              buildEmitterFieldOps(selectedEmitter, 'colorOverLife', nextStops),
              `Color stop ${stopIdx} ${em.id}`,
              true,
            );
          });
          stopsWrap.appendChild(colorInput);
        });
        colorRow.appendChild(stopsWrap);
        inspCard.appendChild(colorRow);
      }

      rightCol.appendChild(inspCard);
    }
    grid.appendChild(rightCol);
    root.appendChild(grid);
  };

  syncFromStudio();
  render();

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
    stopPlaybackLoop();
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
    },
    setDocument(pathOrId, rawDoc) {
      activeDocId = pathOrId;
      docData = normalizeVfxDocument(rawDoc?.root ?? rawDoc);
      selectedEmitter = 0;
      const guide = YOZAKURA_VFX_TABLE_GUIDES[docData.id];
      if (guide) previewSeatId = guide.defaultSeatAnchor;
      render();
    },
    onActivate() {
      active = true;
      syncFromStudio();
      render();
    },
    onDeactivate() {
      active = false;
      stopPlaybackLoop();
    },
  };
}

export const createVfxStudioController = mountVfxStudio;

