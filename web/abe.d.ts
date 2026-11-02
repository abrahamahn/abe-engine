// TypeScript declarations for web/abe.js: library mode (`createEngine`), with which a host page
// drives the renderer. Positions are metres; matrices are 16 numbers, column-major, model to
// world. Handles (textures, meshes, materials, instances, models) are nonzero numbers. Every
// engine method throws an Error carrying the engine's message when a call fails.

export type AbeApi = 'auto' | 'webgpu' | 'webgl2';
export type Vec3 = readonly [number, number, number];
export type Rgba = readonly [number, number, number, number?];
export type Matrix4 = ArrayLike<number>;

export interface CreateEngineOptions {
  readonly canvas: HTMLCanvasElement;
  /** URL of abe_web.wasm. */
  readonly wasm: string;
  /** WebGPU where the browser has it, otherwise WebGL 2 (`auto`, the default). */
  readonly api?: AbeApi;
  /** A cooked `.abew` world: URL or bytes. Omit for a scene of props only. */
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
export interface AbeView {
  readonly eye: Vec3;
  readonly target: Vec3;
  readonly up?: Vec3;
  readonly fov?: number;
  readonly shift?: readonly [number, number];
  readonly near?: number;
}

/** RGBA8 pixels, or a canvas, image bitmap or loaded image. */
export type AbeImageSource =
  | { readonly width: number; readonly height: number; readonly data: ArrayLike<number> & ArrayBufferView }
  | HTMLCanvasElement
  | OffscreenCanvas
  | ImageBitmap
  | HTMLImageElement;

export interface AbeMeshData {
  /** Three numbers per vertex. */
  readonly positions: ArrayLike<number>;
  readonly normals?: ArrayLike<number>;
  /** Two numbers per vertex. */
  readonly uvs?: ArrayLike<number>;
  readonly indices: ArrayLike<number>;
}

export interface AbeMaterialDesc {
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
  /** An overlay drawn by a prop shader (`createShader`) instead of in its colour and texture. */
  readonly shader?: number | null;
  /** Up to twenty numbers the prop shader reads (`ShadeInput.p0`..`p4`). */
  readonly params?: readonly number[];
}

export interface AbeModelInfo {
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
export interface AbeLight {
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

export interface AbeNodeTransform {
  readonly translation: Vec3;
  /** Quaternion x, y, z, w. */
  readonly rotation: readonly [number, number, number, number];
  readonly scale: Vec3;
}

export interface AbeEngine {
  readonly api: 'webgpu' | 'webgl2';
  readonly adapter: string;
  errors(): string[];
  /** Sizes the canvas to its CSS box times `dpr` (default: the device pixel ratio). */
  fitCanvas(dpr?: number): void;
  /** The size the last frame was drawn at, in device pixels. */
  size(): { width: number; height: number };
  setLook(look: object | string): void;
  setView(view: AbeView): void;
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

  createTexture(source: AbeImageSource, options?: { readonly srgb?: boolean }): number;
  updateTexture(texture: number, source: AbeImageSource): void;
  destroyTexture(texture: number): void;
  createMesh(mesh: AbeMeshData): number;
  /** New positions (and normals) for a mesh of the same vertex count. */
  updateMesh(mesh: number, data: { readonly positions: ArrayLike<number>; readonly normals?: ArrayLike<number> }): void;
  destroyMesh(mesh: number): void;
  createMaterial(desc: AbeMaterialDesc): number;
  /** Replaces the whole description. */
  updateMaterial(material: number, desc: AbeMaterialDesc): void;
  destroyMaterial(material: number): void;
  /**
   * Compiles a prop shader: WGSL defining `fn shade(in: ShadeInput) -> vec4f`, returning straight
   * linear colour and alpha, which may call the engine's shader functions but reads nothing
   * besides its input (uv, world, normal, tint, color, the base texture's sample `base`, and the
   * material's params as `p0`..`p4`) and its base texture (`base_tex` with `material_sampler`,
   * at any uv or mip bias). It lasts as long as the engine.
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
  createLight(light: AbeLight): number;
  setLight(light: number, desc: AbeLight): void;
  destroyLight(light: number): void;
  /** Where a world point lands in CSS pixels of the canvas, with its depth; null behind the camera. */
  project(point: Vec3): { x: number; y: number; depth: number } | null;
  /** The visible prop under a CSS pixel on one of `layers`. */
  pick(cssX: number, cssY: number, layers?: number): { instance: number; distance: number; point: [number, number, number] } | null;

  /** Loads a GLB; its images are decoded into textures. */
  loadModel(source: string | ArrayBuffer | Uint8Array): Promise<{ model: number; info: AbeModelInfo; textures: number[] }>;
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
  modelNode(instance: number, node: number): AbeNodeTransform;
  setModelNode(instance: number, node: number, transform: AbeNodeTransform): void;
  modelMorph(instance: number, node: number, target: number, weight: number): void;
  /**
   * Applies the pose to what is drawn: node transforms, the joints its skinned meshes follow (on
   * the GPU) and morphs (on the CPU, where a mesh's morph weights changed).
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

export function createEngine(options: CreateEngineOptions): Promise<AbeEngine>;
