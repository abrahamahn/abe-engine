// TypeScript declarations for web/ganbate.js: library mode (`createEngine`), with which a host page
// drives the renderer. Positions are metres; matrices are 16 numbers, column-major, model to
// world. Handles (textures, meshes, materials, instances, models) are nonzero numbers. Every
// engine method throws an Error carrying the engine's message when a call fails.

export type GanbateApi = 'auto' | 'webgpu' | 'webgl2';
export type Vec3 = readonly [number, number, number];
export type Rgba = readonly [number, number, number, number?];
export type Matrix4 = ArrayLike<number>;

export interface CreateEngineOptions {
  readonly canvas: HTMLCanvasElement;
  /** URL of ganbate_web.wasm. */
  readonly wasm: string;
  /** WebGPU where the browser has it, otherwise WebGL 2 (`auto`, the default). */
  readonly api?: GanbateApi;
  /** A cooked `.gbew` world: URL or bytes. Omit for a scene of props only. */
  readonly world?: string | ArrayBuffer | Uint8Array;
  /** A Look file, as an object or JSON text. */
  readonly look?: object | string;
  readonly quality?: 'realtime' | 'still';
  /** 0 info, 1 warning, 2 error. */
  readonly onLog?: (level: number, text: string) => void;
  /** Loading phases with progress 0..1. */
  readonly onPhase?: (text: string, progress: number) => void;
}

/** `up` defaults to +Y, `fov` (vertical, degrees) to 50, `shift` (where the target lands, in
 * half-viewports, y up) to none and `near` to 0.08 m. */
export interface GanbateView {
  readonly eye: Vec3;
  readonly target: Vec3;
  readonly up?: Vec3;
  readonly fov?: number;
  readonly shift?: readonly [number, number];
  readonly near?: number;
  /** Depth of field; absent, everything is sharp. */
  readonly focus?: GanbateFocus;
}

/**
 * A lens focused `distance` metres from the eye: nothing nearer blurs, and past it the scene
 * softens, reaching its full blur `depth` metres further on (default 4), `blur` across as a share
 * of the image height (default 0.006, capped at 24 pixels). Overlays stay sharp.
 */
export interface GanbateFocus {
  readonly distance: number;
  readonly depth?: number;
  readonly blur?: number;
}

/** RGBA8 pixels, or a canvas, image bitmap or loaded image. */
export type GanbateImageSource =
  | { readonly width: number; readonly height: number; readonly data: ArrayLike<number> & ArrayBufferView }
  | HTMLCanvasElement
  | OffscreenCanvas
  | ImageBitmap
  | HTMLImageElement;

export interface GanbateMeshData {
  /** Three numbers per vertex. */
  readonly positions: ArrayLike<number>;
  readonly normals?: ArrayLike<number>;
  /** Two numbers per vertex. */
  readonly uvs?: ArrayLike<number>;
  readonly indices: ArrayLike<number>;
}

export interface GanbateMaterialDesc {
  /** Look detail rules match this name. */
  readonly name?: string;
  /** Linear RGBA. */
  readonly color?: readonly [number, number, number, number];
  readonly emissive?: Vec3;
  /** The emission follows the time of day's lamp level. */
  readonly lamp?: boolean;
  readonly roughness?: number;
  readonly metallic?: number;
  readonly shading?: 'standard' | 'foliage' | 'water' | 'unlit';
  /** Alpha-tested at this cutoff. */
  readonly alphaCutoff?: number;
  readonly baseTexture?: number | null;
  readonly normalTexture?: number | null;
  readonly emissiveTexture?: number | null;
  readonly normalScale?: number;
  readonly castsShadow?: boolean;
  readonly blend?: 'opaque' | 'alpha' | 'add';
  /** Drawn over the finished image, unlit, in exactly its colour. */
  readonly overlay?: boolean;
  /** `false`: drawn on top of nearer surfaces. */
  readonly depthTest?: boolean;
  /**
   * An opaque overlay that tests depth also writes it: overlays drawn after it hide behind it where
   * it is nearer (the solid pieces of a close-up drawn wholly in overlays).
   */
  readonly depthWrite?: boolean;
  /** Draw order among blended and overlay props: lower first (fractions slip one between two). */
  readonly order?: number;
  /**
   * An overlay drawn by a prop shader (`createShader`) instead of in its colour and texture; the
   * shader reads its base, normal and emissive textures as pictures to combine as it likes.
   */
  readonly shader?: number | null;
  /** Up to twenty numbers the prop shader reads (`ShadeInput.p0`..`p4`). */
  readonly params?: readonly number[];
}

export interface GanbateModelInfo {
  readonly images: readonly { readonly name: string; readonly mime: string; readonly srgb: boolean; readonly size: number }[];
  readonly nodes: readonly {
    readonly name: string;
    readonly parent: number | null;
    readonly mesh: number | null;
    readonly skin: number | null;
  }[];
  readonly meshes: readonly { readonly name: string; readonly targets: number; readonly targetNames: readonly string[] }[];
  /** Each clip: its name, its length in seconds and the nodes its channels move. */
  readonly clips: readonly {
    readonly name: string;
    readonly duration: number;
    readonly nodes: readonly number[];
  }[];
  /** Material names by glTF index. */
  readonly materials: readonly string[];
}

/**
 * A light a game hangs, shaded like the world's lamps: metres, linear colour, an intensity as a
 * cooked lamp's. A spot shines along `direction`, fully within `inner` radians of it and fading to
 * nothing at `outer`. It shines at every hour unless it `followsLamps` (lit only as the evening
 * lights the lamps). At most 64 at once.
 */
export interface GanbateLight {
  readonly position: Vec3;
  readonly range: number;
  readonly color: Vec3;
  readonly intensity: number;
  readonly direction?: Vec3;
  readonly inner?: number;
  readonly outer?: number;
  readonly followsLamps?: boolean;
  /**
   * How much of a spot's light the props' shadows take: 0 (the default) casts none, 1 leaves the
   * shadows without it. The opaque props whose materials cast shadows block it; the world does
   * not. At most 2 lights cast shadows at once, none reaching wider than 1.4 radians out.
   */
  readonly shadow?: number;
}

export interface GanbateNodeTransform {
  readonly translation: Vec3;
  /** Quaternion x, y, z, w. */
  readonly rotation: readonly [number, number, number, number];
  readonly scale: Vec3;
}

/** One authored entity of a cooked scene: its instances and lamps are runs of the world's. */
export interface GanbateEntityRecord {
  /** 16 hexadecimal digits, as in the scene file. */
  readonly id: string;
  readonly name: string;
  /** Index of the parent record (parents come first). */
  readonly parent: number | null;
  /** World matrix when cooked; moving the entity to `m` moves each of its instances `i` to `m * transform⁻¹ * i`. */
  readonly transform: readonly number[];
  readonly instances: readonly [number, number];
  readonly lights: readonly [number, number];
}

export interface GanbateEntityTable {
  readonly entities: readonly GanbateEntityRecord[];
  /** The components the renderer does not draw, by entity id, and the scene's id and name. */
  readonly data: { readonly entities: Record<string, { readonly components: Record<string, unknown> }>; readonly scene?: string; readonly name?: string } | null;
}

export interface GanbateEngine {
  readonly api: 'webgpu' | 'webgl2';
  readonly adapter: string;
  errors(): string[];
  /** Sizes the canvas to its CSS box times `dpr` (default: the device pixel ratio). */
  fitCanvas(dpr?: number): void;
  /** The size the last frame was drawn at, in device pixels. */
  size(): { width: number; height: number };
  setLook(look: object | string): void;
  setView(view: GanbateView): void;
  /** Time of day in hours and the animation clock in seconds. */
  setClock(hours: number, seconds?: number): void;
  /** The world's lamps `boost` times as bright as the hour lights them, glow and light alike (1: as it does). */
  setLampBoost(boost: number): void;
  setWeather(preset: string): void;
  /** Draws and presents a frame; false when it skipped (GPU busy). */
  render(): boolean;
  frames(): number;
  /** The last frame's pixels (RGBA rows, top first). */
  capture(): Promise<{ width: number; height: number; rgba: Uint8ClampedArray }>;
  /** Waits for submitted GPU work (WebGPU; immediate on WebGL 2). */
  idle(): Promise<void>;

  createTexture(source: GanbateImageSource, options?: { readonly srgb?: boolean }): number;
  updateTexture(texture: number, source: GanbateImageSource): void;
  destroyTexture(texture: number): void;
  createMesh(mesh: GanbateMeshData): number;
  /** New positions (and normals) for a mesh of the same vertex count. */
  updateMesh(mesh: number, data: { readonly positions: ArrayLike<number>; readonly normals?: ArrayLike<number> }): void;
  destroyMesh(mesh: number): void;
  createMaterial(desc: GanbateMaterialDesc): number;
  /** Replaces the whole description. */
  updateMaterial(material: number, desc: GanbateMaterialDesc): void;
  destroyMaterial(material: number): void;
  /**
   * Compiles a prop shader: WGSL defining `fn shade(in: ShadeInput) -> vec4f`, returning straight
   * linear colour and alpha, which may call the engine's shader functions but reads nothing
   * besides its input (uv, world, normal, tint, color, the material's textures sampled at uv as
   * `base`, `normal_map` and `emissive_map`, and its params as `p0`..`p4`) and those textures
   * (`base_tex`, `normal_tex`, `emissive_tex` with `material_sampler`, at any uv or mip bias). It
   * lasts as long as the engine.
   */
  createShader(source: string): number;
  /** New params for a material drawn by a prop shader, without a rebuild. */
  setMaterialParams(material: number, params: readonly number[]): void;
  createInstance(mesh: number, material: number, matrix: Matrix4): number;
  setInstance(instance: number, matrix: Matrix4, visible?: boolean): void;
  setInstances(list: readonly { readonly instance: number; readonly matrix: Matrix4; readonly visible?: boolean }[]): void;
  setInstanceMaterial(instance: number, material: number): void;
  /** Multiplies the material's colour and alpha. */
  setInstanceTint(instance: number, tint: Rgba): void;
  /** Pick layers (bit mask); 0: never picked. New instances: 1. */
  setInstancePick(instance: number, layers: number): void;
  destroyInstance(instance: number): void;
  /** Hangs a light (a point light, or a spot with a `direction`); returns its handle. */
  createLight(light: GanbateLight): number;
  setLight(light: number, desc: GanbateLight): void;
  destroyLight(light: number): void;
  /** Where a world point lands in CSS pixels of the canvas, with its depth; null behind the camera. */
  project(point: Vec3): { x: number; y: number; depth: number } | null;
  /** The visible prop under a CSS pixel on one of `layers`. */
  pick(cssX: number, cssY: number, layers?: number): { instance: number; distance: number; point: [number, number, number] } | null;

  /** How many instances the cooked world has. */
  worldInstances(): number;
  /** World instance `index`'s matrix and whether it is shown. */
  worldInstance(index: number): { matrix: number[]; visible: boolean };
  /**
   * Moves world instance `index` (an object of a cooked scene; `worldEntities` lists each
   * entity's instances) and shows or hides it. It stays lit, shadowed and mapped like the rest
   * of the world.
   */
  setWorldInstance(index: number, matrix: Matrix4, visible?: boolean): void;
  /** Moves world lamp `index` (a cooked point light). */
  setWorldLight(index: number, position: Vec3): void;
  /**
   * Starts (or restarts) simulating the cooked world's physics (its `rigid_body` and `collider`
   * components) from the authored state. Returns false when the world has none, or when they
   * could not be built: `physicsError()` then says why.
   */
  physicsStart(): boolean;
  /**
   * Steps the simulation `steps` times (default 1, at most 64 a call) by `dt` seconds each (at
   * most 1/15), then moves the instances and lamps of the bodies that moved (with their
   * children's). Sleeping and still bodies cost nothing; several steps in one call move things
   * once. Steps past 64 are not run, so a host that fell further behind (or passes a frame count
   * or milliseconds) loses that time instead of freezing the page while it catches up.
   */
  physicsStep(dt: number, steps?: number): boolean;
  /** Stops simulating and puts every moved instance and lamp back where it was cooked. */
  physicsStop(): void;
  /** Why the world's physics components could not be built (the entity named), or null. */
  physicsError(): string | null;
  /** The simulation's state hash as 16 hexadecimal digits (equal to a native run's), or null. */
  physicsHash(): string | null;
  /** The cooked world's instance under a CSS pixel of the canvas, or null. */
  pickWorld(cssX: number, cssY: number): { instance: number; distance: number; point: [number, number, number] } | null;
  /** The world's entity table, or null when it was cooked without one. */
  worldEntities(): GanbateEntityTable | null;

  /** Loads a GLB; its images are decoded into textures. */
  loadModel(source: string | ArrayBuffer | Uint8Array): Promise<{ model: number; info: GanbateModelInfo; textures: number[] }>;
  /** Destroys a model and its instances; its textures stay. */
  destroyModel(model: number): void;
  /** The material made for the model's glTF material `index`. */
  modelMaterial(model: number, index: number): number;
  createModelInstance(model: number, matrix: Matrix4): number;
  setModelInstance(instance: number, matrix: Matrix4, visible?: boolean): void;
  destroyModelInstance(instance: number): void;
  modelRest(instance: number): void;
  /** Mixes clip `clip` at `time` seconds into the pose; weight 1 replaces what it animates. */
  modelClip(instance: number, clip: number, time: number, weight?: number): void;
  modelNode(instance: number, node: number): GanbateNodeTransform;
  setModelNode(instance: number, node: number, transform: GanbateNodeTransform): void;
  modelMorph(instance: number, node: number, target: number, weight: number): void;
  /**
   * Applies the pose to what is drawn: node transforms, the joints its skinned meshes follow (on
   * the GPU) and morphs (on the CPU, where the weight of a target that moves a mesh's vertices
   * changed).
   */
  updateModel(instance: number): void;
  /** A node's world matrix as of the last update. */
  modelNodeWorld(instance: number, node: number): Float32Array;
  setModelNodeVisible(instance: number, node: number, visible: boolean): void;
  setModelTint(instance: number, tint: Rgba): void;
  /** The model instance and node a picked prop instance draws, or null. */
  modelOwner(propInstance: number): { instance: number; node: number } | null;
  /** Releases the engine's GPU objects. */
  destroy(): void;
}

export function createEngine(options: CreateEngineOptions): Promise<GanbateEngine>;
