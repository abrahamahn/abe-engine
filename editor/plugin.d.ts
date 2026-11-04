/**
 * Ganbate Studio Workbench Plugin, Host Transport & PIE Bridge Declarations (`editor/plugin.d.ts`).
 *
 * 1. **Engine Workbench Modules** (`editor/<workbench>.js`) mount into Ganbate Studio's docked
 *    workspace tab bar via `mount<Workbench>(container, studioApi)` with zero game-specific coupling.
 * 2. **Game Project Extensions** integrate declaratively via `ganbate.project.json`,
 *    `*.schema.json`, `*.data.json` (`Kind::Data`), `<editorDir>/project-tools.json` (docked
 *    `?embed=studio` workspace tabs + external `"open": "tab"` Live Operations), and
 *    `<editorDir>/studio-game.json` (`StudioGameManifest` for Play-in-Editor & Live Release).
 */

export type StudioSchemaEditorHint = 'layout-rig' | 'sequencer' | 'style-tokens';

export type StudioTransportKind = 'local' | 'cloud';

/** Atomic document mutation matching `ganbate_scene::doc::Op`. */
export type StudioDocOp =
  | { set: string[]; value: unknown; remove?: undefined }
  | { remove: string[]; set?: undefined; value?: undefined };

/** Summary of an indexed project asset or Studio JSON document. */
export interface StudioAssetSummary {
  id: string;
  kind: string;
  path: string;
  bytes: number;
}

/** Snapshot of an open or loaded Studio JSON document (`.scene.json`, `.prefab.json`, `.look.json`, `.material.json`, `.data.json`, `.soundbank.json`, `.character.json`, `.seq.json`, `.vfx.json`). */
export interface StudioDocumentSnapshot<TRoot extends Record<string, unknown> = Record<string, unknown>> {
  id: string;
  path: string;
  kind: string;
  rev: number;
  dirty: boolean;
  undo: string | null;
  redo: string | null;
  root: TRoot;
  issues?: Array<{ path: string; message: string }>;
}

export interface StudioViewportStats {
  fps: number;
  frameTimeMs: number;
  gpuTimeMs: number;
  draws: number;
  triangles: number;
  vramBytes: number;
  backend: string;
}

export interface StudioSelectionSnapshot {
  entityIds: string[];
  assetId: string | null;
  activeDocumentId: string | null;
  activeDocumentPath: string | null;
  activeCamera: Record<string, unknown> | null;
  cursorWorld: [number, number, number] | null;
}

export interface StudioStateSnapshot {
  transport: StudioTransportKind;
  connected: boolean;
  locale: 'en' | 'ko';
  activeWorkbench: string;
  project: {
    id: string;
    name: string;
    content: string[];
    schemas: string[];
    editor: string[];
    startup?: string | null;
  } | null;
  assets: StudioAssetSummary[];
  activeDocument: StudioDocumentSnapshot | null;
  selection: StudioSelectionSnapshot;
  stats: StudioViewportStats;
}

export interface StudioViewportBridge {
  element: HTMLElement | null;
  engine: unknown | null;
  getStats(): StudioViewportStats;
  setTime(hours: number): void;
  setWeather(preset: string): void;
  setPreviewLook(look: Record<string, unknown> | null): void;
}

/**
 * Canonical `studioApi` passed to every standalone Studio workbench module:
 * - `mountModelStudio(container, studioApi)` (`editor/model-studio.js`)
 * - `mountMaterialStudio(container, studioApi)` (`editor/material-studio.js`)
 * - `mountCharacterStudio(container, studioApi)` (`editor/character-studio.js`)
 * - `mountSequencerStudio(container, studioApi)` (`editor/sequencer-studio.js`)
 * - `mountVfxStudio(container, studioApi)` (`editor/vfx-studio.js`)
 * - `mountAudioDaw(container, studioApi)` (`editor/audio-daw.js`)
 * - `mountTelemetryProfiler(container, studioApi)` (`editor/telemetry-profiler.js`)
 */
export interface StudioWorkbenchApi {
  getTransportKind(): StudioTransportKind;
  getState(): StudioStateSnapshot;
  getProject(): StudioStateSnapshot['project'];

  /** Returns all indexed project assets and custom Studio JSON documents. */
  getAssets(): StudioAssetSummary[];
  /** Filters project assets/documents by `kind`, file `suffix` (e.g. `".character.json"`), or search `query`. */
  listAssets(filter?: { kind?: string; suffix?: string; query?: string }): StudioAssetSummary[];
  /** Resolves a fetchable URL for a project asset `path` or `id` across both `LocalRustHost` and `CloudStudioHost`. */
  assetUrl(pathOrId: string): string;
  /** Imports a binary or JSON asset into the project. */
  importAsset(
    path: string,
    data: Uint8Array | ArrayBuffer | string,
  ): Promise<{ id: string; path: string; kind: string } | null>;
  /** Triggers a live project rescan on the Studio host. */
  rescanAssets(): Promise<void>;

  /** Returns the currently active document, or a specific open/cached document by asset `id` or project-relative `path`. */
  getDocument(idOrPath?: string): StudioDocumentSnapshot | null;
  getActiveDocument(): StudioDocumentSnapshot | null;
  /** Lists all JSON documents matching an optional `suffix` (e.g. `".soundbank.json"`, `".seq.json"`) or `kind`. */
  listDocuments(filter?: { suffix?: string; kind?: string }): StudioAssetSummary[];
  /** Opens a project document by asset `id` or relative `path` (supports `.scene.json`, `.prefab.json`, `.look.json`, `.material.json`, `.data.json`, `.soundbank.json`, `.character.json`, `.seq.json`, `.vfx.json`). */
  openDocument(idOrPath: string): Promise<StudioDocumentSnapshot | null>;
  /** Creates a new Studio document on the host and opens it. */
  createDocument(options: {
    path: string;
    kind?: string;
    schema?: string;
    name?: string;
    initialRoot?: Record<string, unknown>;
  }): Promise<StudioDocumentSnapshot | null>;
  /** Applies atomic `ganbate_scene::doc` ops (`{ set, value }` / `{ remove }`) with full Studio Undo/Redo & Cloud Draft staging. */
  edit(
    idOrPath: string | null,
    ops: StudioDocOp[],
    label?: string,
    merge?: string,
  ): Promise<StudioDocumentSnapshot | null>;
  editDocument(
    idOrPath: string | null,
    ops: StudioDocOp[],
    label?: string,
    merge?: string,
  ): Promise<StudioDocumentSnapshot | null>;
  undo(idOrPath?: string): Promise<StudioDocumentSnapshot | null>;
  redo(idOrPath?: string): Promise<StudioDocumentSnapshot | null>;
  save(idOrPath?: string, options?: { force?: boolean }): Promise<boolean>;
  saveDocument(idOrPath?: string, options?: { force?: boolean }): Promise<boolean>;

  getSchemas(): Record<string, Record<string, unknown>>;
  getSelection(): StudioSelectionSnapshot;
  setSelection(entityIds: string[]): void;

  /** Returns the live `ganbate` WebGPU/WebGL2 runtime handle when mounted. */
  getEngine(): unknown | null;
  /** Returns the Studio 3D viewport controller and telemetry reader. */
  getViewport(): StudioViewportBridge;

  /** Broadcasts a clock or cue event to any active Play-in-Editor (`PIE`) game frame. */
  sendPieClock(clock: { playing: boolean; time: number; speed?: number }): void;
  sendPieCue(cue: { cueId: string; track: string; payload?: Record<string, unknown> | null }): void;

  /** Sends a raw protocol request over the active `StudioHostTransport` (`LocalRustHost` or `CloudStudioHost`). */
  hostRequest(message: Record<string, unknown>): Promise<Record<string, unknown>>;

  /** Subscribes to full Studio state updates; returns an unsubscribe function. */
  subscribe(listener: (snapshot: StudioStateSnapshot) => void): () => void;
  /** Subscribes to targeted Studio events (`'state' | 'doc' | 'selection' | 'assets' | 'locale' | 'stats'`). */
  on(
    event: 'state' | 'doc' | 'selection' | 'assets' | 'locale' | 'stats',
    handler: (payload: unknown) => void,
  ): () => void;

  /** Switches the docked Studio workspace tab bar to `workbenchId` (optionally focusing a document). */
  activateWorkbench(workbenchId: string, options?: { doc?: string }): Promise<void>;

  /** Logs a status message to the Studio Console (`'info' | 'warn' | 'error'` or `0 | 1 | 2`). */
  notify(level: 'info' | 'warn' | 'error' | 0 | 1 | 2, message: string): void;
  log(level: 0 | 1 | 2, message: string): void;

  /** Translates a UI key using Studio's active locale (`'en' | 'ko'`), falling back to `key` if unknown. */
  t(key: string, ...args: unknown[]): string;
  getLocale(): 'en' | 'ko';
}

export interface StudioWorkbenchInstance {
  destroy?(): void;
  unmount?(): void;
  refresh?(snapshot: StudioStateSnapshot): void;
  setDocument?(pathOrId: string, doc: StudioDocumentSnapshot | null): void;
  onActivate?(): void;
  onDeactivate?(): void;
}

export type StudioWorkbenchMountFn = (
  container: HTMLElement,
  studioApi: StudioWorkbenchApi,
) => StudioWorkbenchInstance | (() => void) | void;

export interface StudioWorkbenchDescriptor {
  id: string;
  label: string;
  /** Module specifier relative to `/editor/` (e.g. `'./character-studio.js'`). */
  moduleUrl?: string;
  /** Named export to invoke (e.g. `'mountCharacterStudio'`), falling back to `mount` or `default`. */
  exportName?: string;
  mount?: StudioWorkbenchMountFn;
  /** File suffixes that automatically route to this workbench when double-clicked in Content Browser. */
  documentSuffixes?: string[];
}

/** Canonical `*.data.json` (`Kind::Data`) document persisted on disk and edited in Ganbate Studio. */
export interface DataDocument<TValues extends Record<string, unknown> = Record<string, unknown>> {
  ganbate: 'data';
  version: 1;
  /** 16-hex-digit Ganbate asset identifier. */
  id: string;
  /** Non-engine namespaced schema identifier (`<namespace>.<kind>`, e.g. `"hud.anchors"`). */
  schema: string;
  /** Human-readable asset name. */
  name: string;
  /** Sparse or materialized schema-validated field values. */
  values: TValues;
}

export interface StudioGamePreset {
  id: string;
  label: string;
  /** Same-origin relative path starting with `/` (e.g. `"/?studioPie=1"`). */
  path: string;
}

export interface StudioGameDevice {
  id: string;
  label: string;
  width: number;
  height: number;
}

export interface StudioLiveReleaseEndpoints {
  statusPath?: string;
  bundlePath?: string;
  stagePath?: string;
  publishPath?: string;
  discardPath?: string;
}

/** `<editorDir>/studio-game.json` manifest (version 1). */
export interface StudioGameManifest {
  version: 1;
  /**
   * Game server origin (HTTPS, or loopback HTTP using a distinct hostname from Studio such as
   * `http://127.0.0.1:5173` while Studio runs on `http://localhost:7331` so Studio's HttpOnly
   * pairing cookie is never shared). When hosted under Option A (`CloudStudioHost` at `/studio`),
   * same-origin relative PIE paths with `?embed=studio` are also supported safely because no local
   * OS filesystem pairing cookie exists.
   */
  origin: string;
  /** Default PIE entry path (e.g. `"/?studioPie=1"`). */
  entry?: string;
  presets?: StudioGamePreset[];
  devices?: StudioGameDevice[];
  liveRelease?: StudioLiveReleaseEndpoints;
}

export interface StudioSyncedDataEntry<TValues = Record<string, unknown>> {
  id: string;
  path: string;
  schema: string;
  name: string;
  rev: number;
  /** Schema-materialized values (with defaults filled). */
  values: TValues;
  /** Raw sparse values stored on the document. */
  rawValues: Record<string, unknown>;
}

export interface StudioSyncedLookEntry {
  id: string;
  path: string;
  rev: number;
  root: Record<string, unknown>;
}

export type StudioToGameMessage =
  | {
      ganbateStudio: 1;
      type: 'studio:sync';
      datas: Record<string, StudioSyncedDataEntry>;
      schemas: Record<string, Record<string, unknown>>;
      looks: Record<string, StudioSyncedLookEntry>;
      scene: { id: string; path: string; rev: number; name: string } | null;
      environment: {
        time: number;
        weather: string;
        look: Record<string, unknown>;
      };
      uiPick: boolean;
    }
  | {
      ganbateStudio: 1;
      type: 'studio:doc-update';
      kind: 'data' | 'look' | 'scene' | 'prefab' | string;
      doc: string;
      path: string;
      rev: number;
      schema: string | null;
      name: string | null;
      values?: Record<string, unknown>;
      rawValues?: Record<string, unknown>;
      root?: Record<string, unknown>;
      ops: Array<{ set?: string[]; value?: unknown; remove?: string[] }> | null;
    }
  | {
      ganbateStudio: 1;
      type: 'studio:clock';
      playing: boolean;
      time: number;
      speed: number;
    }
  | {
      ganbateStudio: 1;
      type: 'studio:cue';
      cueId: string;
      track: string;
      payload: Record<string, unknown> | null;
    }
  | {
      ganbateStudio: 1;
      type: 'studio:ui-pick';
      active: boolean;
    };

export interface StudioRelativeDataOp {
  /** Path relative to `doc.values`, e.g. `["anchors", "hero", "pos"]` or `["cues", "0", "delay"]`. */
  set?: string[];
  value?: unknown;
  /** Path relative to `doc.values` to reset/remove. */
  remove?: string[];
}

export type GameToStudioMessage =
  | {
      ganbateGame: 1;
      type: 'game:ready';
      presets?: StudioGamePreset[];
    }
  | {
      ganbateGame: 1;
      type: 'game:select-data';
      docId?: string;
      schema?: string;
      path?: string[];
    }
  | {
      ganbateGame: 1;
      type: 'game:edit-data';
      docId?: string;
      schema?: string;
      label?: string;
      merge?: string;
      ops: StudioRelativeDataOp[];
    }
  | {
      ganbateGame: 1;
      type: 'game:select-entity';
      entityId: string;
    };
